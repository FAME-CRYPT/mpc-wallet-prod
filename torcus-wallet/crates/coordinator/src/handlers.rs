//! HTTP request handlers for the coordinator.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use bitcoin::secp256k1::{Secp256k1, SecretKey};
use bitcoin::Network;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, error, info, trace};
use uuid::Uuid;

use common::{
    CreateWalletRequest, CreateWalletResponse, DerivedAddress, MpcHdWallet, WalletType,
    NUM_PARTIES, THRESHOLD,
};

use crate::state::AppState;

/// Parse BITCOIN_NETWORK environment variable into a bitcoin::Network.
fn get_bitcoin_network() -> Result<Network, (StatusCode, String)> {
    let network_str = std::env::var("BITCOIN_NETWORK").unwrap_or_else(|_| "testnet".to_string());
    match network_str.as_str() {
        "mainnet" => Ok(Network::Bitcoin),
        "testnet" => Ok(Network::Testnet),
        "regtest" => Ok(Network::Regtest),
        other => Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid BITCOIN_NETWORK: '{}'. Must be mainnet, testnet, or regtest",
                other
            ),
        )),
    }
}

/// Guard that blocks mainnet usage until the system is production-ready.
/// Call this at API entry points that create wallets or sign transactions.
pub fn reject_mainnet() -> Result<(), (StatusCode, String)> {
    let network_str = std::env::var("BITCOIN_NETWORK").unwrap_or_else(|_| "testnet".to_string());
    if network_str == "mainnet" {
        return Err((
            StatusCode::BAD_REQUEST,
            "Mainnet is not supported yet. This system is not production-ready.".to_string(),
        ));
    }
    Ok(())
}

/// Health check endpoint.
pub async fn health_check() -> impl IntoResponse {
    trace!("Health check endpoint called");
    trace!("Returning healthy status");
    Json(serde_json::json!({
        "status": "healthy",
        "service": "mpc-wallet-coordinator"
    }))
}

/// Response for grant public key endpoint.
#[derive(Debug, Serialize)]
pub struct GrantPubkeyResponse {
    /// Ed25519 public key in hex format (32 bytes = 64 hex chars).
    pub public_key: String,
    /// Key type (always "ed25519" for now).
    pub key_type: String,
}

/// Get the grant signing public key.
///
/// GET /grant/pubkey
///
/// Nodes call this endpoint at startup to fetch the public key
/// used to verify grant signatures.
pub async fn grant_pubkey(State(state): State<Arc<RwLock<AppState>>>) -> impl IntoResponse {
    debug!("Grant pubkey endpoint called");

    let pubkey_hex = {
        let s = state.read().await;
        s.grant_pubkey_hex()
    };

    Json(GrantPubkeyResponse {
        public_key: pubkey_hex,
        key_type: "ed25519".to_string(),
    })
}

/// Status of a single MPC node.
#[derive(Debug, Serialize)]
pub struct NodeStatus {
    pub node_index: usize,
    pub url: String,
    pub reachable: bool,
    pub primes_available: bool,
    pub aux_info_available: bool,
    pub cggmp24_ready: bool,
    pub cggmp24_key_shares: u64,
    pub frost_key_shares: u64,
    pub error: Option<String>,
}

/// Response for aggregated node status.
#[derive(Debug, Serialize)]
pub struct NodesStatusResponse {
    pub nodes: Vec<NodeStatus>,
    pub all_reachable: bool,
    pub all_cggmp24_ready: bool,
    pub total_cggmp24_shares: u64,
    pub total_frost_shares: u64,
}

/// Get aggregated status of all MPC nodes.
///
/// GET /nodes/status
///
/// This endpoint queries all MPC nodes and returns their status.
/// The CLI should use this instead of querying nodes directly.
pub async fn nodes_status(State(state): State<Arc<RwLock<AppState>>>) -> impl IntoResponse {
    debug!("Nodes status endpoint called");

    let nodes = {
        let s = state.read().await;
        s.nodes.clone()
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    // Query all nodes in parallel
    let futures: Vec<_> = nodes
        .iter()
        .enumerate()
        .map(|(i, node_url)| {
            let client = client.clone();
            let url = node_url.clone();
            async move {
                let status_url = format!("{}/status", url);
                match client.get(&status_url).send().await {
                    Ok(resp) => {
                        if let Ok(status) = resp.json::<serde_json::Value>().await {
                            NodeStatus {
                                node_index: i,
                                url,
                                reachable: true,
                                primes_available: status["primes"]["available"]
                                    .as_bool()
                                    .unwrap_or(false),
                                aux_info_available: status["aux_info"]["available"]
                                    .as_bool()
                                    .unwrap_or(false),
                                cggmp24_ready: status["cggmp24_ready"].as_bool().unwrap_or(false),
                                cggmp24_key_shares: status["cggmp24_key_shares"]
                                    .as_u64()
                                    .unwrap_or(0),
                                frost_key_shares: status["frost_key_shares"].as_u64().unwrap_or(0),
                                error: None,
                            }
                        } else {
                            NodeStatus {
                                node_index: i,
                                url,
                                reachable: true,
                                primes_available: false,
                                aux_info_available: false,
                                cggmp24_ready: false,
                                cggmp24_key_shares: 0,
                                frost_key_shares: 0,
                                error: Some("Invalid response format".to_string()),
                            }
                        }
                    }
                    Err(e) => NodeStatus {
                        node_index: i,
                        url,
                        reachable: false,
                        primes_available: false,
                        aux_info_available: false,
                        cggmp24_ready: false,
                        cggmp24_key_shares: 0,
                        frost_key_shares: 0,
                        error: Some(e.to_string()),
                    },
                }
            }
        })
        .collect();

    let node_statuses: Vec<NodeStatus> = futures::future::join_all(futures).await;

    let all_reachable = node_statuses.iter().all(|n| n.reachable);
    let all_cggmp24_ready = node_statuses.iter().all(|n| n.cggmp24_ready);
    let total_cggmp24_shares: u64 = node_statuses.iter().map(|n| n.cggmp24_key_shares).sum();
    let total_frost_shares: u64 = node_statuses.iter().map(|n| n.frost_key_shares).sum();

    Json(NodesStatusResponse {
        nodes: node_statuses,
        all_reachable,
        all_cggmp24_ready,
        total_cggmp24_shares,
        total_frost_shares,
    })
}

/// Aggregated key share information.
#[derive(Debug, Serialize)]
pub struct KeyShareInfo {
    pub wallet_id: String,
    pub public_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    pub threshold: u16,
    pub num_parties: u16,
    pub nodes_with_shares: Vec<usize>,
}

/// Response for aggregated key shares.
#[derive(Debug, Serialize)]
pub struct KeySharesResponse {
    pub keyshares: Vec<KeyShareInfo>,
}

/// Get aggregated CGGMP24 key shares from all nodes.
///
/// GET /cggmp24/keyshares
///
/// This endpoint queries all MPC nodes and aggregates key share information.
pub async fn cggmp24_keyshares(State(state): State<Arc<RwLock<AppState>>>) -> impl IntoResponse {
    debug!("CGGMP24 keyshares endpoint called");

    let nodes = {
        let s = state.read().await;
        s.nodes.clone()
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    // Query all nodes in parallel
    let futures: Vec<_> = nodes
        .iter()
        .enumerate()
        .map(|(i, node_url)| {
            let client = client.clone();
            let url = node_url.clone();
            async move {
                let keyshares_url = format!("{}/cggmp24/keyshares", url);
                match client.get(&keyshares_url).send().await {
                    Ok(resp) => {
                        if let Ok(data) = resp.json::<serde_json::Value>().await {
                            Some((i, data))
                        } else {
                            None
                        }
                    }
                    Err(_) => None,
                }
            }
        })
        .collect();

    let results: Vec<Option<(usize, serde_json::Value)>> = futures::future::join_all(futures).await;

    // Aggregate key shares by wallet_id
    let mut wallet_shares: std::collections::HashMap<String, KeyShareInfo> =
        std::collections::HashMap::new();

    for result in results.into_iter().flatten() {
        let (node_index, data) = result;
        if let Some(keyshares) = data["keyshares"].as_array() {
            for ks in keyshares {
                if let Some(wallet_id) = ks["wallet_id"].as_str() {
                    let entry = wallet_shares
                        .entry(wallet_id.to_string())
                        .or_insert_with(|| KeyShareInfo {
                            wallet_id: wallet_id.to_string(),
                            public_key: ks["public_key"].as_str().unwrap_or("").to_string(),
                            address: None, // CGGMP24 doesn't include address in node response
                            threshold: ks["threshold"].as_u64().unwrap_or(0) as u16,
                            num_parties: ks["num_parties"].as_u64().unwrap_or(0) as u16,
                            nodes_with_shares: Vec::new(),
                        });
                    entry.nodes_with_shares.push(node_index);
                }
            }
        }
    }

    Json(KeySharesResponse {
        keyshares: wallet_shares.into_values().collect(),
    })
}

/// Get aggregated FROST key shares from all nodes.
///
/// GET /frost/keyshares
///
/// This endpoint queries all MPC nodes and aggregates FROST key share information.
pub async fn frost_keyshares(State(state): State<Arc<RwLock<AppState>>>) -> impl IntoResponse {
    debug!("FROST keyshares endpoint called");

    let nodes = {
        let s = state.read().await;
        s.nodes.clone()
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    // Query all nodes in parallel
    let futures: Vec<_> = nodes
        .iter()
        .enumerate()
        .map(|(i, node_url)| {
            let client = client.clone();
            let url = node_url.clone();
            async move {
                let keyshares_url = format!("{}/frost/keyshares", url);
                match client.get(&keyshares_url).send().await {
                    Ok(resp) => {
                        if let Ok(data) = resp.json::<serde_json::Value>().await {
                            Some((i, data))
                        } else {
                            None
                        }
                    }
                    Err(_) => None,
                }
            }
        })
        .collect();

    let results: Vec<Option<(usize, serde_json::Value)>> = futures::future::join_all(futures).await;

    // Aggregate key shares by wallet_id
    let mut wallet_shares: std::collections::HashMap<String, KeyShareInfo> =
        std::collections::HashMap::new();

    for result in results.into_iter().flatten() {
        let (node_index, data) = result;
        if let Some(keyshares) = data["keyshares"].as_array() {
            for ks in keyshares {
                if let Some(wallet_id) = ks["wallet_id"].as_str() {
                    let entry = wallet_shares
                        .entry(wallet_id.to_string())
                        .or_insert_with(|| KeyShareInfo {
                            wallet_id: wallet_id.to_string(),
                            public_key: ks["public_key"].as_str().unwrap_or("").to_string(),
                            address: ks["address"].as_str().map(|s| s.to_string()),
                            threshold: ks["threshold"].as_u64().unwrap_or(0) as u16,
                            num_parties: ks["num_parties"].as_u64().unwrap_or(0) as u16,
                            nodes_with_shares: Vec::new(),
                        });
                    entry.nodes_with_shares.push(node_index);
                }
            }
        }
    }

    Json(KeySharesResponse {
        keyshares: wallet_shares.into_values().collect(),
    })
}

/// System info endpoint.
pub async fn system_info(State(state): State<Arc<RwLock<AppState>>>) -> impl IntoResponse {
    debug!("System info endpoint called");

    trace!("Acquiring read lock on application state");
    let state = state.read().await;
    trace!("Read lock acquired");

    let wallets_count = state.wallets.len();
    debug!(
        "Current state: {} nodes configured, {} wallets created",
        state.nodes.len(),
        wallets_count
    );

    trace!("Building response JSON");
    Json(serde_json::json!({
        "threshold": THRESHOLD,
        "parties": NUM_PARTIES,
        "nodes": state.nodes,
        "wallets_count": wallets_count
    }))
}

/// Generate a placeholder keypair for wallet creation.
fn generate_placeholder_keypair(
    wallet_type: WalletType,
) -> Result<(String, String), (StatusCode, String)> {
    let secp = Secp256k1::new();
    let mut rng = rand::thread_rng();
    let mut secret_bytes = [0u8; 32];
    rng.fill_bytes(&mut secret_bytes);

    let secret_key = SecretKey::from_slice(&secret_bytes).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to generate key: {}", e),
        )
    })?;

    let public_key = secret_key.public_key(&secp);
    let public_key_hex = hex::encode(public_key.serialize());

    // Determine network
    let network = get_bitcoin_network()?;

    // Derive address based on wallet type
    let address = match wallet_type {
        WalletType::Bitcoin => {
            let compressed = bitcoin::CompressedPublicKey(public_key);
            bitcoin::Address::p2wpkh(&compressed, network).to_string()
        }
        WalletType::Taproot => {
            let (xonly, _parity) = public_key.x_only_public_key();
            let secp_xonly = bitcoin::secp256k1::XOnlyPublicKey::from_slice(&xonly.serialize())
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to convert key: {}", e),
                    )
                })?;

            bitcoin::Address::p2tr_tweaked(
                bitcoin::key::TweakedPublicKey::dangerous_assume_tweaked(secp_xonly),
                network,
            )
            .to_string()
        }
        WalletType::Ethereum => {
            format!("0x{}", &public_key_hex[2..42])
        }
    };

    Ok((public_key_hex, address))
}

/// Create a new MPC wallet placeholder.
///
/// This creates a wallet record that will be updated with the actual public key
/// after distributed key generation (CGGMP24 or FROST) completes.
pub async fn create_wallet(
    State(state): State<Arc<RwLock<AppState>>>,
    Json(request): Json<CreateWalletRequest>,
) -> Result<Json<CreateWalletResponse>, (StatusCode, String)> {
    reject_mainnet()?;
    info!("Creating wallet placeholder: name={}", request.name);

    // Generate a placeholder keypair - this will be replaced by DKG output
    let (public_key_hex, address) = generate_placeholder_keypair(request.wallet_type)?;

    let wallet_id = uuid::Uuid::new_v4();

    // Store in database
    {
        let mut s = state.write().await;
        s.save_wallet(
            wallet_id,
            request.name.clone(),
            request.wallet_type,
            public_key_hex.clone(),
            address.clone(),
        )
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    info!(
        "Wallet placeholder created: id={}, address={}",
        wallet_id, address
    );

    Ok(Json(CreateWalletResponse {
        wallet_id,
        wallet_type: request.wallet_type,
        address,
        public_key: public_key_hex,
        message: format!(
            "Wallet '{}' placeholder created. Run DKG to generate the actual threshold key.",
            request.name
        ),
    }))
}

// ============================================================================
// BIP32 HD Address Derivation
// ============================================================================

/// Query parameters for address derivation.
#[derive(Debug, Deserialize)]
pub struct DeriveAddressQuery {
    /// Starting index (default: 0).
    #[serde(default)]
    pub start: u32,
    /// Number of addresses to derive (default: 1, max: 100).
    #[serde(default = "default_count")]
    pub count: u32,
    /// Address type: "receiving" or "change" (default: "receiving").
    #[serde(default = "default_address_type")]
    pub address_type: String,
}

fn default_count() -> u32 {
    1
}

fn default_address_type() -> String {
    "receiving".to_string()
}

/// Response for derived addresses.
#[derive(Debug, Serialize)]
pub struct DeriveAddressResponse {
    pub wallet_id: Uuid,
    pub addresses: Vec<DerivedAddress>,
    pub message: String,
}

/// Derive HD addresses for a wallet.
///
/// GET /wallet/:wallet_id/derive?start=0&count=5&address_type=receiving
pub async fn derive_addresses(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(wallet_id): Path<Uuid>,
    Query(query): Query<DeriveAddressQuery>,
) -> Result<Json<DeriveAddressResponse>, (StatusCode, String)> {
    info!("========================================");
    info!("  DERIVE ADDRESSES REQUEST");
    info!("========================================");
    info!("Wallet ID: {}", wallet_id);
    info!("Start index: {}", query.start);
    info!("Count: {}", query.count);
    info!("Address type: {}", query.address_type);

    // Limit count to prevent abuse
    let count = query.count.min(100);

    // Get wallet info
    let wallet_info = {
        let state_guard = state.read().await;
        state_guard.wallets.get(&wallet_id).cloned()
    };

    let wallet_info = wallet_info.ok_or_else(|| {
        error!("Wallet not found: {}", wallet_id);
        (StatusCode::NOT_FOUND, "Wallet not found".to_string())
    })?;

    // Only Bitcoin wallets support HD derivation currently
    if wallet_info.wallet_type != WalletType::Bitcoin {
        return Err((
            StatusCode::BAD_REQUEST,
            "HD derivation only supported for Bitcoin wallets".to_string(),
        ));
    }

    // Parse the public key
    let public_key_bytes = hex::decode(&wallet_info.public_key).map_err(|e| {
        error!("Invalid public key in wallet: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    // Get network from environment
    let network = get_bitcoin_network()?;

    // Create HD wallet
    let hd_wallet = MpcHdWallet::new(&public_key_bytes, network).map_err(|e| {
        error!("Failed to create HD wallet: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    // Derive addresses
    let addresses = if query.address_type == "change" {
        (query.start..query.start + count)
            .map(|i| hd_wallet.get_change_address(i))
            .collect::<Result<Vec<_>, _>>()
    } else {
        hd_wallet.get_receiving_addresses(query.start, count)
    }
    .map_err(|e| {
        error!("Failed to derive addresses: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    info!("Derived {} addresses", addresses.len());
    for addr in &addresses {
        debug!("  {} -> {}", addr.path, addr.address);
    }

    Ok(Json(DeriveAddressResponse {
        wallet_id,
        addresses,
        message: format!(
            "Derived {} {} addresses starting at index {}",
            count, query.address_type, query.start
        ),
    }))
}

/// Get a single address at a specific index.
///
/// GET /wallet/:wallet_id/address/:index
pub async fn get_address(
    State(state): State<Arc<RwLock<AppState>>>,
    Path((wallet_id, index)): Path<(Uuid, u32)>,
) -> Result<Json<DerivedAddress>, (StatusCode, String)> {
    debug!("Get address: wallet={}, index={}", wallet_id, index);

    // Get wallet info
    let wallet_info = {
        let state_guard = state.read().await;
        state_guard.wallets.get(&wallet_id).cloned()
    };

    let wallet_info =
        wallet_info.ok_or_else(|| (StatusCode::NOT_FOUND, "Wallet not found".to_string()))?;

    if wallet_info.wallet_type != WalletType::Bitcoin {
        return Err((
            StatusCode::BAD_REQUEST,
            "HD derivation only supported for Bitcoin wallets".to_string(),
        ));
    }

    let public_key_bytes = hex::decode(&wallet_info.public_key)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Get network from environment
    let network = get_bitcoin_network()?;

    let hd_wallet = MpcHdWallet::new(&public_key_bytes, network)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let address = hd_wallet
        .get_receiving_address(index)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(address))
}

/// List all wallets.
///
/// GET /wallets
pub async fn list_wallets(
    State(state): State<Arc<RwLock<AppState>>>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    debug!("List wallets endpoint called");

    let state_guard = state.read().await;

    let wallets = state_guard
        .list_stored_wallets()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let wallet_json: Vec<_> = wallets
        .iter()
        .map(|w| {
            serde_json::json!({
                "wallet_id": w.id,
                "name": w.name,
                "wallet_type": w.wallet_type,
                "public_key": w.public_key,
                "address": w.address,
                "created_at": w.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "wallets": wallet_json,
        "count": wallets.len()
    })))
}

/// Get a specific wallet.
///
/// GET /wallet/:wallet_id
pub async fn get_wallet(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(wallet_id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    debug!("Get wallet endpoint called: {}", wallet_id);

    let state_guard = state.read().await;

    let wallet = state_guard
        .get_stored_wallet(wallet_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Wallet not found".to_string()))?;

    Ok(Json(serde_json::json!({
        "wallet_id": wallet.id,
        "name": wallet.name,
        "wallet_type": wallet.wallet_type,
        "public_key": wallet.public_key,
        "address": wallet.address,
        "created_at": wallet.created_at.to_rfc3339(),
    })))
}

/// Delete a wallet.
///
/// DELETE /wallet/:wallet_id
pub async fn delete_wallet(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(wallet_id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    info!("Delete wallet request: {}", wallet_id);

    let mut state_guard = state.write().await;

    let deleted = state_guard
        .delete_wallet(wallet_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if deleted {
        info!("Wallet {} deleted successfully", wallet_id);
        Ok(Json(serde_json::json!({
            "success": true,
            "message": format!("Wallet {} deleted", wallet_id)
        })))
    } else {
        Err((StatusCode::NOT_FOUND, "Wallet not found".to_string()))
    }
}

// ============================================================================
// Update Wallet Public Key (for CGGMP24)
// ============================================================================

/// Request to update wallet's public key.
#[derive(Debug, Deserialize)]
pub struct UpdateWalletPubkeyRequest {
    pub public_key: String,
}

/// Update wallet's public key.
/// This is used after CGGMP24 keygen to replace the legacy DKG public key.
///
/// PUT /wallet/:wallet_id/pubkey
pub async fn update_wallet_pubkey(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(wallet_id): Path<Uuid>,
    Json(request): Json<UpdateWalletPubkeyRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    reject_mainnet()?;
    info!("Updating public key for wallet: {}", wallet_id);

    // Get existing wallet
    let existing_wallet = {
        let s = state.read().await;
        s.get_stored_wallet(wallet_id)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or_else(|| (StatusCode::NOT_FOUND, "Wallet not found".to_string()))?
    };

    // Decode and validate new public key
    let public_key_bytes = hex::decode(&request.public_key).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid public key hex: {}", e),
        )
    })?;

    if public_key_bytes.len() != 33 {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Public key must be 33 bytes (compressed), got {}",
                public_key_bytes.len()
            ),
        ));
    }

    // Derive new address from the public key
    use bitcoin::secp256k1::PublicKey;
    use bitcoin::{Address, CompressedPublicKey};

    let secp_pubkey = PublicKey::from_slice(&public_key_bytes).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid secp256k1 public key: {}", e),
        )
    })?;

    // Determine network from environment
    let network = get_bitcoin_network()?;

    let compressed = CompressedPublicKey(secp_pubkey);
    let new_address = Address::p2wpkh(&compressed, network);

    info!("Old public key: {}", existing_wallet.public_key);
    info!("New public key: {}", request.public_key);
    info!("Old address: {}", existing_wallet.address);
    info!("New address: {}", new_address);

    // Update the wallet
    {
        let mut s = state.write().await;
        s.save_wallet(
            wallet_id,
            existing_wallet.name.clone(),
            existing_wallet.wallet_type,
            request.public_key.clone(),
            new_address.to_string(),
        )
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    info!("Wallet {} public key updated successfully", wallet_id);

    Ok(Json(serde_json::json!({
        "success": true,
        "wallet_id": wallet_id.to_string(),
        "public_key": request.public_key,
        "address": new_address.to_string(),
        "message": "Wallet public key updated for CGGMP24"
    })))
}

/// Update a Taproot wallet's public key and address after FROST keygen.
///
/// PUT /wallet/:wallet_id/taproot-pubkey
pub async fn update_taproot_wallet(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(wallet_id): Path<Uuid>,
    Json(request): Json<UpdateWalletPubkeyRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    reject_mainnet()?;
    info!("Updating Taproot wallet: {}", wallet_id);

    // Get existing wallet
    let existing_wallet = {
        let s = state.read().await;
        s.get_stored_wallet(wallet_id)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or_else(|| (StatusCode::NOT_FOUND, "Wallet not found".to_string()))?
    };

    if existing_wallet.wallet_type != common::WalletType::Taproot {
        return Err((
            StatusCode::BAD_REQUEST,
            "This endpoint is for Taproot wallets only".to_string(),
        ));
    }

    // Decode and validate new public key (x-only, 32 bytes)
    let public_key_bytes = hex::decode(&request.public_key).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid public key hex: {}", e),
        )
    })?;

    if public_key_bytes.len() != 32 {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Taproot public key must be 32 bytes (x-only), got {}",
                public_key_bytes.len()
            ),
        ));
    }

    // Determine network from environment
    let network = get_bitcoin_network()?;

    // Derive Taproot address from the x-only public key
    let new_address = common::derive_bitcoin_address_taproot_from_xonly(&public_key_bytes, network)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("Failed to derive address: {}", e),
            )
        })?;

    info!("Old public key: {}", existing_wallet.public_key);
    info!("New public key: {}", request.public_key);
    info!("Old address: {}", existing_wallet.address);
    info!("New address: {}", new_address);

    // Update the wallet
    {
        let mut s = state.write().await;
        s.save_wallet(
            wallet_id,
            existing_wallet.name.clone(),
            existing_wallet.wallet_type,
            request.public_key.clone(),
            new_address.clone(),
        )
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    info!("Taproot wallet {} updated successfully", wallet_id);

    Ok(Json(serde_json::json!({
        "success": true,
        "wallet_id": wallet_id.to_string(),
        "public_key": request.public_key,
        "address": new_address,
        "message": "Taproot wallet updated with FROST public key"
    })))
}
