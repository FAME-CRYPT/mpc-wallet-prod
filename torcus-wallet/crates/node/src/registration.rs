//! Node registration and heartbeat client.
//!
//! This module handles registering the node with the coordinator
//! and sending periodic heartbeats to maintain registration.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tracing::{debug, error, info, trace, warn};

use common::discovery::{
    HeartbeatRequest, HeartbeatResponse, NodeCapabilities, ProtocolCapability, RegisterNodeRequest,
    RegisterNodeResponse,
};

use crate::state::NodeState;

/// Default heartbeat interval (will be updated from coordinator response).
pub const DEFAULT_HEARTBEAT_INTERVAL_SECS: u64 = 10;

/// Maximum retries for registration.
const MAX_REGISTRATION_RETRIES: u32 = 5;

/// Delay between registration retries.
const REGISTRATION_RETRY_DELAY_SECS: u64 = 5;

/// Environment variable for the node registration pre-shared key.
const NODE_PSK_ENV: &str = "NODE_REGISTRATION_PSK";

/// Environment variable to indicate production mode (disables dev fallbacks).
const PRODUCTION_ENV: &str = "TORCUS_PRODUCTION";

/// Default PSK for development ONLY (must match coordinator's default).
const DEFAULT_DEV_PSK: &str = "dev-psk-change-in-production";

/// Build node capabilities from current state.
pub fn build_capabilities(state: &NodeState) -> NodeCapabilities {
    let mut protocols = Vec::new();

    // Always advertise basic protocols
    protocols.push(ProtocolCapability::Cggmp24);
    protocols.push(ProtocolCapability::Frost);

    if state.has_primes() {
        protocols.push(ProtocolCapability::AuxInfo);
    }

    // Collect wallet IDs from key shares
    let wallet_ids: Vec<String> = state.cggmp24_key_shares.keys().cloned().collect();

    NodeCapabilities {
        protocols,
        has_primes: state.has_primes(),
        has_aux_info: state.has_aux_info(),
        cggmp24_keyshare_count: state.cggmp24_key_shares.len(),
        frost_keyshare_count: 0, // TODO: Add FROST key share tracking when implemented
        wallet_ids,
    }
}

/// Result of successful registration.
pub struct RegistrationResult {
    /// Recommended heartbeat interval in seconds.
    pub heartbeat_interval_secs: u64,
    /// Secret token for certificate issuance (only present for new registrations).
    pub cert_token: Option<String>,
}

/// Register this node with the coordinator.
///
/// Retries up to MAX_REGISTRATION_RETRIES times with exponential backoff.
/// Returns registration result including heartbeat interval and cert_token (for new registrations).
pub async fn register_with_coordinator(
    state: Arc<RwLock<NodeState>>,
    coordinator_url: &str,
    listen_port: u16,
) -> Result<RegistrationResult, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let register_url = format!("{}/registry/register", coordinator_url);

    // Check if we're in production mode
    let is_production = std::env::var(PRODUCTION_ENV)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    // Load PSK from environment
    let psk = match std::env::var(NODE_PSK_ENV) {
        Ok(psk) => psk,
        Err(_) => {
            if is_production {
                return Err(format!(
                    "{} must be set when {} is enabled",
                    NODE_PSK_ENV, PRODUCTION_ENV
                ));
            }
            warn!(
                "NODE_REGISTRATION_PSK not set, using default dev PSK. \
                 Set {}=true in production to require explicit PSK.",
                PRODUCTION_ENV
            );
            DEFAULT_DEV_PSK.to_string()
        }
    };

    for attempt in 1..=MAX_REGISTRATION_RETRIES {
        let (node_id, party_index, capabilities) = {
            let s = state.read().await;
            (s.node_id, s.party_index, build_capabilities(&s))
        };

        // Determine our endpoint URL
        // In Docker, nodes are named mpc-node-1, mpc-node-2, etc.
        let endpoint = format!("http://mpc-node-{}:{}", party_index + 1, listen_port);

        let request = RegisterNodeRequest {
            psk: psk.clone(),
            node_id,
            party_index,
            endpoint: endpoint.clone(),
            capabilities,
        };

        info!(
            "Registering with coordinator (attempt {}/{}): {}",
            attempt, MAX_REGISTRATION_RETRIES, register_url
        );
        debug!("Registration endpoint: {}", endpoint);

        match client.post(&register_url).json(&request).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    match resp.json::<RegisterNodeResponse>().await {
                        Ok(response) => {
                            info!(
                                "Registration successful: {}. Heartbeat interval: {}s",
                                response.message, response.heartbeat_interval_secs
                            );
                            if response.cert_token.is_some() {
                                info!("Received cert_token for certificate issuance");
                            }
                            return Ok(RegistrationResult {
                                heartbeat_interval_secs: response.heartbeat_interval_secs,
                                cert_token: response.cert_token,
                            });
                        }
                        Err(e) => {
                            warn!("Failed to parse registration response: {}", e);
                        }
                    }
                } else {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    warn!("Registration failed with status {}: {}", status, body);
                }
            }
            Err(e) => {
                warn!("Failed to connect to coordinator: {}", e);
            }
        }

        if attempt < MAX_REGISTRATION_RETRIES {
            info!(
                "Retrying registration in {}s...",
                REGISTRATION_RETRY_DELAY_SECS
            );
            tokio::time::sleep(Duration::from_secs(REGISTRATION_RETRY_DELAY_SECS)).await;
        }
    }

    Err(format!(
        "Failed to register with coordinator after {} attempts",
        MAX_REGISTRATION_RETRIES
    ))
}

/// Spawn the heartbeat task that periodically sends heartbeats to the coordinator.
pub fn spawn_heartbeat_task(
    state: Arc<RwLock<NodeState>>,
    coordinator_url: String,
    heartbeat_interval_secs: u64,
) -> tokio::task::JoinHandle<()> {
    info!(
        "Starting heartbeat task (interval: {}s)",
        heartbeat_interval_secs
    );

    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("Failed to create HTTP client");

        let heartbeat_url = format!("{}/registry/heartbeat", coordinator_url);

        let mut interval = tokio::time::interval(Duration::from_secs(heartbeat_interval_secs));

        // Skip the first immediate tick
        interval.tick().await;

        loop {
            interval.tick().await;

            let (node_id, party_index, capabilities, cert_token) = {
                let s = state.read().await;
                (
                    s.node_id,
                    s.party_index,
                    build_capabilities(&s),
                    s.cert_token().map(|t| t.to_string()),
                )
            };

            // Skip heartbeat if we don't have a cert_token (not yet registered)
            let cert_token = match cert_token {
                Some(token) => token.to_string(),
                None => {
                    debug!("Skipping heartbeat: no cert_token available");
                    continue;
                }
            };

            let request = HeartbeatRequest {
                cert_token,
                node_id,
                party_index,
                capabilities,
            };

            match client.post(&heartbeat_url).json(&request).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        match resp.json::<HeartbeatResponse>().await {
                            Ok(response) => {
                                trace!(
                                    "Heartbeat acknowledged: health_score={:.2}, registered_nodes={}",
                                    response.health_score, response.registered_nodes
                                );
                            }
                            Err(e) => {
                                warn!("Failed to parse heartbeat response: {}", e);
                            }
                        }
                    } else {
                        warn!("Heartbeat failed with status {}", resp.status());
                    }
                }
                Err(e) => {
                    error!("Failed to send heartbeat: {}", e);
                }
            }
        }
    })
}
