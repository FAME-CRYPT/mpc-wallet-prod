//! MPC Wallet API Server
//!
//! Production-ready API server for the distributed threshold wallet system.
//! Provides REST endpoints for wallet operations, transaction management,
//! and cluster monitoring.

use anyhow::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use threshold_api::{start_server, AppState};
use threshold_storage::{PostgresStorage, EtcdStorage};
use threshold_bitcoin::{BitcoinClient, BitcoinNetwork};
use threshold_types::PostgresConfig;
use threshold_consensus::VoteProcessor;
use protocols::p2p::{P2pSessionCoordinator, QuicTransport};
use protocols::p2p::certs::{NodeCertificate, StoredNodeCert};
use threshold_orchestrator::{
    OrchestrationService, OrchestrationServiceBuilder,
    TimeoutMonitor, TimeoutMonitorBuilder,
    HealthChecker, HealthCheckerBuilder,
    OrchestrationConfig,
    DkgService,
    AuxInfoService,
};
use threshold_network::{QuicEngine, PeerRegistry};
use tracing::{info, error, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use tokio::signal;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize rustls crypto provider (required for QUIC/mTLS)
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("Failed to install default crypto provider"))?;

    // Initialize tracing/logging
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    info!("Starting MPC Wallet API Server");

    // Load configuration from environment
    let config = load_config()?;

    // Initialize PostgreSQL storage
    info!("Connecting to PostgreSQL at {}", mask_password(&config.postgres_config.url));
    let postgres = Arc::new(PostgresStorage::new(&config.postgres_config).await?);
    info!("PostgreSQL storage initialized");

    // Initialize etcd storage (with Mutex for interior mutability)
    info!("Connecting to etcd cluster: {:?}", config.etcd_endpoints);
    let etcd = Arc::new(tokio::sync::Mutex::new(EtcdStorage::new(config.etcd_endpoints.clone()).await?));
    info!("etcd storage initialized");

    // Initialize Bitcoin client
    info!("Initializing Bitcoin client for {:?}", config.bitcoin_network);
    let bitcoin = Arc::new(BitcoinClient::new(config.bitcoin_network)?);
    info!("Bitcoin client initialized");

    // Orchestration components will be initialized inside the orchestration block
    // to avoid ownership issues with storage backends

    // Create application state with new storage instances
    // AppState::new takes ownership, so we create fresh instances
    let postgres_for_state = PostgresStorage::new(&config.postgres_config).await?;
    let etcd_for_state = EtcdStorage::new(config.etcd_endpoints.clone()).await?;
    let bitcoin_for_state = BitcoinClient::new(config.bitcoin_network)?;

    // Create QuicEngine for DKG service
    // Note: For API server, we create a minimal QuicEngine without full transport setup
    // The DKG service will use this for network communication
    let registry_url = config.registry_url.clone().unwrap_or_else(|| "http://mpc-coordinator:3000".to_string());
    let peer_registry = Arc::new(PeerRegistry::new(
        threshold_types::NodeId(config.node_id),
        Some(registry_url.clone()),
    ));
    let mut quic_engine = QuicEngine::new(
        threshold_types::NodeId(config.node_id),
        peer_registry,
        None, // mTLS config will be added later if needed
    );
    // Initialize client endpoint for outgoing connections
    quic_engine.init_client().await?;
    let quic_engine = Arc::new(quic_engine);
    info!("QUIC engine initialized for DKG service");

    // Create message router for protocol communication
    let message_router = Arc::new(threshold_orchestrator::MessageRouter::new(
        Arc::clone(&quic_engine),
        threshold_types::NodeId(config.node_id),
    ));
    info!("Message router initialized");

    // Create DKG service
    let dkg_service = DkgService::new(
        Arc::clone(&postgres),
        Arc::clone(&etcd),
        Arc::clone(&quic_engine),
        Arc::clone(&message_router),
        threshold_types::NodeId(config.node_id),
    );
    info!("DKG service initialized");

    // Create Aux Info service
    let aux_info_service = AuxInfoService::new(
        Arc::clone(&postgres),
        Arc::clone(&etcd),
        Arc::clone(&quic_engine),
        Arc::clone(&message_router),
        threshold_types::NodeId(config.node_id),
    );
    info!("Aux info service initialized");

    let state = AppState::new(postgres_for_state, etcd_for_state, bitcoin_for_state, dkg_service, aux_info_service);

    // Parse listen address
    let addr: SocketAddr = config.listen_addr.parse()?;

    info!("Server configuration:");
    info!("  Node ID: {}", config.node_id);
    info!("  Listen Address: {}", addr);
    info!("  Threshold: {}/{}", config.threshold, config.total_nodes);
    info!("  Bitcoin Network: {:?}", config.bitcoin_network);
    info!("  Orchestration: {}", if config.enable_orchestration { "enabled" } else { "disabled" });

    // Start orchestration if enabled
    let orchestrator_handle = if config.enable_orchestration {
        info!("Starting orchestration services...");

        // Configure orchestration
        let orchestration_config = OrchestrationConfig::default();

        // Initialize VoteProcessor with cloned storage (VoteProcessor takes ownership)
        // We need to create new EtcdStorage and PostgresStorage instances
        let etcd_for_vp = EtcdStorage::new(config.etcd_endpoints.clone()).await?;
        let postgres_for_vp = PostgresStorage::new(&config.postgres_config).await?;
        let vote_processor = Arc::new(VoteProcessor::new(etcd_for_vp, postgres_for_vp));
        info!("Vote processor initialized for orchestration");

        // Initialize P2P session coordinator with QUIC transport
        use ed25519_dalek::SigningKey;
        let dummy_signing_key = SigningKey::from_bytes(&[1u8; 32]);
        let grant_verifying_key = dummy_signing_key.verifying_key();
        let party_index = config.node_id as u16;

        // Load certificates for QUIC/mTLS from existing PEM files
        // Note: node_id (1-5) matches certificate file numbers directly
        let node_cert = load_node_cert_from_pem(config.node_id as u16).await?;
        info!("Node certificate loaded for node {} from /certs/", config.node_id);

        // Create QUIC transport with mTLS
        let registry_url = config.registry_url.clone().unwrap_or_else(|| "http://mpc-coordinator:3000".to_string());
        let quic_listen_addr = config.quic_listen_addr.parse()?;
        let quic_transport = Arc::new(QuicTransport::new(
            party_index,
            &registry_url,
            node_cert,
            quic_listen_addr,
            Some(config.quic_port),
        )?);

        // Initialize QUIC transport (start listener)
        quic_transport.init().await?;
        info!("QUIC transport initialized on {}", quic_listen_addr);

        let session_coordinator = Arc::new(P2pSessionCoordinator::new(
            party_index,
            grant_verifying_key,
        ));
        info!("P2P session coordinator initialized with QUIC/mTLS");

        // Start health checker
        let health_checker = HealthCheckerBuilder::new()
            .with_nodes(config.node_endpoints.clone())
            .with_etcd(Arc::clone(&etcd))
            .build()?;
        let health_handle = Arc::clone(&health_checker).start();
        info!("Health checker started");

        // Start timeout monitor
        let timeout_monitor = TimeoutMonitorBuilder::new()
            .with_config(orchestration_config.clone())
            .with_postgres(Arc::clone(&postgres))
            .build()?;
        let timeout_handle = Arc::clone(&timeout_monitor).start();
        info!("Timeout monitor started");

        // Start orchestration service
        let orchestrator = OrchestrationServiceBuilder::new()
            .with_config(orchestration_config)
            .with_vote_processor(Arc::clone(&vote_processor))
            .with_session_coordinator(Arc::clone(&session_coordinator))
            .with_postgres(Arc::clone(&postgres))
            .with_etcd(Arc::clone(&etcd))
            .with_bitcoin(Arc::clone(&bitcoin))
            .build()?;
        let orchestrator_handle = Arc::clone(&orchestrator).start();
        info!("Orchestration service started");

        Some((orchestrator, timeout_monitor, health_checker, quic_transport, orchestrator_handle, timeout_handle, health_handle))
    } else {
        warn!("Orchestration disabled - transactions will not be automatically processed");
        None
    };

    // Start the API server in a separate task
    info!("API server starting on {}", addr);
    let server_handle = tokio::spawn(async move {
        if let Err(e) = start_server(state, addr).await {
            error!("Server error: {}", e);
        }
    });

    // Wait for shutdown signal
    info!("Server running. Press Ctrl+C to shutdown.");
    match signal::ctrl_c().await {
        Ok(()) => {
            info!("Shutdown signal received");
        }
        Err(err) => {
            error!("Unable to listen for shutdown signal: {}", err);
        }
    }

    // Graceful shutdown
    if let Some((orchestrator, timeout_monitor, health_checker, quic_transport, orch_handle, timeout_handle, health_handle)) = orchestrator_handle {
        info!("Shutting down orchestration services...");
        orchestrator.shutdown().await;
        timeout_monitor.shutdown().await;
        health_checker.shutdown().await;

        // Shutdown QUIC transport
        info!("Shutting down QUIC transport...");
        quic_transport.shutdown().await;

        // Wait for services to stop (with timeout)
        let shutdown_timeout = tokio::time::Duration::from_secs(10);
        tokio::select! {
            _ = orch_handle => info!("Orchestration service stopped"),
            _ = tokio::time::sleep(shutdown_timeout) => warn!("Orchestration service shutdown timed out"),
        }
        tokio::select! {
            _ = timeout_handle => info!("Timeout monitor stopped"),
            _ = tokio::time::sleep(shutdown_timeout) => warn!("Timeout monitor shutdown timed out"),
        }
        tokio::select! {
            _ = health_handle => info!("Health checker stopped"),
            _ = tokio::time::sleep(shutdown_timeout) => warn!("Health checker shutdown timed out"),
        }
    }

    // Stop API server
    server_handle.abort();
    info!("API server stopped");

    info!("Shutdown complete");
    Ok(())
}

#[derive(Debug)]
struct Config {
    node_id: u64,
    listen_addr: String,
    postgres_config: PostgresConfig,
    etcd_endpoints: Vec<String>,
    threshold: u32,
    total_nodes: u32,
    bitcoin_network: BitcoinNetwork,
    enable_orchestration: bool,
    node_endpoints: Vec<(u64, String)>,
    // QUIC/mTLS configuration
    quic_listen_addr: String,
    quic_port: u16,
    registry_url: Option<String>,
}

fn load_config() -> Result<Config> {
    let node_id = std::env::var("NODE_ID")
        .unwrap_or_else(|_| "1".to_string())
        .parse::<u64>()?;

    let listen_addr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string());

    let postgres_url = std::env::var("POSTGRES_URL")
        .map_err(|_| anyhow::anyhow!("POSTGRES_URL environment variable is required"))?;

    let postgres_config = PostgresConfig {
        url: postgres_url,
        max_connections: 10,
        connect_timeout_secs: 30,
    };

    let etcd_endpoints_str = std::env::var("ETCD_ENDPOINTS")
        .map_err(|_| anyhow::anyhow!("ETCD_ENDPOINTS environment variable is required"))?;

    let etcd_endpoints: Vec<String> = etcd_endpoints_str
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    let threshold = std::env::var("THRESHOLD")
        .unwrap_or_else(|_| "4".to_string())
        .parse::<u32>()?;

    let total_nodes = std::env::var("TOTAL_NODES")
        .unwrap_or_else(|_| "5".to_string())
        .parse::<u32>()?;

    let bitcoin_network_str = std::env::var("BITCOIN_NETWORK")
        .unwrap_or_else(|_| "testnet".to_string());
    let bitcoin_network = BitcoinNetwork::parse(&bitcoin_network_str);

    let enable_orchestration = std::env::var("ENABLE_ORCHESTRATION")
        .unwrap_or_else(|_| "true".to_string())
        .parse::<bool>()
        .unwrap_or(true);

    // Parse node endpoints for health checking
    let node_endpoints_str = std::env::var("NODE_ENDPOINTS").unwrap_or_default();
    let node_endpoints: Vec<(u64, String)> = if !node_endpoints_str.is_empty() {
        node_endpoints_str
            .split(';')
            .filter_map(|entry| {
                let parts: Vec<&str> = entry.split('=').collect();
                if parts.len() == 2 {
                    if let Ok(id) = parts[0].parse::<u64>() {
                        return Some((id, parts[1].to_string()));
                    }
                }
                None
            })
            .collect()
    } else {
        // Default: generate endpoints for all nodes
        (1..=total_nodes)
            .map(|id| (id as u64, format!("http://mpc-node-{}:8080", id)))
            .collect()
    };

    // QUIC/mTLS configuration
    let quic_listen_addr = std::env::var("QUIC_LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:4001".to_string());

    let quic_port = std::env::var("QUIC_PORT")
        .unwrap_or_else(|_| "4001".to_string())
        .parse::<u16>()?;

    let registry_url = std::env::var("REGISTRY_URL").ok();

    Ok(Config {
        node_id,
        listen_addr,
        postgres_config,
        etcd_endpoints,
        threshold,
        total_nodes,
        bitcoin_network,
        enable_orchestration,
        node_endpoints,
        quic_listen_addr,
        quic_port,
        registry_url,
    })
}

fn mask_password(url: &str) -> String {
    if let Some(at_pos) = url.rfind('@') {
        if let Some(colon_pos) = url[..at_pos].rfind(':') {
            let mut masked = url.to_string();
            masked.replace_range(colon_pos + 1..at_pos, "****");
            return masked;
        }
    }
    url.to_string()
}

/// Load node certificate from existing PEM files in /certs/ directory.
///
/// This function loads certificates generated by the production certificate
/// generation scripts (generate-certs.sh) which use OpenSSL to create
/// CA and node certificates in standard PEM format.
///
/// Expected files:
/// - /certs/ca.crt - Root CA certificate
/// - /certs/node{N}.crt - Node certificate (where N is node_id, 1-5)
/// - /certs/node{N}.key - Node private key
///
/// # Arguments
/// * `node_id` - Node ID (1-5), matches certificate file numbers
async fn load_node_cert_from_pem(node_id: u16) -> Result<NodeCertificate> {
    use tokio::fs;

    let ca_cert_path = "/certs/ca.crt";
    let node_cert_path = format!("/certs/node{}.crt", node_id);
    let node_key_path = format!("/certs/node{}.key", node_id);

    info!("Loading certificates from: ca={}, cert={}, key={}",
          ca_cert_path, node_cert_path, node_key_path);

    // Read PEM files
    let ca_cert_pem = fs::read_to_string(ca_cert_path).await
        .map_err(|e| anyhow::anyhow!("Failed to read CA certificate from {}: {}", ca_cert_path, e))?;
    let cert_pem = fs::read_to_string(&node_cert_path).await
        .map_err(|e| anyhow::anyhow!("Failed to read node certificate from {}: {}", node_cert_path, e))?;
    let key_pem = fs::read_to_string(&node_key_path).await
        .map_err(|e| anyhow::anyhow!("Failed to read node key from {}: {}", node_key_path, e))?;

    // Create StoredNodeCert structure
    // Note: party_index is 0-indexed (node_id - 1)
    let party_index = node_id - 1;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let stored_cert = StoredNodeCert {
        party_index,
        cert_pem,
        key_pem,
        ca_cert_pem,
        created_at: now,
    };

    // Convert to NodeCertificate
    let node_cert = NodeCertificate::from_stored(&stored_cert)?;

    info!("Successfully loaded certificates for node {} (party_index {})", node_id, party_index);

    Ok(node_cert)
}
