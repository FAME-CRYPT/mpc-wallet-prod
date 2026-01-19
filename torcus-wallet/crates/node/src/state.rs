//! Node state management.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use common::observability::ProtocolMetrics;
use ed25519_dalek::VerifyingKey;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use protocols::cggmp24::keygen::StoredKeyShare as Cggmp24KeyShare;
use protocols::p2p::{P2pSessionCoordinator, QuicTransport};
use protocols::transport::TransportConfig;

use crate::session::SessionManager;

/// Path for persisted cert_token.
fn cert_token_path(party_index: u16) -> PathBuf {
    PathBuf::from(format!("/data/cert-token-party-{}.json", party_index))
}

/// Stored format for cert_token.
#[derive(serde::Serialize, serde::Deserialize)]
struct StoredCertToken {
    /// The secret cert_token.
    token: String,
}

/// Load cert_token from disk if it exists.
fn load_cert_token(party_index: u16) -> Option<String> {
    let path = cert_token_path(party_index);
    if !path.exists() {
        return None;
    }

    match std::fs::read_to_string(&path) {
        Ok(contents) => match serde_json::from_str::<StoredCertToken>(&contents) {
            Ok(stored) => {
                info!("Loaded cert_token from {:?}", path);
                Some(stored.token)
            }
            Err(e) => {
                warn!("Failed to parse cert_token from {:?}: {}", path, e);
                None
            }
        },
        Err(e) => {
            warn!("Failed to read cert_token from {:?}: {}", path, e);
            None
        }
    }
}

/// Save cert_token to disk.
fn save_cert_token(party_index: u16, token: &str) -> Result<(), String> {
    let path = cert_token_path(party_index);
    let stored = StoredCertToken {
        token: token.to_string(),
    };
    let json = serde_json::to_string_pretty(&stored)
        .map_err(|e| format!("Failed to serialize cert_token: {}", e))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("Failed to write cert_token to {:?}: {}", path, e))?;
    info!("Saved cert_token to {:?}", path);
    Ok(())
}

/// Node state.
pub struct NodeState {
    pub node_id: Uuid,
    pub party_index: u16,
    /// CGGMP24 key shares (wallet_id -> key share)
    pub cggmp24_key_shares: HashMap<String, Cggmp24KeyShare>,
    /// Pregenerated primes for CGGMP24 (serialized bytes)
    pub pregenerated_primes: Option<Vec<u8>>,
    /// Auxiliary info for CGGMP24 (serialized bytes)
    pub aux_info: Option<Vec<u8>>,
    /// Grant verifying key (fetched from coordinator)
    grant_verifying_key: Option<VerifyingKey>,
    /// Session manager for tracking signing sessions and replay protection.
    pub session_manager: SessionManager,
    /// Transport configuration for MPC protocol communication.
    pub transport_config: TransportConfig,
    /// Protocol metrics for observability.
    pub metrics: Arc<ProtocolMetrics>,
    /// P2P session coordinator for managing P2P signing sessions.
    p2p_coordinator: Option<Arc<P2pSessionCoordinator>>,
    /// QUIC transport for P2P messaging.
    quic_transport: Option<Arc<QuicTransport>>,
    /// Whether P2P mode is enabled and ready.
    p2p_enabled: bool,
    /// Secret token for certificate issuance (received during registration).
    /// This is never exposed publicly and is required for P2P certificate requests.
    cert_token: Option<String>,
}

impl std::fmt::Debug for NodeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeState")
            .field("node_id", &self.node_id)
            .field("party_index", &self.party_index)
            .field("cggmp24_key_shares", &self.cggmp24_key_shares.len())
            .field("has_primes", &self.pregenerated_primes.is_some())
            .field("has_aux_info", &self.aux_info.is_some())
            .field("has_grant_key", &self.grant_verifying_key.is_some())
            .field("session_stats", &self.session_manager.stats())
            .field("transport_type", &self.transport_config.transport_type)
            .field("p2p_enabled", &self.p2p_enabled)
            .field("has_cert_token", &self.cert_token.is_some())
            .finish()
    }
}

impl NodeState {
    /// Create new node state with primes.
    pub fn new_with_primes(node_id: Uuid, party_index: u16, primes: Option<Vec<u8>>) -> Self {
        debug!("Creating NodeState");
        debug!("  Node ID: {}", node_id);
        debug!("  Party Index: {}", party_index);
        debug!("  Has primes: {}", primes.is_some());

        // Try to load aux_info from disk
        let aux_info = match protocols::cggmp24::aux_info::check_aux_info(party_index, None) {
            protocols::cggmp24::aux_info::AuxInfoInitResult::Loaded(data) => {
                info!("Loaded existing aux_info ({} bytes)", data.len());
                Some(data)
            }
            _ => None,
        };

        // Load CGGMP24 key shares from disk
        let cggmp24_key_shares = load_cggmp24_key_shares(party_index);

        // Try to load cert_token from disk (persisted from previous registration)
        let cert_token = load_cert_token(party_index);

        let node_id_str = format!("node-{}", party_index);
        info!(
            "NodeState created: party {}, {} CGGMP24 key shares, primes: {}, aux_info: {}, cert_token: {}",
            party_index,
            cggmp24_key_shares.len(),
            if primes.is_some() { "yes" } else { "no" },
            if aux_info.is_some() { "yes" } else { "no" },
            if cert_token.is_some() { "yes" } else { "no" }
        );

        Self {
            node_id,
            party_index,
            cggmp24_key_shares,
            pregenerated_primes: primes,
            aux_info,
            grant_verifying_key: None,
            session_manager: SessionManager::new(),
            transport_config: TransportConfig::default(),
            metrics: Arc::new(ProtocolMetrics::new(node_id_str)),
            p2p_coordinator: None,
            quic_transport: None,
            p2p_enabled: false,
            cert_token,
        }
    }

    /// Create new node state with custom transport configuration.
    #[allow(dead_code)]
    pub fn new_with_transport(
        node_id: Uuid,
        party_index: u16,
        primes: Option<Vec<u8>>,
        transport_config: TransportConfig,
    ) -> Self {
        let mut state = Self::new_with_primes(node_id, party_index, primes);
        state.transport_config = transport_config;
        state
    }

    /// Get the coordinator URL from transport config.
    #[allow(dead_code)]
    pub fn coordinator_url(&self) -> Option<&str> {
        self.transport_config.coordinator_url.as_deref()
    }

    /// Set pregenerated primes.
    pub fn set_primes(&mut self, primes: Vec<u8>) {
        info!("Setting pregenerated primes ({} bytes)", primes.len());
        self.pregenerated_primes = Some(primes);
    }

    /// Get pregenerated primes.
    pub fn get_primes(&self) -> Option<&[u8]> {
        self.pregenerated_primes.as_deref()
    }

    /// Check if we have pregenerated primes.
    pub fn has_primes(&self) -> bool {
        self.pregenerated_primes.is_some()
    }

    /// Set auxiliary info.
    pub fn set_aux_info(&mut self, aux_info: Vec<u8>) {
        info!("Setting aux_info ({} bytes)", aux_info.len());
        self.aux_info = Some(aux_info);
    }

    /// Get auxiliary info.
    pub fn get_aux_info(&self) -> Option<&[u8]> {
        self.aux_info.as_deref()
    }

    /// Check if we have auxiliary info.
    pub fn has_aux_info(&self) -> bool {
        self.aux_info.is_some()
    }

    /// Check if we're ready for CGGMP24 protocol (have both primes and aux_info).
    pub fn is_cggmp24_ready(&self) -> bool {
        self.has_primes() && self.has_aux_info()
    }

    /// Set the grant verifying key (fetched from coordinator).
    pub fn set_grant_verifying_key(&mut self, key: VerifyingKey) {
        info!(
            "Setting grant verifying key: {}",
            hex::encode(key.as_bytes())
        );
        self.grant_verifying_key = Some(key);
    }

    /// Get the grant verifying key.
    #[allow(dead_code)]
    pub fn grant_verifying_key(&self) -> Option<&VerifyingKey> {
        self.grant_verifying_key.as_ref()
    }

    /// Check if we have a grant verifying key.
    pub fn has_grant_verifying_key(&self) -> bool {
        self.grant_verifying_key.is_some()
    }

    /// Set the certificate token (received during registration).
    /// This token is required for requesting TLS certificates from the coordinator.
    /// The token is also persisted to disk for recovery after restarts.
    pub fn set_cert_token(&mut self, token: String) {
        info!("Setting cert_token for certificate issuance");
        // Persist to disk for recovery
        if let Err(e) = save_cert_token(self.party_index, &token) {
            warn!("Failed to persist cert_token: {}", e);
            // Continue anyway - we'll have it in memory for this session
        }
        self.cert_token = Some(token);
    }

    /// Get the certificate token.
    pub fn cert_token(&self) -> Option<&str> {
        self.cert_token.as_deref()
    }

    /// Check if we have a certificate token.
    #[allow(dead_code)]
    pub fn has_cert_token(&self) -> bool {
        self.cert_token.is_some()
    }

    /// Verify a signing grant.
    ///
    /// Returns Ok(()) if the grant is valid for this party, or an error message.
    pub fn verify_grant(&self, grant: &common::SigningGrant) -> Result<(), String> {
        let key = self
            .grant_verifying_key
            .as_ref()
            .ok_or_else(|| "Grant verifying key not set".to_string())?;

        grant
            .validate(key, self.party_index)
            .map_err(|e| format!("Grant validation failed: {}", e))
    }

    // =========================================================================
    // P2P Infrastructure (Phase 4)
    // =========================================================================

    /// Initialize P2P infrastructure.
    ///
    /// Sets up the P2P session coordinator and QUIC transport for direct
    /// peer-to-peer communication.
    pub fn init_p2p(
        &mut self,
        coordinator: Arc<P2pSessionCoordinator>,
        transport: Arc<QuicTransport>,
    ) {
        info!(
            "Initializing P2P infrastructure for party {}",
            self.party_index
        );
        self.p2p_coordinator = Some(coordinator);
        self.quic_transport = Some(transport);
        self.p2p_enabled = true;
        info!("P2P infrastructure initialized successfully");
    }

    /// Check if P2P mode is enabled and ready.
    pub fn is_p2p_ready(&self) -> bool {
        self.p2p_enabled && self.p2p_coordinator.is_some() && self.quic_transport.is_some()
    }

    /// Check if P2P is enabled (may not be fully ready).
    #[allow(dead_code)]
    pub fn is_p2p_enabled(&self) -> bool {
        self.p2p_enabled
    }

    /// Get the P2P session coordinator.
    #[allow(dead_code)]
    pub fn p2p_coordinator(&self) -> Option<&Arc<P2pSessionCoordinator>> {
        self.p2p_coordinator.as_ref()
    }

    /// Get the QUIC transport.
    #[allow(dead_code)]
    pub fn quic_transport(&self) -> Option<&Arc<QuicTransport>> {
        self.quic_transport.as_ref()
    }

    /// Get both P2P components (coordinator and transport).
    ///
    /// Returns None if P2P is not ready.
    pub fn p2p_components(&self) -> Option<(Arc<P2pSessionCoordinator>, Arc<QuicTransport>)> {
        if !self.is_p2p_ready() {
            return None;
        }
        Some((
            self.p2p_coordinator.clone().unwrap(),
            self.quic_transport.clone().unwrap(),
        ))
    }

    /// Disable P2P mode (e.g., on failure).
    #[allow(dead_code)]
    pub fn disable_p2p(&mut self) {
        info!("Disabling P2P mode");
        self.p2p_enabled = false;
        // Keep the components around in case we want to retry
    }
}

/// Load CGGMP24 key shares from disk
fn load_cggmp24_key_shares(party_index: u16) -> HashMap<String, Cggmp24KeyShare> {
    let mut key_shares = HashMap::new();

    let data_dir = std::path::Path::new("/data");
    if !data_dir.exists() {
        return key_shares;
    }

    let prefix = format!("keyshare-party-{}-", party_index);

    if let Ok(entries) = std::fs::read_dir(data_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if filename.starts_with(&prefix) && filename.ends_with(".json") {
                    match protocols::cggmp24::keygen::load_key_share(&path) {
                        Ok(ks) => {
                            info!("Loaded CGGMP24 key share for wallet: {}", ks.wallet_id);
                            key_shares.insert(ks.wallet_id.clone(), ks);
                        }
                        Err(e) => {
                            error!("Failed to load key share from {:?}: {}", path, e);
                        }
                    }
                }
            }
        }
    }

    key_shares
}
