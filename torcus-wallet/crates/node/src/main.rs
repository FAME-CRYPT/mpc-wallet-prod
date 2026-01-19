//! MPC Node
//!
//! A node that participates in MPC wallet operations using threshold ECDSA.
//! Each node holds a share of the private key and participates in
//! distributed key generation and signing protocols.

mod certs;
mod handlers;
mod p2p_init;
mod p2p_listener;
mod registration;
mod session;
mod state;

use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use clap::Parser;
use tokio::sync::RwLock;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::state::NodeState;
use common::{NUM_PARTIES, THRESHOLD};
use ed25519_dalek::VerifyingKey;

/// Coordinator URL (used to fetch grant public key).
const COORDINATOR_URL: &str = "http://mpc-coordinator:3000";

/// Fetch the grant verifying key from the coordinator.
async fn fetch_grant_pubkey() -> Option<VerifyingKey> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    let url = format!("{}/grant/pubkey", COORDINATOR_URL);
    info!("Fetching grant public key from {}", url);

    match client.get(&url).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    if let Some(pubkey_hex) = json["public_key"].as_str() {
                        if let Ok(bytes) = hex::decode(pubkey_hex) {
                            if bytes.len() == 32 {
                                let bytes_arr: [u8; 32] = bytes.try_into().unwrap();
                                match VerifyingKey::from_bytes(&bytes_arr) {
                                    Ok(key) => {
                                        info!(
                                            "Successfully fetched grant public key: {}",
                                            pubkey_hex
                                        );
                                        return Some(key);
                                    }
                                    Err(e) => {
                                        error!("Invalid grant public key: {}", e);
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                error!("Failed to fetch grant pubkey: HTTP {}", resp.status());
            }
        }
        Err(e) => {
            error!("Failed to connect to coordinator for grant pubkey: {}", e);
        }
    }

    None
}

/// MPC Node CLI arguments.
#[derive(Parser, Debug)]
#[command(name = "mpc-node")]
#[command(author, version, about = "MPC Wallet Node")]
struct Args {
    /// Port to listen on.
    #[arg(short, long, default_value = "3000")]
    port: u16,

    /// Party index (1-based, will be converted to 0-based internally).
    #[arg(long, default_value = "1", value_parser = clap::value_parser!(u16).range(1..=NUM_PARTIES as i64))]
    party_index: u16,

    /// Skip pregenerated primes initialization (faster startup, but CGGMP24 won't work).
    /// Can also be set via SKIP_PRIMES=true environment variable.
    #[arg(long, env = "SKIP_PRIMES", default_value = "false")]
    skip_primes: bool,

    /// Force regeneration of primes even if they exist.
    #[arg(long, env = "FORCE_PRIMES", default_value = "false")]
    force_primes: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize tracing with RUST_LOG environment variable
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("debug".parse().unwrap()),
        )
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    info!("========================================");
    info!("  MPC Node Starting");
    info!("========================================");
    debug!("CLI arguments: {:?}", args);

    let node_id = Uuid::new_v4();

    // Validate party index (must be 1..=NUM_PARTIES for 1-based input)
    if args.party_index == 0 || args.party_index > NUM_PARTIES {
        let message = format!(
            "--party-index must be between 1 and {} (got {})",
            NUM_PARTIES, args.party_index
        );
        error!("{}", message);
        anyhow::bail!(message);
    }

    // Convert to 0-based index for internal use.
    let party_index = args.party_index - 1;

    info!("Node configuration:");
    info!("  - Node ID: {}", node_id);
    info!(
        "  - Party Index: {} (1-based: {})",
        party_index, args.party_index
    );
    info!("  - Port: {}", args.port);
    info!("  - Threshold: {}-of-{}", THRESHOLD, NUM_PARTIES);

    // Initialize pregenerated primes (for CGGMP24)
    info!("Initializing pregenerated primes...");
    let primes_data = if args.skip_primes {
        info!("Skipping prime generation (--skip-primes flag or SKIP_PRIMES=true)");
        None
    } else {
        use protocols::cggmp24::primes::{initialize_primes, PrimeInitResult};
        match initialize_primes(party_index, None, args.force_primes) {
            PrimeInitResult::Loaded(data) => {
                info!("Loaded existing primes ({} bytes)", data.len());
                Some(data)
            }
            PrimeInitResult::Generated(data) => {
                info!("Generated new primes ({} bytes)", data.len());
                Some(data)
            }
            PrimeInitResult::Failed(e) => {
                error!("Failed to initialize primes: {}", e);
                error!("Node will start without primes - CGGMP24 signing will not work");
                None
            }
        }
    };

    debug!("Creating node state...");
    let state = Arc::new(RwLock::new(NodeState::new_with_primes(
        node_id,
        party_index,
        primes_data,
    )));
    debug!("Node state created");

    // Fetch grant public key from coordinator (with retries)
    // This is REQUIRED for signing authorization - node cannot proceed without it
    info!("Fetching grant public key from coordinator...");
    let mut grant_key: Option<VerifyingKey> = None;
    for attempt in 1..=10 {
        if let Some(key) = fetch_grant_pubkey().await {
            state.write().await.set_grant_verifying_key(key);
            grant_key = Some(key);
            break;
        }
        if attempt < 10 {
            info!(
                "Retrying grant key fetch in 3 seconds (attempt {}/10)...",
                attempt
            );
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
    }
    let grant_key = match grant_key {
        Some(key) => key,
        None => {
            error!("========================================");
            error!("  FATAL: Failed to fetch grant public key");
            error!("========================================");
            error!("The node cannot verify signing grants without the coordinator's public key.");
            error!(
                "This is a security requirement - signing without grant verification is not allowed."
            );
            error!(
                "Please ensure the coordinator is running and accessible at {}",
                COORDINATOR_URL
            );
            anyhow::bail!(
                "Failed to fetch grant public key from coordinator - cannot start without it"
            );
        }
    };

    // Spawn background session cleanup task
    let _cleanup_handle = session::spawn_cleanup_task(state.clone());

    // Register with coordinator for node discovery (MUST happen before P2P init)
    // Registration provides the cert_token needed for certificate issuance
    info!("Registering with coordinator for node discovery...");
    let heartbeat_interval =
        match registration::register_with_coordinator(state.clone(), COORDINATOR_URL, args.port)
            .await
        {
            Ok(result) => {
                info!("Successfully registered with coordinator");
                // Store cert_token if this is a new registration
                if let Some(token) = result.cert_token {
                    state.write().await.set_cert_token(token);
                }
                result.heartbeat_interval_secs
            }
            Err(e) => {
                error!("Failed to register with coordinator: {}", e);
                error!("Node will continue without registration (discovery disabled)");
                registration::DEFAULT_HEARTBEAT_INTERVAL_SECS
            }
        };

    // Spawn heartbeat task to maintain registration
    let _heartbeat_handle = registration::spawn_heartbeat_task(
        state.clone(),
        COORDINATOR_URL.to_string(),
        heartbeat_interval,
    );

    // Initialize P2P infrastructure (optional - falls back to HTTP relay if disabled/failed)
    // MUST happen after registration because it requires cert_token for certificate issuance
    let p2p_ready = if p2p_init::is_p2p_enabled_env() {
        info!("P2P mode is enabled, initializing...");
        let quic_port = p2p_init::get_quic_port_env();
        let ready = p2p_init::try_init_p2p(
            state.clone(),
            COORDINATOR_URL,
            grant_key,
            Some(quic_port),
            3, // max retries
        )
        .await;

        // If P2P is ready, spawn the control message listener
        if ready {
            if let Some((coordinator, transport)) = state.read().await.p2p_components() {
                let _listener_handle = p2p_listener::spawn_control_message_listener(
                    state.clone(),
                    coordinator,
                    transport,
                );
                info!("P2P control message listener spawned");
            }
        }

        ready
    } else {
        info!("P2P mode is disabled (P2P_ENABLED=false)");
        false
    };

    debug!("Building router...");
    let app = Router::new()
        .route("/health", get(handlers::health_check))
        .route("/status", get(handlers::full_status))
        // CGGMP24 infrastructure
        .route("/primes/status", get(handlers::primes_status))
        .route("/primes/regenerate", post(handlers::regenerate_primes))
        .route("/aux-info/status", get(handlers::aux_info_status))
        .route("/aux-info/start", post(handlers::start_aux_info))
        // CGGMP24 keygen and signing (Native SegWit / ECDSA)
        .route(
            "/cggmp24/keygen/start",
            post(handlers::start_cggmp24_keygen),
        )
        .route("/cggmp24/sign", post(handlers::cggmp24_sign_sync))
        .route("/cggmp24/keyshares", get(handlers::list_cggmp24_keyshares))
        // FROST keygen and signing (Taproot / Schnorr)
        .route("/frost/keygen/start", post(handlers::start_frost_keygen))
        .route("/frost/sign", post(handlers::frost_sign_sync))
        .route("/frost/keyshares", get(handlers::list_frost_keyshares))
        // P2P session coordination (Phase 4)
        .route("/p2p/sign", post(handlers::p2p_sign))
        .route(
            "/p2p/session/{session_id}",
            get(handlers::get_p2p_session_status),
        )
        .with_state(state.clone());
    debug!("Router built successfully");

    let addr = format!("0.0.0.0:{}", args.port);

    info!("Endpoints:");
    info!("  GET  /health                  - Health check");
    info!("  GET  /status                  - Full node status");
    info!("  GET  /primes/status           - Check primes status");
    info!("  POST /primes/regenerate       - Regenerate primes");
    info!("  GET  /aux-info/status         - Check aux_info status");
    info!("  POST /aux-info/start          - Start aux_info generation");
    info!("  POST /cggmp24/keygen/start    - Start CGGMP24 keygen (SegWit)");
    info!("  POST /cggmp24/sign            - CGGMP24 sign (sync)");
    info!("  GET  /cggmp24/keyshares       - List CGGMP24 key shares");
    info!("  POST /frost/keygen/start      - Start FROST keygen (Taproot)");
    info!("  POST /frost/sign              - FROST sign (sync)");
    info!("  GET  /frost/keyshares         - List FROST key shares");
    info!("  POST /p2p/sign                - P2P signing (Phase 4)");
    info!("  GET  /p2p/session/:id         - P2P session status");

    debug!("Binding TCP listener to {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    debug!("TCP listener bound successfully");

    let has_primes = state.read().await.has_primes();
    let has_aux_info = state.read().await.has_aux_info();
    let is_cggmp24_ready = state.read().await.is_cggmp24_ready();
    let has_grant_key = state.read().await.has_grant_verifying_key();

    info!("========================================");
    info!("  MPC Node Ready");
    info!("========================================");
    info!("Listening on: {}", addr);
    info!("Party: {}/{}", args.party_index, NUM_PARTIES);
    info!(
        "Primes: {}",
        if has_primes { "READY" } else { "NOT AVAILABLE" }
    );
    info!(
        "Aux Info: {}",
        if has_aux_info {
            "READY"
        } else {
            "NOT AVAILABLE"
        }
    );
    info!(
        "CGGMP24: {}",
        if is_cggmp24_ready {
            "READY"
        } else {
            "NOT READY"
        }
    );
    info!(
        "Grant Key: {}",
        if has_grant_key {
            "READY"
        } else {
            "NOT AVAILABLE"
        }
    );
    info!(
        "P2P Mode: {}",
        if p2p_ready {
            "READY (QUIC)"
        } else {
            "DISABLED (HTTP relay fallback)"
        }
    );
    info!("========================================");

    axum::serve(listener, app).await?;

    Ok(())
}
