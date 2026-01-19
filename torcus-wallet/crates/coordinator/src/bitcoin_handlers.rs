//! Bitcoin-specific handlers for balance checking and sending.

use std::str::FromStr;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use bitcoin::secp256k1::PublicKey;
use tokio::sync::RwLock;
use tracing::{debug, error, info};
use uuid::Uuid;

use chains::bitcoin::{BalanceResponse, BitcoinNetwork, Utxo};
use common::{SendBitcoinRequest, SendBitcoinResponse, SigningGrant, WalletType, THRESHOLD};

use crate::bitcoin_client::BitcoinBackend;
use crate::handlers::reject_mainnet;
use crate::state::AppState;

/// Get balance for a wallet.
///
/// GET /wallet/:wallet_id/balance
pub async fn get_balance(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(wallet_id): Path<Uuid>,
) -> Result<Json<BalanceResponse>, (StatusCode, String)> {
    info!("Get balance for wallet: {}", wallet_id);

    // Get wallet
    let wallet = {
        let s = state.read().await;
        s.get_stored_wallet(wallet_id)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or_else(|| (StatusCode::NOT_FOUND, "Wallet not found".to_string()))?
    };

    // Allow both Bitcoin (SegWit) and Taproot wallet types
    if wallet.wallet_type != WalletType::Bitcoin && wallet.wallet_type != WalletType::Taproot {
        return Err((
            StatusCode::BAD_REQUEST,
            "Balance only available for Bitcoin/Taproot wallets".to_string(),
        ));
    }

    // Query blockchain
    let backend = BitcoinBackend::from_env().map_err(|e| {
        error!("Failed to create Bitcoin backend: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    let info = backend
        .get_address_info(&wallet.address)
        .await
        .map_err(|e| {
            error!("Failed to get address info: {}", e);
            (StatusCode::BAD_GATEWAY, e.to_string())
        })?;

    let response = BalanceResponse::from_address_info(wallet.address.clone(), &info);

    info!(
        "Balance for {}: {} sats (confirmed: {})",
        wallet.address, response.total_sats, response.confirmed_sats
    );

    Ok(Json(response))
}

/// Get faucet information for testnet.
///
/// GET /wallet/:wallet_id/faucet
pub async fn get_faucet_info(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(wallet_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    info!("Get faucet info for wallet: {}", wallet_id);

    // Get wallet
    let wallet = {
        let s = state.read().await;
        s.get_stored_wallet(wallet_id)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or_else(|| (StatusCode::NOT_FOUND, "Wallet not found".to_string()))?
    };

    // Allow both Bitcoin (SegWit) and Taproot wallet types
    if wallet.wallet_type != WalletType::Bitcoin && wallet.wallet_type != WalletType::Taproot {
        return Err((
            StatusCode::BAD_REQUEST,
            "Faucet only available for Bitcoin/Taproot wallets".to_string(),
        ));
    }

    let backend = BitcoinBackend::from_env()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let network_name = backend.network_name();
    let is_regtest = backend.is_regtest();

    if is_regtest {
        // For regtest, provide mine endpoint instructions instead of faucets
        Ok(Json(serde_json::json!({
            "wallet_id": wallet_id,
            "address": wallet.address,
            "network": network_name,
            "instructions": "Use the /wallet/{id}/mine endpoint to mine blocks and receive regtest coins",
            "mine_endpoint": format!("/wallet/{}/mine", wallet_id),
            "example": format!("POST /wallet/{}/mine with body: {{\"blocks\": 101}}", wallet_id),
            "note": "Mine at least 101 blocks for coinbase maturity before spending"
        })))
    } else {
        // For testnet/mainnet, provide faucet info
        Ok(Json(serde_json::json!({
            "wallet_id": wallet_id,
            "address": wallet.address,
            "network": network_name,
            "instructions": "Copy your address and paste it into one of the faucets below to receive testnet BTC",
            "faucets": BitcoinNetwork::faucet_urls(),
            "explorer_url": backend.address_url(&wallet.address),
            "note": "Testnet coins have no real value - they're for testing only"
        })))
    }
}

/// Send Bitcoin from a wallet using threshold ECDSA signing.
///
/// POST /wallet/:wallet_id/send
pub async fn send_bitcoin(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(wallet_id): Path<Uuid>,
    Json(request): Json<SendBitcoinRequest>,
) -> Result<Json<SendBitcoinResponse>, (StatusCode, String)> {
    reject_mainnet()?;
    info!("========================================");
    info!("  SEND BITCOIN REQUEST");
    info!("========================================");
    info!("Wallet ID: {}", wallet_id);
    info!("To: {}", request.to_address);
    info!("Amount: {} sats", request.amount_sats);

    // Get wallet and nodes
    let (wallet, nodes) = {
        let s = state.read().await;
        let wallet = s
            .get_stored_wallet(wallet_id)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or_else(|| (StatusCode::NOT_FOUND, "Wallet not found".to_string()))?;
        (wallet, s.nodes.clone())
    };

    if wallet.wallet_type == WalletType::Taproot {
        return Err((
            StatusCode::BAD_REQUEST,
            "For Taproot wallets, use /wallet/{id}/send-taproot endpoint instead".to_string(),
        ));
    }

    if wallet.wallet_type != WalletType::Bitcoin {
        return Err((
            StatusCode::BAD_REQUEST,
            "Send only available for Bitcoin (SegWit) wallets".to_string(),
        ));
    }

    // Get UTXOs and balance
    let backend = BitcoinBackend::from_env().map_err(|e| {
        error!("Failed to create Bitcoin backend: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    let utxos = backend.get_utxos(&wallet.address).await.map_err(|e| {
        error!("Failed to get UTXOs: {}", e);
        (StatusCode::BAD_GATEWAY, e.to_string())
    })?;

    if utxos.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "No UTXOs available. Get testnet coins from a faucet first.".to_string(),
        ));
    }

    let total_balance: u64 = utxos.iter().map(|u| u.value).sum();
    info!(
        "Available balance: {} sats from {} UTXOs",
        total_balance,
        utxos.len()
    );

    // Get fee estimate
    let fee_estimates = backend.get_fee_estimates().await.unwrap_or_default();
    let fee_rate = request
        .fee_rate
        .unwrap_or_else(|| fee_estimates.recommended().max(1));
    info!("Using fee rate: {} sat/vB", fee_rate);

    // Build the unsigned transaction
    let public_key_bytes = hex::decode(&wallet.public_key).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Invalid public key: {}", e),
        )
    })?;

    let (unsigned_tx, sighashes, fee_sats) = build_unsigned_transaction(
        &utxos,
        &request.to_address,
        request.amount_sats,
        fee_rate,
        &wallet.address,
        &public_key_bytes,
    )
    .map_err(|e| {
        error!("Failed to build transaction: {}", e);
        (StatusCode::BAD_REQUEST, e)
    })?;

    info!(
        "Built unsigned tx with {} inputs, fee={} sats",
        sighashes.len(),
        fee_sats
    );

    // Sign each input using threshold ECDSA
    let participant_indices: Vec<u16> = (0..THRESHOLD).collect();
    let http_client = reqwest::Client::new();

    let mut signatures: Vec<Vec<u8>> = Vec::new();

    for (input_idx, sighash) in sighashes.iter().enumerate() {
        info!(
            "Signing input {} with sighash: {}",
            input_idx,
            hex::encode(sighash)
        );

        let signature = run_threshold_signing(
            &http_client,
            &nodes,
            wallet_id,
            sighash,
            &participant_indices,
            &state,
        )
        .await
        .map_err(|e| {
            error!("Threshold signing failed for input {}: {}", input_idx, e);
            (StatusCode::INTERNAL_SERVER_ERROR, e)
        })?;

        signatures.push(signature);
    }

    info!("All {} inputs signed successfully", signatures.len());

    // Create the signed transaction
    let signed_tx_hex =
        finalize_transaction(unsigned_tx, &signatures, &public_key_bytes).map_err(|e| {
            error!("Failed to finalize transaction: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e)
        })?;

    info!("Transaction finalized, broadcasting...");

    // Broadcast transaction
    let txid = backend.broadcast_tx(&signed_tx_hex).await.map_err(|e| {
        error!("Failed to broadcast: {}", e);
        (StatusCode::BAD_GATEWAY, format!("Broadcast failed: {}", e))
    })?;

    info!("========================================");
    info!("  TRANSACTION BROADCAST SUCCESS");
    info!("========================================");
    info!("TXID: {}", txid);
    info!("Explorer: {}", backend.tx_url(&txid));

    Ok(Json(SendBitcoinResponse {
        txid: txid.clone(),
        amount_sats: request.amount_sats,
        fee_sats,
        to_address: request.to_address,
        message: format!("Transaction broadcast! View at: {}", backend.tx_url(&txid)),
    }))
}

/// Run CGGMP24 threshold signing.
///
/// This coordinates the real CGGMP24 threshold signing protocol across nodes.
/// All participating nodes run the protocol together and produce the same signature.
async fn run_threshold_signing(
    http_client: &reqwest::Client,
    nodes: &[String],
    wallet_id: Uuid,
    message_hash: &[u8; 32],
    participants: &[u16],
    state: &Arc<RwLock<AppState>>,
) -> Result<Vec<u8>, String> {
    use crate::relay_handlers::RelaySession;

    info!("========================================");
    info!("  CGGMP24 THRESHOLD SIGNING");
    info!("========================================");
    info!("Wallet ID: {}", wallet_id);
    info!("Message hash: {}", hex::encode(message_hash));
    info!("Participants: {:?}", participants);

    // Create signing grant
    let grant = {
        let s = state.read().await;
        SigningGrant::new(
            wallet_id.to_string(),
            *message_hash,
            THRESHOLD,
            participants.to_vec(),
            s.grant_signing_key(),
        )
    };

    // Use grant-derived session ID for determinism
    let session_id = grant.session_id();
    let message_hash_hex = hex::encode(message_hash);
    let wallet_id_str = wallet_id.to_string();

    info!("Grant ID: {}", grant.grant_id);

    // Create relay session for message passing
    {
        let mut s = state.write().await;
        // Check session limit
        s.can_create_session()
            .map_err(|e| format!("Cannot create session: {}", e))?;
        let session = RelaySession::new(
            session_id.clone(),
            "signing".to_string(),
            participants.to_vec(),
        )
        .map_err(|e| format!("Failed to create relay session: {}", e))?;
        s.relay_sessions.insert(session_id.clone(), session);
        info!("Created relay session {} for signing", session_id);
    }

    // Start signing on all participating nodes in parallel
    let mut handles = Vec::new();

    for &party_idx in participants {
        let node_url = nodes
            .get(party_idx as usize)
            .ok_or_else(|| format!("Invalid party index: {}", party_idx))?
            .clone();

        let client = http_client.clone();
        let session_id = session_id.clone();
        let message_hash_hex = message_hash_hex.clone();
        let wallet_id_str = wallet_id_str.clone();
        let parties = participants.to_vec();
        let grant_clone = grant.clone();

        let handle = tokio::spawn(async move {
            let sign_url = format!("{}/cggmp24/sign", node_url);

            debug!("Starting signing on node {} at {}", party_idx, sign_url);

            let response = client
                .post(&sign_url)
                .json(&serde_json::json!({
                    "session_id": session_id,
                    "wallet_id": wallet_id_str,
                    "message_hash": message_hash_hex,
                    "parties": parties,
                    "grant": grant_clone
                }))
                .timeout(std::time::Duration::from_secs(120)) // 2 minute timeout
                .send()
                .await;

            match response {
                Ok(resp) => {
                    if resp.status().is_success() {
                        resp.json::<serde_json::Value>().await.ok()
                    } else {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        error!(
                            "Node {} signing failed with status {}: {}",
                            party_idx, status, body
                        );
                        None
                    }
                }
                Err(e) => {
                    error!("Failed to reach node {} for signing: {}", party_idx, e);
                    None
                }
            }
        });

        handles.push((party_idx, handle));
    }

    // Wait for all nodes and collect results
    let mut signature_der: Option<Vec<u8>> = None;
    let mut errors: Vec<String> = Vec::new();

    for (party_idx, handle) in handles {
        match handle.await {
            Ok(Some(response)) => {
                if response
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    if let Some(der_hex) = response.get("signature_der").and_then(|v| v.as_str()) {
                        match hex::decode(der_hex) {
                            Ok(der) => {
                                info!(
                                    "Got signature from party {}: {} bytes",
                                    party_idx,
                                    der.len()
                                );
                                if signature_der.is_none() {
                                    signature_der = Some(der);
                                }
                            }
                            Err(e) => {
                                errors.push(format!(
                                    "Party {} returned invalid DER: {}",
                                    party_idx, e
                                ));
                            }
                        }
                    }
                } else {
                    let error = response
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown error");
                    errors.push(format!("Party {} failed: {}", party_idx, error));
                }
            }
            Ok(None) => {
                errors.push(format!("Party {} returned no response", party_idx));
            }
            Err(e) => {
                errors.push(format!("Party {} task failed: {}", party_idx, e));
            }
        }
    }

    // Clean up relay session
    {
        let mut s = state.write().await;
        s.relay_sessions.remove(&session_id);
        debug!("Removed relay session {}", session_id);
    }

    // Return the signature if we got one
    if let Some(der) = signature_der {
        info!("========================================");
        info!("  CGGMP24 SIGNING COMPLETE");
        info!("========================================");
        info!("Signature (DER): {} bytes", der.len());
        Ok(der)
    } else {
        error!("All signing attempts failed: {:?}", errors);
        Err(format!("CGGMP24 signing failed: {:?}", errors))
    }
}

// Legacy key reconstruction functions removed - now using CGGMP24 threshold signing

/// Create a dummy witness for P2WPKH to calculate accurate vsize.
/// Uses maximum possible sizes: 73-byte signature + 33-byte compressed pubkey.
fn create_dummy_p2wpkh_witness() -> bitcoin::Witness {
    // Max DER signature is 73 bytes (including sighash byte)
    let dummy_sig = vec![0u8; 73];
    // Compressed public key is always 33 bytes
    let dummy_pubkey = vec![0u8; 33];
    bitcoin::Witness::from_slice(&[dummy_sig, dummy_pubkey])
}

/// Build unsigned transaction and return sighashes for each input.
fn build_unsigned_transaction(
    utxos: &[Utxo],
    to_address: &str,
    amount_sats: u64,
    fee_rate: u64,
    change_address: &str,
    pubkey: &[u8],
) -> Result<(bitcoin::Transaction, Vec<[u8; 32]>, u64), String> {
    use bitcoin::hashes::Hash;
    use bitcoin::sighash::{EcdsaSighashType, SighashCache};
    use bitcoin::transaction::Version;
    use bitcoin::{
        Address, Amount, CompressedPublicKey, OutPoint, ScriptBuf, Sequence, Transaction, TxIn,
        TxOut, Txid, Witness,
    };
    use std::str::FromStr;

    // Parse public key for script generation
    let pk = PublicKey::from_slice(pubkey).map_err(|e| format!("Invalid public key: {}", e))?;
    let compressed_pk = CompressedPublicKey(pk);
    let wpkh = compressed_pk.wpubkey_hash();

    // Sort UTXOs by value (largest first)
    let mut sorted_utxos = utxos.to_vec();
    sorted_utxos.sort_by(|a, b| b.value.cmp(&a.value));

    // Parse addresses upfront
    let dest_address = Address::from_str(to_address)
        .map_err(|e| format!("Invalid destination address: {}", e))?
        .assume_checked();

    let change_addr = Address::from_str(change_address)
        .map_err(|e| format!("Invalid change address: {}", e))?
        .assume_checked();

    // Helper to build a transaction with given UTXOs and calculate its vsize
    let build_tx_with_dummy_witness =
        |selected: &[Utxo], amt: u64, change: u64| -> Result<(Transaction, u64), String> {
            let mut inputs = Vec::new();
            for utxo in selected {
                let txid =
                    Txid::from_str(&utxo.txid).map_err(|e| format!("Invalid txid: {}", e))?;
                inputs.push(TxIn {
                    previous_output: OutPoint {
                        txid,
                        vout: utxo.vout,
                    },
                    script_sig: ScriptBuf::new(),
                    sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                    witness: create_dummy_p2wpkh_witness(),
                });
            }

            let mut outputs = vec![TxOut {
                value: Amount::from_sat(amt),
                script_pubkey: dest_address.script_pubkey(),
            }];

            if change > 546 {
                outputs.push(TxOut {
                    value: Amount::from_sat(change),
                    script_pubkey: change_addr.script_pubkey(),
                });
            }

            let tx = Transaction {
                version: Version::TWO,
                lock_time: bitcoin::absolute::LockTime::ZERO,
                input: inputs,
                output: outputs,
            };

            let vsize = tx.vsize() as u64;
            Ok((tx, vsize))
        };

    // Select UTXOs using rough estimate first
    let mut selected_utxos: Vec<Utxo> = Vec::new();
    let mut total_input: u64 = 0;

    // Initial rough estimate: 70 vbytes per input, 35 per output, 11 base
    let rough_vsize_per_input: u64 = 70;
    let rough_vsize_base: u64 = 11 + 35 * 2; // base + 2 outputs

    for utxo in &sorted_utxos {
        selected_utxos.push(utxo.clone());
        total_input += utxo.value;

        let rough_vsize = rough_vsize_base + (selected_utxos.len() as u64 * rough_vsize_per_input);
        let rough_fee = fee_rate * rough_vsize;

        if total_input >= amount_sats + rough_fee {
            break;
        }
    }

    if selected_utxos.is_empty() {
        return Err("No UTXOs available".to_string());
    }

    // Now calculate actual vsize with dummy witness
    let (_, actual_vsize) = build_tx_with_dummy_witness(&selected_utxos, amount_sats, 1000)?; // dummy change

    let fee_sats = fee_rate * actual_vsize;
    let change_sats = total_input.saturating_sub(amount_sats + fee_sats);

    // Check if we have enough funds
    if total_input < amount_sats + fee_sats {
        return Err(format!(
            "Insufficient funds: have {} sats, need {} sats (amount: {}, fee: {})",
            total_input,
            amount_sats + fee_sats,
            amount_sats,
            fee_sats
        ));
    }

    // Build final transaction (without witness - will be added after signing)
    let mut tx_inputs = Vec::new();
    for utxo in &selected_utxos {
        let txid = Txid::from_str(&utxo.txid).map_err(|e| format!("Invalid txid: {}", e))?;

        tx_inputs.push(TxIn {
            previous_output: OutPoint {
                txid,
                vout: utxo.vout,
            },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: Witness::default(),
        });
    }

    let mut tx_outputs = vec![TxOut {
        value: Amount::from_sat(amount_sats),
        script_pubkey: dest_address.script_pubkey(),
    }];

    if change_sats > 546 {
        tx_outputs.push(TxOut {
            value: Amount::from_sat(change_sats),
            script_pubkey: change_addr.script_pubkey(),
        });
    }

    let tx = Transaction {
        version: Version::TWO,
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: tx_inputs,
        output: tx_outputs,
    };

    // Compute sighashes for each input
    let mut sighash_cache = SighashCache::new(&tx);
    let mut sighashes: Vec<[u8; 32]> = Vec::new();
    let script_code = ScriptBuf::new_p2wpkh(&wpkh);

    for (i, utxo) in selected_utxos.iter().enumerate() {
        let value = Amount::from_sat(utxo.value);

        let sighash = sighash_cache
            .p2wpkh_signature_hash(i, &script_code, value, EcdsaSighashType::All)
            .map_err(|e| format!("Sighash error: {}", e))?;

        sighashes.push(sighash.to_byte_array());
    }

    Ok((tx, sighashes, fee_sats))
}

/// Finalize transaction by adding witness data with signatures.
fn finalize_transaction(
    mut tx: bitcoin::Transaction,
    signatures: &[Vec<u8>],
    pubkey: &[u8],
) -> Result<String, String> {
    use bitcoin::consensus::encode::serialize_hex;
    use bitcoin::sighash::EcdsaSighashType;
    use bitcoin::Witness;

    if signatures.len() != tx.input.len() {
        return Err(format!(
            "Signature count mismatch: {} signatures for {} inputs",
            signatures.len(),
            tx.input.len()
        ));
    }

    let pubkey_vec = pubkey.to_vec();

    for (i, sig_der) in signatures.iter().enumerate() {
        // Append sighash type to signature
        let mut sig_with_type = sig_der.clone();
        sig_with_type.push(EcdsaSighashType::All.to_u32() as u8);

        // Create witness: [signature, pubkey]
        let witness = Witness::from_slice(&[&sig_with_type, &pubkey_vec]);
        tx.input[i].witness = witness;
    }

    Ok(serialize_hex(&tx))
}

/// Get UTXOs for a wallet.
///
/// GET /wallet/:wallet_id/utxos
pub async fn get_utxos(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(wallet_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    info!("Get UTXOs for wallet: {}", wallet_id);

    let wallet = {
        let s = state.read().await;
        s.get_stored_wallet(wallet_id)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or_else(|| (StatusCode::NOT_FOUND, "Wallet not found".to_string()))?
    };

    // Allow both Bitcoin (SegWit) and Taproot wallet types
    if wallet.wallet_type != WalletType::Bitcoin && wallet.wallet_type != WalletType::Taproot {
        return Err((
            StatusCode::BAD_REQUEST,
            "UTXOs only available for Bitcoin/Taproot wallets".to_string(),
        ));
    }

    let backend = BitcoinBackend::from_env()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let utxos = backend
        .get_utxos(&wallet.address)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;

    let total: u64 = utxos.iter().map(|u| u.value).sum();

    Ok(Json(serde_json::json!({
        "address": wallet.address,
        "utxos": utxos,
        "count": utxos.len(),
        "total_sats": total,
        "total_btc": format!("{:.8}", total as f64 / 100_000_000.0)
    })))
}

// ============================================================================
// Taproot (FROST/Schnorr) Send Handler
// ============================================================================

/// Send Bitcoin from a Taproot wallet using FROST threshold Schnorr signatures.
pub async fn send_taproot(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(wallet_id): Path<Uuid>,
    Json(request): Json<SendBitcoinRequest>,
) -> Result<Json<SendBitcoinResponse>, (StatusCode, String)> {
    reject_mainnet()?;
    info!("========================================");
    info!("  TAPROOT SEND REQUEST (FROST/Schnorr)");
    info!("========================================");
    info!("Wallet ID: {}", wallet_id);
    info!("To: {}", request.to_address);
    info!("Amount: {} sats", request.amount_sats);

    // Get wallet and nodes
    let (wallet, nodes) = {
        let s = state.read().await;
        let wallet = s
            .get_stored_wallet(wallet_id)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or_else(|| (StatusCode::NOT_FOUND, "Wallet not found".to_string()))?;
        (wallet, s.nodes.clone())
    };

    if wallet.wallet_type != WalletType::Taproot {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "This endpoint is for Taproot wallets only. Wallet type: {:?}",
                wallet.wallet_type
            ),
        ));
    }

    // Get UTXOs and balance
    let backend = BitcoinBackend::from_env().map_err(|e| {
        error!("Failed to create Bitcoin backend: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    let utxos = backend.get_utxos(&wallet.address).await.map_err(|e| {
        error!("Failed to get UTXOs: {}", e);
        (StatusCode::BAD_GATEWAY, e.to_string())
    })?;

    if utxos.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "No UTXOs available. Fund the Taproot address first.".to_string(),
        ));
    }

    let total_balance: u64 = utxos.iter().map(|u| u.value).sum();
    info!(
        "Available balance: {} sats from {} UTXOs",
        total_balance,
        utxos.len()
    );

    // Get fee estimate
    let fee_estimates = backend.get_fee_estimates().await.unwrap_or_default();
    let fee_rate = request
        .fee_rate
        .unwrap_or_else(|| fee_estimates.recommended().max(1));
    info!("Using fee rate: {} sat/vB", fee_rate);

    // Get the script pubkey for the Taproot address
    let address = bitcoin::Address::from_str(&wallet.address)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Invalid address: {}", e),
            )
        })?
        .assume_checked();
    let script_pubkey = address.script_pubkey();

    let (unsigned_tx, sighashes, fee_sats) = build_unsigned_taproot_tx(
        &utxos,
        &request.to_address,
        request.amount_sats,
        fee_rate,
        &wallet.address,
        script_pubkey.as_bytes(),
    )
    .map_err(|e| {
        error!("Failed to build Taproot transaction: {}", e);
        (StatusCode::BAD_REQUEST, e)
    })?;

    info!(
        "Built unsigned Taproot tx with {} inputs, fee={} sats",
        sighashes.len(),
        fee_sats
    );

    // Sign each input using FROST threshold Schnorr
    let participant_indices: Vec<u16> = (0..THRESHOLD).collect();
    let http_client = reqwest::Client::new();

    let mut signatures: Vec<Vec<u8>> = Vec::new();

    for (input_idx, sighash) in sighashes.iter().enumerate() {
        info!(
            "FROST signing input {} with sighash: {}",
            input_idx,
            hex::encode(sighash)
        );

        let signature = run_frost_signing(
            &http_client,
            &nodes,
            wallet_id,
            sighash,
            &participant_indices,
            &state,
        )
        .await
        .map_err(|e| {
            error!("FROST signing failed for input {}: {}", input_idx, e);
            (StatusCode::INTERNAL_SERVER_ERROR, e)
        })?;

        signatures.push(signature);
    }

    info!("All {} inputs signed with FROST", signatures.len());

    // Finalize the transaction with Schnorr signatures
    let signed_tx = finalize_taproot_tx(&unsigned_tx, &signatures).map_err(|e| {
        error!("Failed to finalize Taproot transaction: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, e)
    })?;

    info!("Finalized Taproot transaction");

    // Broadcast the transaction
    let txid = backend.broadcast_tx(&signed_tx).await.map_err(|e| {
        error!("Broadcast failed: {}", e);
        (StatusCode::BAD_GATEWAY, e.to_string())
    })?;

    info!("========================================");
    info!("  TAPROOT TRANSACTION BROADCAST SUCCESS");
    info!("========================================");
    info!("TXID: {}", txid);

    Ok(Json(SendBitcoinResponse {
        txid: txid.clone(),
        amount_sats: request.amount_sats,
        fee_sats,
        to_address: request.to_address.clone(),
        message: format!(
            "Taproot transaction broadcast successfully! View at: {}",
            backend.tx_url(&txid)
        ),
    }))
}

/// Build an unsigned Taproot transaction with BIP-341 sighashes.
fn build_unsigned_taproot_tx(
    utxos: &[Utxo],
    to_address: &str,
    amount_sats: u64,
    fee_rate: u64,
    change_address: &str,
    script_pubkey: &[u8],
) -> Result<(bitcoin::Transaction, Vec<[u8; 32]>, u64), String> {
    use bitcoin::hashes::Hash;
    use bitcoin::sighash::{Prevouts, SighashCache, TapSighashType};
    use bitcoin::transaction::Version;
    use bitcoin::{
        Address, Amount, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Txid, Witness,
    };
    use std::str::FromStr;

    let script_pubkey_buf = ScriptBuf::from_bytes(script_pubkey.to_vec());

    // Sort UTXOs by value (largest first)
    let mut sorted_utxos = utxos.to_vec();
    sorted_utxos.sort_by(|a, b| b.value.cmp(&a.value));

    // Estimate sizes for Taproot
    let base_size: u64 = 10;
    let input_size: u64 = 58; // P2TR key-path spend: ~57.5 vB
    let output_size: u64 = 43; // P2TR output

    // Select UTXOs
    let mut selected_utxos: Vec<Utxo> = Vec::new();
    let mut total_input: u64 = 0;
    let min_fee = fee_rate * (base_size + input_size + 2 * output_size);
    let mut target = amount_sats + min_fee;

    for utxo in &sorted_utxos {
        if total_input >= target {
            break;
        }
        selected_utxos.push(utxo.clone());
        total_input += utxo.value;

        let estimated_size =
            base_size + (selected_utxos.len() as u64 * input_size) + (2 * output_size);
        let estimated_fee = fee_rate * estimated_size;
        target = amount_sats + estimated_fee;
    }

    if total_input < target {
        return Err(format!(
            "Insufficient funds: have {} sats, need {} sats",
            total_input, target
        ));
    }

    // Calculate fee
    let tx_size = base_size + (selected_utxos.len() as u64 * input_size) + (2 * output_size);
    let fee_sats = fee_rate * tx_size;
    let change_sats = total_input.saturating_sub(amount_sats + fee_sats);

    // Parse addresses
    let dest_address = Address::from_str(to_address)
        .map_err(|e| format!("Invalid destination address: {}", e))?
        .assume_checked();
    let change_addr = Address::from_str(change_address)
        .map_err(|e| format!("Invalid change address: {}", e))?
        .assume_checked();

    // Build inputs and collect prevouts
    let mut inputs = Vec::new();
    let mut prevouts = Vec::new();

    for utxo in &selected_utxos {
        let txid = Txid::from_str(&utxo.txid).map_err(|e| format!("Invalid txid: {}", e))?;

        inputs.push(TxIn {
            previous_output: OutPoint {
                txid,
                vout: utxo.vout,
            },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: Witness::default(),
        });

        prevouts.push(TxOut {
            value: Amount::from_sat(utxo.value),
            script_pubkey: script_pubkey_buf.clone(),
        });
    }

    // Build outputs
    let mut outputs = vec![TxOut {
        value: Amount::from_sat(amount_sats),
        script_pubkey: dest_address.script_pubkey(),
    }];

    if change_sats > 546 {
        outputs.push(TxOut {
            value: Amount::from_sat(change_sats),
            script_pubkey: change_addr.script_pubkey(),
        });
    }

    // Create transaction
    let tx = Transaction {
        version: Version::TWO,
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: inputs,
        output: outputs,
    };

    // Compute Taproot sighashes (BIP-341)
    let mut sighashes = Vec::new();
    let mut sighash_cache = SighashCache::new(&tx);
    let prevouts_ref = Prevouts::All(&prevouts);

    for i in 0..selected_utxos.len() {
        let sighash = sighash_cache
            .taproot_key_spend_signature_hash(i, &prevouts_ref, TapSighashType::Default)
            .map_err(|e| format!("Taproot sighash error: {}", e))?;

        sighashes.push(sighash.to_byte_array());
    }

    Ok((tx, sighashes, fee_sats))
}

/// Finalize a Taproot transaction with Schnorr signatures.
fn finalize_taproot_tx(
    unsigned_tx: &bitcoin::Transaction,
    signatures: &[Vec<u8>],
) -> Result<String, String> {
    use bitcoin::consensus::encode::serialize_hex;
    use bitcoin::Witness;

    let mut tx = unsigned_tx.clone();

    if signatures.len() != tx.input.len() {
        return Err(format!(
            "Expected {} signatures, got {}",
            tx.input.len(),
            signatures.len()
        ));
    }

    for (i, sig) in signatures.iter().enumerate() {
        if sig.len() != 64 {
            return Err(format!(
                "Invalid Schnorr signature length for input {}: expected 64, got {}",
                i,
                sig.len()
            ));
        }

        // For Taproot key-path spend with SIGHASH_DEFAULT, witness is just the 64-byte signature
        let mut witness = Witness::new();
        witness.push(sig);
        tx.input[i].witness = witness;
    }

    Ok(serialize_hex(&tx))
}

/// Run FROST threshold signing across MPC nodes.
async fn run_frost_signing(
    client: &reqwest::Client,
    nodes: &[String],
    wallet_id: Uuid,
    message_hash: &[u8; 32],
    participant_indices: &[u16],
    state: &Arc<RwLock<AppState>>,
) -> Result<Vec<u8>, String> {
    // Create signing grant
    let grant = {
        let s = state.read().await;
        SigningGrant::new(
            wallet_id.to_string(),
            *message_hash,
            THRESHOLD,
            participant_indices.to_vec(),
            s.grant_signing_key(),
        )
    };

    // Use grant-derived session ID for determinism
    let session_id = grant.session_id();

    info!("Starting FROST signing session: {}", session_id);
    info!("Grant ID: {}", grant.grant_id);
    info!("Participants: {:?}", participant_indices);

    // Create relay session
    {
        let mut s = state.write().await;
        // Check session limit
        s.can_create_session()
            .map_err(|e| format!("Cannot create session: {}", e))?;
        let session = crate::relay_handlers::RelaySession::new(
            session_id.clone(),
            "frost_signing".to_string(),
            participant_indices.to_vec(),
        )
        .map_err(|e| format!("Failed to create relay session: {}", e))?;
        s.relay_sessions.insert(session_id.clone(), session);
    }

    // Start signing on participating nodes
    let mut handles = Vec::new();

    for &party_idx in participant_indices {
        let node_url = nodes
            .get(party_idx as usize)
            .ok_or_else(|| format!("Invalid participant index: {}", party_idx))?;

        let url = format!("{}/frost/sign", node_url);
        let client = client.clone();
        let session_id = session_id.clone();
        let wallet_id = wallet_id.to_string();
        let message_hash_hex = hex::encode(message_hash);
        let parties = participant_indices.to_vec();
        let grant_clone = grant.clone();

        let handle = tokio::spawn(async move {
            let response = client
                .post(&url)
                .json(&serde_json::json!({
                    "session_id": session_id,
                    "wallet_id": wallet_id,
                    "message_hash": message_hash_hex,
                    "parties": parties,
                    "grant": grant_clone
                }))
                .timeout(std::time::Duration::from_secs(60))
                .send()
                .await;

            match response {
                Ok(resp) => {
                    if resp.status().is_success() {
                        resp.json::<serde_json::Value>().await.ok()
                    } else {
                        None
                    }
                }
                Err(_) => None,
            }
        });

        handles.push(handle);
    }

    // Wait for results
    let results: Vec<_> = futures::future::join_all(handles).await;

    // Find successful signature
    for result in results {
        if let Ok(Some(json)) = result {
            if json["success"].as_bool().unwrap_or(false) {
                if let Some(sig_hex) = json["signature"].as_str() {
                    let signature = hex::decode(sig_hex)
                        .map_err(|e| format!("Invalid signature hex: {}", e))?;

                    if signature.len() == 64 {
                        info!("FROST signing successful");
                        return Ok(signature);
                    }
                }
            }
        }
    }

    Err("FROST signing failed: no valid signature received".to_string())
}

// ============================================================================
// Regtest Mining Handler
// ============================================================================

/// Request to mine blocks (regtest only).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct MineBlocksRequest {
    /// Number of blocks to mine (default: 1).
    #[serde(default = "default_blocks")]
    pub blocks: u32,
}

fn default_blocks() -> u32 {
    1
}

/// Response from mining blocks.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MineBlocksResponse {
    pub success: bool,
    pub blocks_mined: u32,
    pub block_hashes: Vec<String>,
    pub address: String,
    pub message: String,
}

/// Mine blocks to a wallet address (regtest only).
///
/// POST /wallet/:wallet_id/mine
///
/// This endpoint is only available when running on regtest network.
/// It mines blocks to the wallet's address, which is useful for testing
/// without needing to use faucets.
pub async fn mine_blocks(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(wallet_id): Path<Uuid>,
    Json(request): Json<MineBlocksRequest>,
) -> Result<Json<MineBlocksResponse>, (StatusCode, String)> {
    info!("Mine {} blocks for wallet: {}", request.blocks, wallet_id);

    // Get wallet
    let wallet = {
        let s = state.read().await;
        s.get_stored_wallet(wallet_id)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or_else(|| (StatusCode::NOT_FOUND, "Wallet not found".to_string()))?
    };

    // Verify it's a Bitcoin/Taproot wallet
    if wallet.wallet_type != WalletType::Bitcoin && wallet.wallet_type != WalletType::Taproot {
        return Err((
            StatusCode::BAD_REQUEST,
            "Mining only available for Bitcoin/Taproot wallets".to_string(),
        ));
    }

    // Create backend and verify we're on regtest
    let backend = BitcoinBackend::from_env().map_err(|e| {
        error!("Failed to create Bitcoin backend: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    if !backend.is_regtest() {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Mining is only available on regtest network. Current network: {}",
                backend.network_name()
            ),
        ));
    }

    // Mine blocks to the wallet address
    let block_hashes = backend
        .mine_blocks(request.blocks, &wallet.address)
        .await
        .map_err(|e| {
            error!("Failed to mine blocks: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    let blocks_mined = block_hashes.len() as u32;

    info!(
        "Mined {} blocks to address {}",
        blocks_mined, wallet.address
    );

    // Calculate approximate reward (50 BTC per block on regtest, but needs 100 confirmations)
    let reward_note = if blocks_mined >= 101 {
        format!(
            "Mined {} blocks. First block reward ({} BTC) is now spendable!",
            blocks_mined, 50
        )
    } else {
        format!(
            "Mined {} blocks. Mine {} more blocks for coinbase maturity (100 confirmations required).",
            blocks_mined,
            101 - blocks_mined
        )
    };

    Ok(Json(MineBlocksResponse {
        success: true,
        blocks_mined,
        block_hashes,
        address: wallet.address.clone(),
        message: reward_note,
    }))
}
