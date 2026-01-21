//! MPC Wallet API Server
//!
//! Production-ready API server for the distributed threshold wallet system.
//! Provides REST endpoints for wallet operations, transaction management,
//! and cluster monitoring.

use anyhow::Result;
use std::net::SocketAddr;
use threshold_api::{start_server, AppState};
use threshold_storage::{PostgresStorage, EtcdStorage};
use threshold_bitcoin::{BitcoinClient, BitcoinNetwork};
use threshold_types::PostgresConfig;
use tracing::{info, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
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
    let postgres = PostgresStorage::new(&config.postgres_config).await?;
    info!("PostgreSQL storage initialized");

    // Initialize etcd storage
    info!("Connecting to etcd cluster: {:?}", config.etcd_endpoints);
    let etcd = EtcdStorage::new(config.etcd_endpoints).await?;
    info!("etcd storage initialized");

    // Initialize Bitcoin client
    info!("Initializing Bitcoin client for {:?}", config.bitcoin_network);
    let bitcoin = BitcoinClient::new(config.bitcoin_network)?;
    info!("Bitcoin client initialized");

    // Create application state
    let state = AppState::new(postgres, etcd, bitcoin);

    // Parse listen address
    let addr: SocketAddr = config.listen_addr.parse()?;

    info!("Server configuration:");
    info!("  Node ID: {}", config.node_id);
    info!("  Listen Address: {}", addr);
    info!("  Threshold: {}/{}", config.threshold, config.total_nodes);
    info!("  Bitcoin Network: {:?}", config.bitcoin_network);

    // Start the server
    info!("API server starting on {}", addr);

    if let Err(e) = start_server(state, addr).await {
        error!("Server error: {}", e);
        return Err(e);
    }

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

    Ok(Config {
        node_id,
        listen_addr,
        postgres_config,
        etcd_endpoints,
        threshold,
        total_nodes,
        bitcoin_network,
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
