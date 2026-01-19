//! P2P infrastructure initialization.
//!
//! This module handles setting up the P2P session coordinator and QUIC
//! transport for direct peer-to-peer MPC protocol execution.
//!
//! ## Initialization Flow
//!
//! ```text
//! 1. Fetch/load node certificate (from coordinator or cache)
//! 2. Create P2pSessionCoordinator with grant_verifying_key
//! 3. Create QuicTransport with certificate
//! 4. Initialize transport (starts QUIC listener)
//! 5. Refresh peer list from coordinator registry
//! 6. Store components in NodeState
//! ```

use std::net::SocketAddr;
use std::sync::Arc;

use ed25519_dalek::VerifyingKey;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use protocols::p2p::{P2pSessionCoordinator, QuicTransportBuilder, DEFAULT_QUIC_PORT};

use crate::certs::{default_hostnames, ensure_node_certificate, CertError};
use crate::state::NodeState;

/// Error during P2P initialization.
#[derive(Debug)]
pub enum P2pInitError {
    /// Failed to obtain certificate.
    Certificate(CertError),
    /// Failed to create transport.
    Transport(String),
    /// Failed to initialize transport.
    Init(String),
    /// Grant verifying key not available.
    #[allow(dead_code)]
    NoGrantKey,
}

impl std::fmt::Display for P2pInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Certificate(e) => write!(f, "Certificate error: {}", e),
            Self::Transport(e) => write!(f, "Transport error: {}", e),
            Self::Init(e) => write!(f, "Initialization error: {}", e),
            Self::NoGrantKey => write!(f, "Grant verifying key not available"),
        }
    }
}

impl std::error::Error for P2pInitError {}

/// Initialize P2P infrastructure for a node.
///
/// This function:
/// 1. Fetches or loads the node's TLS certificate
/// 2. Creates the P2P session coordinator
/// 3. Creates and initializes the QUIC transport
/// 4. Stores everything in the node state
///
/// # Arguments
///
/// * `state` - The node state (wrapped in RwLock)
/// * `coordinator_url` - Base URL of the coordinator
/// * `grant_verifying_key` - Key to verify grants (fetched earlier from coordinator)
/// * `quic_port` - Port to listen on for QUIC connections
///
/// # Returns
///
/// Ok(()) on success, or an error if initialization fails.
/// On failure, the node should fall back to HTTP relay mode.
pub async fn init_p2p_infrastructure(
    state: Arc<RwLock<NodeState>>,
    coordinator_url: &str,
    grant_verifying_key: VerifyingKey,
    quic_port: Option<u16>,
) -> Result<(), P2pInitError> {
    let (party_index, cert_token) = {
        let s = state.read().await;
        let token = s.cert_token().map(|s| s.to_string());
        (s.party_index, token)
    };
    let port = quic_port.unwrap_or(DEFAULT_QUIC_PORT);

    info!("========================================");
    info!("  P2P Infrastructure Initialization");
    info!("========================================");
    info!("Party index: {}", party_index);
    info!("Coordinator URL: {}", coordinator_url);
    info!("QUIC port: {}", port);
    info!(
        "Has cert_token: {}",
        if cert_token.is_some() { "yes" } else { "no" }
    );

    // Step 1: Fetch or load certificate
    // We can use a cached cert without cert_token, but need cert_token to fetch a new one
    info!("Step 1: Fetching node certificate...");
    let hostnames = default_hostnames(party_index);
    let node_cert = ensure_node_certificate(
        cert_token.as_deref(),
        party_index,
        coordinator_url,
        hostnames,
    )
    .await
    .map_err(P2pInitError::Certificate)?;
    info!("Certificate obtained for party {}", party_index);

    // Step 2: Create P2P session coordinator
    info!("Step 2: Creating P2P session coordinator...");
    let p2p_coordinator = P2pSessionCoordinator::new(party_index, grant_verifying_key);
    let p2p_coordinator = Arc::new(p2p_coordinator);
    info!("P2P session coordinator created");

    // Step 3: Create QUIC transport
    info!("Step 3: Creating QUIC transport...");
    let listen_addr: SocketAddr = format!("0.0.0.0:{}", port)
        .parse()
        .map_err(|e| P2pInitError::Transport(format!("Invalid listen address: {}", e)))?;

    let quic_transport = QuicTransportBuilder::new(party_index, coordinator_url)
        .with_cert(node_cert)
        .with_listen_addr(listen_addr)
        .build()
        .map_err(|e| P2pInitError::Transport(format!("Failed to build transport: {}", e)))?;
    let quic_transport = Arc::new(quic_transport);
    info!("QUIC transport created");

    // Step 4: Initialize transport (starts listener)
    info!("Step 4: Initializing QUIC transport (starting listener)...");
    quic_transport
        .init()
        .await
        .map_err(|e| P2pInitError::Init(format!("Failed to initialize transport: {}", e)))?;
    info!("QUIC listener started on port {}", port);

    // Step 5: Refresh peer list
    info!("Step 5: Refreshing peer list from registry...");
    match quic_transport.refresh_peers().await {
        Ok(()) => {
            info!("Peer list refreshed from registry");
        }
        Err(e) => {
            // Non-fatal - we can still work with peers that connect to us
            warn!("Failed to refresh peer list: {} (continuing anyway)", e);
        }
    }

    // Step 6: Store in state
    info!("Step 6: Storing P2P components in node state...");
    {
        let mut s = state.write().await;
        s.init_p2p(p2p_coordinator, quic_transport);
    }

    info!("========================================");
    info!("  P2P Infrastructure Ready");
    info!("========================================");

    Ok(())
}

/// Try to initialize P2P with retries.
///
/// If initialization fails, logs the error and returns false.
/// The node should continue with HTTP relay fallback.
pub async fn try_init_p2p(
    state: Arc<RwLock<NodeState>>,
    coordinator_url: &str,
    grant_verifying_key: VerifyingKey,
    quic_port: Option<u16>,
    max_retries: u32,
) -> bool {
    for attempt in 1..=max_retries {
        info!("P2P initialization attempt {}/{}", attempt, max_retries);

        match init_p2p_infrastructure(
            state.clone(),
            coordinator_url,
            grant_verifying_key,
            quic_port,
        )
        .await
        {
            Ok(()) => {
                info!("P2P infrastructure initialized successfully");
                return true;
            }
            Err(e) => {
                error!("P2P initialization failed: {}", e);
                if attempt < max_retries {
                    let delay = std::time::Duration::from_secs(2u64.pow(attempt.min(4)));
                    warn!("Retrying in {:?}...", delay);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    error!("========================================");
    error!("  P2P Initialization Failed");
    error!("========================================");
    error!("P2P mode will be disabled. Node will use HTTP relay fallback.");
    error!("This is not fatal - signing will still work via the coordinator.");

    false
}

/// Check if P2P should be enabled based on environment.
///
/// Checks the P2P_ENABLED environment variable.
/// Defaults to true if not set.
pub fn is_p2p_enabled_env() -> bool {
    std::env::var("P2P_ENABLED")
        .map(|v| !matches!(v.to_lowercase().as_str(), "false" | "0" | "no"))
        .unwrap_or(true)
}

/// Get QUIC port from environment or use default.
pub fn get_quic_port_env() -> u16 {
    std::env::var("QUIC_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_QUIC_PORT)
}

/// Check if HTTP fallback is enabled when P2P fails.
///
/// Checks the P2P_FALLBACK_HTTP environment variable.
/// Defaults to true if not set.
///
/// When enabled:
/// - If P2P signing fails, automatically retry with HTTP relay
/// - Provides resilience against network issues
///
/// When disabled:
/// - P2P failures are returned as errors
/// - Useful for testing pure P2P mode
pub fn is_p2p_fallback_enabled() -> bool {
    std::env::var("P2P_FALLBACK_HTTP")
        .map(|v| !matches!(v.to_lowercase().as_str(), "false" | "0" | "no"))
        .unwrap_or(true)
}

/// Get the P2P session proposal timeout in seconds.
///
/// Checks the P2P_PROPOSAL_TIMEOUT environment variable.
/// Defaults to 10 seconds.
pub fn get_proposal_timeout_secs() -> u64 {
    std::env::var("P2P_PROPOSAL_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_p2p_enabled_env_default() {
        // When env var is not set, should default to true
        std::env::remove_var("P2P_ENABLED");
        assert!(is_p2p_enabled_env());
    }

    #[test]
    fn test_get_quic_port_env_default() {
        std::env::remove_var("QUIC_PORT");
        assert_eq!(get_quic_port_env(), DEFAULT_QUIC_PORT);
    }

    #[test]
    fn test_is_p2p_fallback_enabled_default() {
        // When env var is not set, should default to true
        std::env::remove_var("P2P_FALLBACK_HTTP");
        assert!(is_p2p_fallback_enabled());
    }

    #[test]
    fn test_get_proposal_timeout_secs_default() {
        std::env::remove_var("P2P_PROPOSAL_TIMEOUT");
        assert_eq!(get_proposal_timeout_secs(), 10);
    }
}
