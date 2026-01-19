//! MPC Wallet Coordinator Server
//!
//! This server acts as the coordinator for MPC wallet operations.
//! It receives requests from the CLI and orchestrates the MPC nodes
//! to perform threshold key generation using DKG protocol.

mod bitcoin_client;
mod bitcoin_handlers;
mod cert_handlers;
mod grant_handlers;
mod handlers;
mod health_checker;
mod registry;
mod registry_handlers;
mod relay_handlers;
mod state;

use std::sync::Arc;

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tracing::{debug, info};

use crate::state::AppState;
use common::{NUM_PARTIES, THRESHOLD};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
    info!("  MPC Wallet Coordinator Starting");
    info!("========================================");

    debug!("Initializing application state...");
    let state = Arc::new(RwLock::new(AppState::new()));
    debug!("Application state initialized");

    debug!("Building router...");

    // Configure CORS to allow requests from the UI
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        // Health & Info
        .route("/health", get(handlers::health_check))
        .route("/info", get(handlers::system_info))
        .route("/nodes/status", get(handlers::nodes_status))
        // Grant system
        .route("/grant/pubkey", get(handlers::grant_pubkey))
        // P2P Grant Issuance (Phase 4)
        .route("/grant/signing", post(grant_handlers::issue_signing_grant))
        .route("/grant/keygen", post(grant_handlers::issue_keygen_grant))
        // P2P Certificate Management (Phase 4)
        .route("/certs/ca", get(cert_handlers::get_ca_cert))
        .route("/certs/status", get(cert_handlers::cert_status))
        .route(
            "/certs/node/{party_index}",
            post(cert_handlers::issue_node_cert),
        )
        // Wallet CRUD
        .route("/wallet", post(handlers::create_wallet))
        .route("/wallets", get(handlers::list_wallets))
        .route("/wallet/{wallet_id}", get(handlers::get_wallet))
        .route("/wallet/{wallet_id}", delete(handlers::delete_wallet))
        .route(
            "/wallet/{wallet_id}/pubkey",
            put(handlers::update_wallet_pubkey),
        )
        .route(
            "/wallet/{wallet_id}/taproot-pubkey",
            put(handlers::update_taproot_wallet),
        )
        // BIP32 HD Address Derivation
        .route(
            "/wallet/{wallet_id}/derive",
            get(handlers::derive_addresses),
        )
        .route(
            "/wallet/{wallet_id}/address/{index}",
            get(handlers::get_address),
        )
        // Bitcoin Operations
        .route(
            "/wallet/{wallet_id}/balance",
            get(bitcoin_handlers::get_balance),
        )
        .route(
            "/wallet/{wallet_id}/utxos",
            get(bitcoin_handlers::get_utxos),
        )
        .route(
            "/wallet/{wallet_id}/faucet",
            get(bitcoin_handlers::get_faucet_info),
        )
        .route(
            "/wallet/{wallet_id}/send",
            post(bitcoin_handlers::send_bitcoin),
        )
        .route(
            "/wallet/{wallet_id}/send-taproot",
            post(bitcoin_handlers::send_taproot),
        )
        // Regtest Mining (only works on regtest network)
        .route(
            "/wallet/{wallet_id}/mine",
            post(bitcoin_handlers::mine_blocks),
        )
        // MPC Protocol Relay
        .route("/relay/submit", post(relay_handlers::submit_message))
        .route(
            "/relay/poll/{session_id}/{party_index}",
            get(relay_handlers::poll_messages),
        )
        .route(
            "/relay/session/{session_id}",
            get(relay_handlers::get_session),
        )
        .route("/relay/sessions", get(relay_handlers::list_sessions))
        .route(
            "/relay/aux-info/complete",
            post(relay_handlers::aux_info_complete),
        )
        // Aux Info Generation
        .route("/aux-info/start", post(relay_handlers::start_aux_info_gen))
        // CGGMP24 Keygen and Signing (Native SegWit / ECDSA)
        .route(
            "/cggmp24/keygen/start",
            post(relay_handlers::start_cggmp24_keygen),
        )
        .route(
            "/cggmp24/signing/start",
            post(relay_handlers::start_cggmp24_signing),
        )
        .route("/cggmp24/keyshares", get(handlers::cggmp24_keyshares))
        // FROST Keygen and Signing (Taproot / Schnorr)
        .route(
            "/frost/keygen/start",
            post(relay_handlers::start_frost_keygen),
        )
        .route("/frost/keyshares", get(handlers::frost_keyshares))
        // Node Registry (P2P Phase 2)
        .route("/registry/register", post(registry_handlers::register_node))
        .route("/registry/heartbeat", post(registry_handlers::heartbeat))
        .route("/registry/nodes", get(registry_handlers::list_nodes))
        .route(
            "/registry/node/{party_index}",
            get(registry_handlers::get_node),
        )
        .route("/registry/stats", get(registry_handlers::registry_stats))
        .layer(cors)
        .with_state(state.clone());
    debug!("Router built successfully");

    let addr = "0.0.0.0:3000";
    info!("Configuration:");
    info!("  - Listen address: {}", addr);
    info!("  - Threshold: {}-of-{}", THRESHOLD, NUM_PARTIES);
    info!("  - Number of nodes: {}", NUM_PARTIES);
    info!("");
    info!("Endpoints:");
    info!("  GET    /health                       - Health check");
    info!("  GET    /info                         - System info");
    info!("  GET    /nodes/status                 - Aggregated node status");
    info!("  GET    /grant/pubkey                 - Grant signing public key");
    info!("  POST   /grant/signing                - Issue P2P signing grant");
    info!("  POST   /grant/keygen                 - Issue P2P keygen grant");
    info!("  GET    /certs/ca                     - Get CA certificate (P2P)");
    info!("  GET    /certs/status                 - Certificate status");
    info!("  POST   /certs/node/:party            - Issue node certificate");
    info!("  POST   /wallet                       - Create wallet");
    info!("  GET    /wallets                      - List wallets");
    info!("  GET    /wallet/:id                   - Get wallet");
    info!("  DELETE /wallet/:id                   - Delete wallet");
    info!("  GET    /wallet/:id/derive            - Derive HD addresses");
    info!("  GET    /wallet/:id/address/:index    - Get specific address");
    info!("  GET    /wallet/:id/balance           - Get wallet balance");
    info!("  GET    /wallet/:id/utxos             - Get wallet UTXOs");
    info!("  GET    /wallet/:id/faucet            - Get testnet faucet info");
    info!("  POST   /wallet/:id/send              - Send Bitcoin (SegWit)");
    info!("  POST   /wallet/:id/send-taproot      - Send Bitcoin (Taproot)");
    info!("  POST   /wallet/:id/mine              - Mine blocks (regtest only)");
    info!("");
    info!("  MPC Protocol Relay:");
    info!("  POST   /relay/submit                 - Submit protocol message");
    info!("  GET    /relay/poll/:sid/:party       - Poll for messages");
    info!("  GET    /relay/session/:sid           - Get session status");
    info!("  GET    /relay/sessions               - List all sessions");
    info!("  POST   /aux-info/start               - Start aux_info generation");
    info!("");
    info!("  CGGMP24 Protocol (SegWit/ECDSA):");
    info!("  POST   /cggmp24/keygen/start         - Start CGGMP24 keygen");
    info!("  POST   /cggmp24/signing/start        - Start CGGMP24 signing");
    info!("  GET    /cggmp24/keyshares            - List CGGMP24 key shares");
    info!("");
    info!("  FROST Protocol (Taproot/Schnorr):");
    info!("  POST   /frost/keygen/start           - Start FROST keygen");
    info!("  GET    /frost/keyshares              - List FROST key shares");
    info!("");
    info!("  Node Registry (P2P Phase 2):");
    info!("  POST   /registry/register            - Register a node");
    info!("  POST   /registry/heartbeat           - Node heartbeat");
    info!("  GET    /registry/nodes               - List registered nodes");
    info!("  GET    /registry/node/:party         - Get node by party index");
    info!("  GET    /registry/stats               - Registry statistics");

    // Spawn background health checker
    let _health_checker = health_checker::spawn_health_checker(state);

    debug!("Binding TCP listener to {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    debug!("TCP listener bound successfully");

    info!("");
    info!("Coordinator server is ready on {}", addr);
    info!("========================================");

    axum::serve(listener, app).await?;

    Ok(())
}
