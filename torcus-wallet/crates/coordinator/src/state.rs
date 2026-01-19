//! Application state for the coordinator.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::Utc;
use common::{RelaySessionStore, StoredWallet, WalletStore, WalletType, NUM_PARTIES};
use ed25519_dalek::{SigningKey, VerifyingKey};
use protocols::p2p::certs::{self, CaCertificate};
use rand::rngs::OsRng;
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

use crate::registry::NodeRegistry;
use crate::relay_handlers::{RelaySession, SESSION_TTL_SECS};

/// Default path for the grant signing key.
const GRANT_KEY_PATH: &str = "/data/grant-signing-key.json";

/// Default path for the CA certificate.
const CA_CERT_PATH: &str = "/data/ca-cert.json";

/// Environment variable for the node registration pre-shared key.
const NODE_PSK_ENV: &str = "NODE_REGISTRATION_PSK";

/// Environment variable to indicate production mode (disables dev fallbacks).
const PRODUCTION_ENV: &str = "TORCUS_PRODUCTION";

/// Default PSK for development ONLY. Not used in production mode.
const DEFAULT_DEV_PSK: &str = "dev-psk-change-in-production";

/// Stored format for the grant signing key.
#[derive(serde::Serialize, serde::Deserialize)]
struct StoredGrantKey {
    /// Secret key bytes (32 bytes, hex encoded).
    secret_key: String,
}

/// Information about a created wallet (in-memory cache).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WalletInfo {
    pub name: String,
    pub wallet_type: WalletType,
    pub public_key: String,
    pub address: String,
}

impl From<StoredWallet> for WalletInfo {
    fn from(w: StoredWallet) -> Self {
        Self {
            name: w.name,
            wallet_type: w.wallet_type,
            public_key: w.public_key,
            address: w.address,
        }
    }
}

/// Application state shared across handlers.
pub struct AppState {
    /// Registered MPC node endpoints (hardcoded fallback).
    pub nodes: Vec<String>,
    /// In-memory wallet cache (wallet_id -> info).
    pub wallets: HashMap<Uuid, WalletInfo>,
    /// Persistent wallet storage.
    store: WalletStore,
    /// Active relay sessions for MPC protocols (session_id -> session).
    pub relay_sessions: HashMap<String, RelaySession>,
    /// Persistent relay session storage.
    pub session_store: RelaySessionStore,
    /// Ed25519 signing key for creating grants.
    grant_signing_key: SigningKey,
    /// Node registry for dynamic node discovery.
    pub node_registry: NodeRegistry,
    /// CA certificate for signing node certificates (P2P mTLS).
    #[allow(dead_code)]
    ca_certificate: Option<CaCertificate>,
    /// Pre-shared key for node registration authentication.
    node_psk: String,
}

/// Type alias for backward compatibility
pub type CoordinatorState = AppState;

/// Load or generate the CA certificate for P2P mTLS.
fn load_or_generate_ca_cert(cert_path: &str) -> Option<CaCertificate> {
    let path = std::path::Path::new(cert_path);

    // Try to load existing CA certificate
    if path.exists() {
        match certs::load_ca_cert(path) {
            Ok(stored) => match CaCertificate::from_pem(&stored.cert_pem, &stored.key_pem) {
                Ok(ca) => {
                    info!("Loaded existing CA certificate from {}", cert_path);
                    return Some(ca);
                }
                Err(e) => {
                    warn!(
                        "Failed to parse CA certificate from {}: {}. Generating new one.",
                        cert_path, e
                    );
                }
            },
            Err(e) => {
                warn!(
                    "Failed to read CA certificate from {}: {}. Generating new one.",
                    cert_path, e
                );
            }
        }
    }

    // Generate new CA certificate
    match CaCertificate::generate() {
        Ok(ca) => {
            info!("Generated new CA certificate");

            // Try to save it
            let stored = ca.to_stored();
            if let Err(e) = certs::save_ca_cert(path, &stored) {
                warn!("Failed to save CA certificate to {}: {}", cert_path, e);
            } else {
                info!("Saved CA certificate to {}", cert_path);
            }

            Some(ca)
        }
        Err(e) => {
            error!("Failed to generate CA certificate: {}", e);
            None
        }
    }
}

/// Load or generate the grant signing key.
fn load_or_generate_grant_key(key_path: &str) -> SigningKey {
    // Try to load existing key
    if let Ok(contents) = std::fs::read_to_string(key_path) {
        if let Ok(stored) = serde_json::from_str::<StoredGrantKey>(&contents) {
            if let Ok(secret_bytes) = hex::decode(&stored.secret_key) {
                if secret_bytes.len() == 32 {
                    let bytes: [u8; 32] = secret_bytes.try_into().unwrap();
                    let key = SigningKey::from_bytes(&bytes);
                    info!("Loaded existing grant signing key from {}", key_path);
                    return key;
                }
            }
        }
        warn!(
            "Failed to parse grant signing key from {}, generating new one",
            key_path
        );
    }

    // Generate new key
    let key = SigningKey::generate(&mut OsRng);
    info!("Generated new grant signing key");

    // Try to save it
    let stored = StoredGrantKey {
        secret_key: hex::encode(key.to_bytes()),
    };
    if let Ok(json) = serde_json::to_string_pretty(&stored) {
        if let Err(e) = std::fs::write(key_path, json) {
            warn!("Failed to save grant signing key to {}: {}", key_path, e);
        } else {
            info!("Saved grant signing key to {}", key_path);
        }
    }

    key
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("nodes", &self.nodes)
            .field("wallets", &self.wallets.len())
            .field("relay_sessions", &self.relay_sessions.len())
            .finish()
    }
}

impl AppState {
    /// Create new app state with storage at the given path.
    pub fn new_with_storage(db_path: PathBuf) -> Result<Self, common::MpcWalletError> {
        debug!("Creating AppState with storage at {:?}", db_path);

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                common::MpcWalletError::Storage(format!("Failed to create data dir: {}", e))
            })?;
        }

        let store = WalletStore::open(&db_path)?;
        info!("Wallet store opened at {:?}", db_path);

        // Open session store (in same directory as wallet store)
        let session_db_path = db_path.with_file_name("sessions.db");
        let session_store = RelaySessionStore::open(&session_db_path)?;
        info!("Session store opened at {:?}", session_db_path);

        // Node URLs based on Docker Compose service names
        let nodes: Vec<String> = (1..=NUM_PARTIES)
            .map(|i| format!("http://mpc-node-{}:3000", i))
            .collect();
        trace!("Node URLs: {:?}", nodes);

        // Load existing wallets into memory cache
        let stored_wallets = store.list_wallets()?;
        let mut wallets = HashMap::new();
        for w in stored_wallets {
            debug!("Loaded wallet from storage: {} ({})", w.name, w.id);
            wallets.insert(w.id, WalletInfo::from(w));
        }

        // Load existing relay sessions from persistent storage
        let mut relay_sessions = HashMap::new();
        match session_store.list_active_sessions(SESSION_TTL_SECS) {
            Ok(stored_sessions) => {
                for stored in stored_sessions {
                    match RelaySession::from_stored(stored) {
                        Ok(session) => {
                            info!(
                                "Restored relay session {} (protocol: {}, parties: {:?})",
                                session.session_id, session.protocol, session.parties
                            );
                            relay_sessions.insert(session.session_id.clone(), session);
                        }
                        Err(e) => {
                            warn!("Failed to restore relay session: {}", e);
                        }
                    }
                }
                if !relay_sessions.is_empty() {
                    info!(
                        "Restored {} relay sessions from storage",
                        relay_sessions.len()
                    );
                }
            }
            Err(e) => {
                warn!("Failed to load relay sessions from storage: {}", e);
            }
        }

        // Load or generate grant signing key
        let grant_signing_key = load_or_generate_grant_key(GRANT_KEY_PATH);
        let pubkey_hex = hex::encode(grant_signing_key.verifying_key().as_bytes());
        info!("Grant signing public key: {}", pubkey_hex);

        // Load or generate CA certificate for P2P mTLS
        let ca_certificate = load_or_generate_ca_cert(CA_CERT_PATH);
        if ca_certificate.is_some() {
            info!("CA certificate ready for P2P node certificate signing");
        } else {
            warn!("CA certificate not available - P2P certificate issuance disabled");
        }

        info!(
            "AppState created with {} nodes, {} wallets, {} sessions loaded",
            nodes.len(),
            wallets.len(),
            relay_sessions.len()
        );

        // Initialize node registry
        let node_registry = NodeRegistry::with_defaults();
        info!("Node registry initialized");

        // Check if we're in production mode
        let is_production = std::env::var(PRODUCTION_ENV)
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        // Load node registration PSK from environment
        let node_psk = match std::env::var(NODE_PSK_ENV) {
            Ok(psk) => {
                info!("Node registration PSK configured from environment");
                psk
            }
            Err(_) => {
                if is_production {
                    error!(
                        "FATAL: {} must be set in production mode ({}=true)",
                        NODE_PSK_ENV, PRODUCTION_ENV
                    );
                    return Err(common::MpcWalletError::Configuration(format!(
                        "{} must be set when {} is enabled",
                        NODE_PSK_ENV, PRODUCTION_ENV
                    )));
                }
                warn!(
                    "NODE_REGISTRATION_PSK not set, using default dev PSK. \
                     Set {}=true in production to require explicit PSK.",
                    PRODUCTION_ENV
                );
                DEFAULT_DEV_PSK.to_string()
            }
        };

        Ok(Self {
            nodes,
            wallets,
            store,
            relay_sessions,
            session_store,
            grant_signing_key,
            node_registry,
            ca_certificate,
            node_psk,
        })
    }

    /// Create new app state with default storage path.
    ///
    /// # Panics
    ///
    /// Panics in production mode (`TORCUS_PRODUCTION=true`) if:
    /// - `NODE_REGISTRATION_PSK` is not set
    /// - Storage initialization fails
    ///
    /// In development mode, falls back to in-memory storage on errors.
    pub fn new() -> Self {
        let is_production = std::env::var(PRODUCTION_ENV)
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let db_path = PathBuf::from("/data/coordinator.db");
        match Self::new_with_storage(db_path) {
            Ok(state) => state,
            Err(e) => {
                if is_production {
                    // In production, configuration errors are fatal - do not fall back
                    panic!(
                        "FATAL: Failed to initialize coordinator in production mode: {}. \
                         Refusing to fall back to in-memory storage with dev PSK.",
                        e
                    );
                }
                error!("Failed to open persistent storage: {}. Using in-memory.", e);
                Self::new_in_memory()
            }
        }
    }

    /// Create new app state with in-memory storage (for testing only).
    ///
    /// # Panics
    ///
    /// Panics if `TORCUS_PRODUCTION=true` is set. In-memory storage with
    /// dev PSK should never be used in production.
    pub fn new_in_memory() -> Self {
        // Check production mode - in-memory is NEVER allowed in production
        let is_production = std::env::var(PRODUCTION_ENV)
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        if is_production {
            panic!(
                "FATAL: new_in_memory() called with {}=true. \
                 In-memory storage with dev PSK is not allowed in production.",
                PRODUCTION_ENV
            );
        }

        debug!("Creating AppState with in-memory storage (development mode)");

        let store = WalletStore::open_in_memory().expect("Failed to create in-memory store");
        let session_store =
            RelaySessionStore::open_in_memory().expect("Failed to create in-memory session store");
        let nodes: Vec<String> = (1..=NUM_PARTIES)
            .map(|i| format!("http://mpc-node-{}:3000", i))
            .collect();

        // Generate ephemeral grant signing key for testing
        let grant_signing_key = SigningKey::generate(&mut OsRng);
        debug!("Generated ephemeral grant signing key for in-memory state");

        // Generate ephemeral CA certificate for testing
        let ca_certificate = CaCertificate::generate().ok();
        debug!("Generated ephemeral CA certificate for in-memory state");

        // Initialize node registry
        let node_registry = NodeRegistry::with_defaults();

        // Use default dev PSK for in-memory/testing (ONLY in development mode)
        let node_psk = DEFAULT_DEV_PSK.to_string();
        warn!(
            "Using default dev PSK. This is only safe for development/testing. \
             Set {}=true and {} in production.",
            PRODUCTION_ENV, NODE_PSK_ENV
        );

        Self {
            nodes,
            wallets: HashMap::new(),
            store,
            relay_sessions: HashMap::new(),
            session_store,
            grant_signing_key,
            node_registry,
            ca_certificate,
            node_psk,
        }
    }

    /// Get node endpoints - uses registry if populated, falls back to hardcoded.
    ///
    /// This provides backward compatibility during the P2P migration:
    /// - If nodes are registered in the registry, use those endpoints
    /// - Otherwise, fall back to the hardcoded node list
    #[allow(dead_code)]
    pub fn get_node_endpoints(&self) -> Vec<String> {
        let registry_endpoints = self.node_registry.online_endpoints();
        if !registry_endpoints.is_empty() {
            registry_endpoints
        } else {
            // Fallback to hardcoded nodes for backward compatibility
            self.nodes.clone()
        }
    }

    /// Save a new wallet to storage.
    pub fn save_wallet(
        &mut self,
        id: Uuid,
        name: String,
        wallet_type: WalletType,
        public_key: String,
        address: String,
    ) -> Result<(), common::MpcWalletError> {
        let wallet = StoredWallet {
            id,
            name: name.clone(),
            wallet_type,
            public_key: public_key.clone(),
            address: address.clone(),
            created_at: Utc::now(),
        };

        // Save to persistent storage
        self.store.save_wallet(&wallet)?;

        // Update in-memory cache
        self.wallets.insert(
            id,
            WalletInfo {
                name,
                wallet_type,
                public_key,
                address,
            },
        );

        Ok(())
    }

    /// Get a stored wallet with full details.
    pub fn get_stored_wallet(
        &self,
        id: Uuid,
    ) -> Result<Option<StoredWallet>, common::MpcWalletError> {
        self.store.get_wallet(id)
    }

    /// List all stored wallets.
    pub fn list_stored_wallets(&self) -> Result<Vec<StoredWallet>, common::MpcWalletError> {
        self.store.list_wallets()
    }

    /// Delete a wallet.
    pub fn delete_wallet(&mut self, id: Uuid) -> Result<bool, common::MpcWalletError> {
        // Remove from persistent storage
        let deleted = self.store.delete_wallet(id)?;

        // Remove from cache
        self.wallets.remove(&id);

        Ok(deleted)
    }

    /// Get the grant signing key (for creating grants).
    pub fn grant_signing_key(&self) -> &SigningKey {
        &self.grant_signing_key
    }

    /// Get the grant verifying key (public key for nodes to verify grants).
    #[allow(dead_code)]
    pub fn grant_verifying_key(&self) -> VerifyingKey {
        self.grant_signing_key.verifying_key()
    }

    /// Get the grant public key as hex string.
    pub fn grant_pubkey_hex(&self) -> String {
        hex::encode(self.grant_signing_key.verifying_key().as_bytes())
    }

    /// Verify the pre-shared key for node registration.
    /// Uses constant-time comparison to prevent timing attacks.
    pub fn verify_node_psk(&self, psk: &str) -> bool {
        use subtle::ConstantTimeEq;
        self.node_psk.as_bytes().ct_eq(psk.as_bytes()).into()
    }

    /// Get the CA certificate for signing node certificates.
    #[allow(dead_code)]
    pub fn ca_certificate(&self) -> Option<&CaCertificate> {
        self.ca_certificate.as_ref()
    }

    /// Check if CA certificate is available.
    #[allow(dead_code)]
    pub fn has_ca_certificate(&self) -> bool {
        self.ca_certificate.is_some()
    }

    /// Get the CA certificate in PEM format (for nodes to fetch).
    #[allow(dead_code)]
    pub fn ca_cert_pem(&self) -> Option<String> {
        self.ca_certificate.as_ref().map(|ca| ca.cert_pem())
    }

    /// Sign a node certificate using the CA.
    /// Returns the signed node certificate or None if CA is not available.
    #[allow(dead_code)]
    pub fn sign_node_certificate(
        &self,
        party_index: u16,
        hostnames: &[String],
    ) -> Result<certs::NodeCertificate, String> {
        let ca = self
            .ca_certificate
            .as_ref()
            .ok_or_else(|| "CA certificate not available".to_string())?;

        ca.sign_node_cert(party_index, hostnames)
            .map_err(|e| format!("Failed to sign node certificate: {}", e))
    }

    /// Clean up expired relay sessions.
    /// Returns the number of sessions removed.
    pub fn cleanup_expired_sessions(&mut self) -> usize {
        let before_count = self.relay_sessions.len();
        let mut expired_ids = Vec::new();

        self.relay_sessions.retain(|session_id, session| {
            if session.is_expired() {
                info!(
                    "Removing expired relay session {} (protocol: {}, inactive for >{}s)",
                    session_id,
                    session.protocol,
                    crate::relay_handlers::SESSION_TTL_SECS
                );
                expired_ids.push(session_id.clone());
                false
            } else {
                true
            }
        });

        // Also clean up from persistent storage
        for session_id in &expired_ids {
            if let Err(e) = self.session_store.delete_session(session_id) {
                warn!(
                    "Failed to delete expired session {} from storage: {}",
                    session_id, e
                );
            }
        }

        // Clean up any other expired sessions in storage
        if let Err(e) = self.session_store.cleanup_expired(SESSION_TTL_SECS) {
            warn!("Failed to cleanup expired sessions from storage: {}", e);
        }

        before_count - self.relay_sessions.len()
    }

    /// Persist a relay session to storage.
    pub fn persist_session(&self, session: &RelaySession) {
        let stored = session.to_stored();
        if let Err(e) = self.session_store.save_session(&stored) {
            warn!("Failed to persist session {}: {}", session.session_id, e);
        }
    }

    /// Delete a relay session from storage.
    #[allow(dead_code)]
    pub fn delete_session_from_storage(&self, session_id: &str) {
        if let Err(e) = self.session_store.delete_session(session_id) {
            warn!(
                "Failed to delete session {} from storage: {}",
                session_id, e
            );
        }
    }

    /// Check if we can create a new relay session (enforces max session limit).
    /// Returns Ok(()) if allowed, or Err with a message if limit reached.
    pub fn can_create_session(&mut self) -> Result<(), String> {
        // First, clean up expired sessions
        self.cleanup_expired_sessions();

        // Check against limit
        if self.relay_sessions.len() >= crate::relay_handlers::MAX_RELAY_SESSIONS {
            Err(format!(
                "Maximum number of relay sessions reached ({}). Try again later.",
                crate::relay_handlers::MAX_RELAY_SESSIONS
            ))
        } else {
            Ok(())
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
