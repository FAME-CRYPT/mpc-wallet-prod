//! FROST (Taproot/Schnorr) protocol handlers for the MPC node.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use common::observability::{EventType, LogEvent};
use common::SigningGrant;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::state::NodeState;

// ============================================================================
// FROST Key Generation
// ============================================================================

/// Request to start FROST keygen.
#[derive(Debug, Deserialize)]
pub struct StartFrostKeygenRequest {
    pub session_id: String,
    pub wallet_id: String,
    pub num_parties: u16,
    pub threshold: u16,
}

/// POST /frost/keygen/start
pub async fn start_frost_keygen(
    State(state): State<Arc<RwLock<NodeState>>>,
    Json(request): Json<StartFrostKeygenRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let party_index = {
        let s = state.read().await;
        s.party_index
    };

    info!("========================================");
    info!("  FROST KEYGEN REQUEST (Taproot)");
    info!("========================================");
    info!("Session ID: {}", request.session_id);
    info!("Wallet ID: {}", request.wallet_id);
    info!("Party index: {}", party_index);
    info!("Num parties: {}", request.num_parties);
    info!("Threshold: {}", request.threshold);

    // Clone what we need for the background task
    let session_id = request.session_id.clone();
    let wallet_id = request.wallet_id.clone();
    let num_parties = request.num_parties;
    let threshold = request.threshold;
    let state_clone = state.clone();

    // Spawn background task to run the protocol
    tokio::spawn(async move {
        run_frost_keygen_protocol(
            state_clone,
            session_id,
            wallet_id,
            party_index,
            num_parties,
            threshold,
        )
        .await;
    });

    Ok(Json(serde_json::json!({
        "success": true,
        "party_index": party_index,
        "session_id": request.session_id,
        "wallet_id": request.wallet_id,
        "message": "FROST keygen started (Taproot)"
    })))
}

/// Stored FROST key share.
#[derive(Debug, Serialize, Deserialize)]
pub struct StoredFrostKeyShare {
    pub version: u32,
    pub wallet_id: String,
    pub party_index: u16,
    pub threshold: u16,
    pub num_parties: u16,
    /// Serialized FROST key share
    pub key_share: Vec<u8>,
    /// X-only public key (32 bytes) for Taproot address
    pub public_key: Vec<u8>,
    pub created_at: u64,
}

impl StoredFrostKeyShare {
    pub const CURRENT_VERSION: u32 = 1;
}

/// Background task to run the FROST keygen protocol.
async fn run_frost_keygen_protocol(
    _state: Arc<RwLock<NodeState>>,
    session_id: String,
    wallet_id: String,
    party_index: u16,
    num_parties: u16,
    threshold: u16,
) {
    use protocols::frost::keygen as frost_keygen_runner;
    use protocols::relay::{RelayClient, RelayMessage};

    info!(
        "FROST keygen protocol task started for session {}",
        session_id
    );

    let coordinator_url = "http://mpc-coordinator:3000";

    // Create channels for the protocol
    let (outgoing_tx, outgoing_rx) =
        async_channel::bounded::<frost_keygen_runner::ProtocolMessage>(1000);
    let (incoming_tx, incoming_rx) =
        async_channel::bounded::<frost_keygen_runner::ProtocolMessage>(1000);

    // Spawn task to send outgoing messages to coordinator
    let client_for_outgoing = RelayClient::new(coordinator_url, party_index);
    let session_id_for_outgoing = session_id.clone();
    let outgoing_handle = tokio::spawn(async move {
        while let Ok(msg) = outgoing_rx.recv().await {
            let relay_msg = RelayMessage {
                session_id: session_id_for_outgoing.clone(),
                protocol: "frost_keygen".to_string(),
                sender: msg.sender,
                recipient: msg.recipient,
                round: msg.round,
                payload: msg.payload,
                seq: msg.seq,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            };
            if let Err(e) = client_for_outgoing.submit_message(relay_msg).await {
                error!("Failed to submit FROST message: {}", e);
            }
        }
    });

    // Spawn task to poll incoming messages from coordinator
    let session_id_for_poll = session_id.clone();
    let poll_client = RelayClient::new(coordinator_url, party_index);
    let polling_handle = tokio::spawn(async move {
        let mut messages_received = false;
        loop {
            match poll_client.poll_messages(&session_id_for_poll).await {
                Ok(response) => {
                    if !response.session_active {
                        if !messages_received {
                            warn!("FROST keygen session ended before any messages were processed");
                        }
                        info!("FROST keygen session no longer active, stopping poll");
                        break;
                    }
                    for relay_msg in response.messages {
                        messages_received = true;
                        let protocol_msg = frost_keygen_runner::ProtocolMessage {
                            session_id: relay_msg.session_id,
                            sender: relay_msg.sender,
                            recipient: relay_msg.recipient,
                            round: relay_msg.round,
                            payload: relay_msg.payload,
                            seq: relay_msg.seq,
                        };
                        if let Err(e) = incoming_tx.send(protocol_msg).await {
                            error!("Failed to forward message to protocol: {}", e);
                            break;
                        }
                    }
                }
                Err(e) => {
                    error!("Poll error: {}", e);
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    });

    // Run the actual protocol
    let result = frost_keygen_runner::run_frost_keygen(
        party_index,
        num_parties,
        threshold,
        &session_id,
        incoming_rx,
        outgoing_tx,
    )
    .await;

    // Let the outgoing queue drain before stopping
    info!("FROST keygen completed, waiting for message queue to drain...");
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Stop tasks
    polling_handle.abort();
    outgoing_handle.abort();

    // Handle result
    if result.success {
        if let (Some(key_share_data), Some(public_key)) = (result.key_share_data, result.public_key)
        {
            // Save to disk
            let stored = StoredFrostKeyShare {
                version: StoredFrostKeyShare::CURRENT_VERSION,
                wallet_id: wallet_id.clone(),
                party_index,
                threshold,
                num_parties,
                key_share: key_share_data,
                public_key: public_key.clone(),
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            };

            let filename = format!("/data/frost-keyshare-{}.json", wallet_id);
            match serde_json::to_string_pretty(&stored) {
                Ok(json) => {
                    if let Err(e) = std::fs::write(&filename, json) {
                        error!("Failed to save FROST key share: {}", e);
                    } else {
                        info!("FROST key share saved to {}", filename);
                        info!("Public key (x-only): {}", hex::encode(&public_key));
                    }
                }
                Err(e) => {
                    error!("Failed to serialize FROST key share: {}", e);
                }
            }
        }
    } else {
        error!("FROST keygen failed: {:?}", result.error);
    }
}

/// List all FROST key shares.
///
/// GET /frost/keyshares
pub async fn list_frost_keyshares(
    State(state): State<Arc<RwLock<NodeState>>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let party_index = {
        let s = state.read().await;
        s.party_index
    };

    // Read all FROST key share files
    let mut keyshares = Vec::new();

    if let Ok(entries) = std::fs::read_dir("/data") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if filename.starts_with("frost-keyshare-") && filename.ends_with(".json") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(stored) = serde_json::from_str::<StoredFrostKeyShare>(&content) {
                            // Derive Taproot address from x-only public key
                            let address =
                                common::bitcoin_utils::derive_bitcoin_address_taproot_from_xonly(
                                    &stored.public_key,
                                    bitcoin::Network::Testnet,
                                )
                                .unwrap_or_else(|_| "unknown".to_string());

                            keyshares.push(serde_json::json!({
                                "wallet_id": stored.wallet_id,
                                "party_index": stored.party_index,
                                "threshold": stored.threshold,
                                "num_parties": stored.num_parties,
                                "public_key": hex::encode(&stored.public_key),
                                "address": address,
                                "created_at": stored.created_at,
                                "wallet_type": "taproot"
                            }));
                        }
                    }
                }
            }
        }
    }

    Ok(Json(serde_json::json!({
        "party_index": party_index,
        "keyshares": keyshares,
        "count": keyshares.len()
    })))
}

// ============================================================================
// FROST Signing
// ============================================================================

/// Request for FROST sync signing.
#[derive(Debug, Deserialize)]
pub struct FrostSyncSignRequest {
    pub session_id: String,
    pub wallet_id: String,
    pub message_hash: String,
    pub parties: Vec<u16>,
    /// Signing grant (required for authorization).
    pub grant: Option<SigningGrant>,
}

/// Response for FROST sync signing.
#[derive(Debug, Serialize)]
pub struct FrostSyncSignResponse {
    pub success: bool,
    /// R component (32 bytes hex)
    pub signature_r: Option<String>,
    /// s component (32 bytes hex)
    pub signature_s: Option<String>,
    /// Full 64-byte Schnorr signature (hex)
    pub signature: Option<String>,
    pub error: Option<String>,
    pub duration_ms: u64,
}

/// POST /frost/sign
///
/// Synchronous FROST signing for Taproot transactions.
pub async fn frost_sign_sync(
    State(state): State<Arc<RwLock<NodeState>>>,
    Json(request): Json<FrostSyncSignRequest>,
) -> Result<Json<FrostSyncSignResponse>, (StatusCode, String)> {
    use protocols::frost::signing as frost_signing_runner;
    use protocols::relay::{RelayClient, RelayMessage};

    let start = std::time::Instant::now();

    let party_index = {
        let s = state.read().await;
        s.party_index
    };

    info!("========================================");
    info!("  FROST SYNC SIGNING REQUEST (Taproot)");
    info!("========================================");
    info!("Session ID: {}", request.session_id);
    info!("Wallet ID: {}", request.wallet_id);
    info!("Party index: {}", party_index);
    info!("Parties: {:?}", request.parties);

    // Verify grant - REQUIRED for all signing operations
    // Verify grant and extract canonical participant order
    let grant = match &request.grant {
        Some(g) => {
            info!("Grant ID: {}", g.grant_id);

            // Verify grant signature and validity
            let s = state.read().await;
            if let Err(e) = s.verify_grant(g) {
                error!("Grant verification failed: {}", e);
                LogEvent::new(EventType::GrantRejected)
                    .with_correlation_id(&request.session_id)
                    .with_party(party_index)
                    .with_protocol("frost")
                    .with_context("grant_id", g.grant_id.to_string())
                    .with_error(&e)
                    .emit();
                return Err((
                    StatusCode::FORBIDDEN,
                    format!("Grant verification failed: {}", e),
                ));
            }
            info!("Grant verified successfully");
            LogEvent::new(EventType::GrantValidated)
                .with_correlation_id(&request.session_id)
                .with_party(party_index)
                .with_protocol("frost")
                .with_context("grant_id", g.grant_id.to_string())
                .with_context("wallet_id", &g.wallet_id)
                .emit();

            // Verify grant matches request
            if g.wallet_id != request.wallet_id {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!(
                        "Grant wallet_id mismatch: grant={}, request={}",
                        g.wallet_id, request.wallet_id
                    ),
                ));
            }

            // Verify message hash matches
            let request_hash_bytes: [u8; 32] = hex::decode(&request.message_hash)
                .map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        format!("Invalid message hash hex: {}", e),
                    )
                })?
                .try_into()
                .map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        "message_hash must be 32 bytes".to_string(),
                    )
                })?;

            if g.message_hash != request_hash_bytes {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Grant message_hash does not match request".to_string(),
                ));
            }

            // Verify parties match (order-insensitive comparison)
            // Grant participants are already canonicalized (sorted) at creation time
            let mut request_participants = request.parties.clone();
            request_participants.sort();
            if g.participants != request_participants {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!(
                        "Grant participants mismatch: grant={:?}, request={:?}",
                        g.participants, request.parties
                    ),
                ));
            }

            // Return the grant - we'll use its canonical participant order
            g
        }
        None => {
            error!("No grant provided - signing request rejected");
            return Err((
                StatusCode::FORBIDDEN,
                "Signing grant required but not provided".to_string(),
            ));
        }
    };

    info!("Proceeding with grant-authorized FROST signing");

    // Use the grant's canonical participant order for the protocol
    let parties = &grant.participants;

    // Verify our party is in the signing parties BEFORE creating session
    if !parties.contains(&party_index) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Party {} not in signing parties {:?}", party_index, parties),
        ));
    }

    info!(
        "Party index: {} (in participants: {:?})",
        party_index, parties
    );

    // Load the FROST key share BEFORE creating session
    // This prevents blocking retries if key share is missing
    let filename = format!("/data/frost-keyshare-{}.json", request.wallet_id);
    let key_share: StoredFrostKeyShare = std::fs::read_to_string(&filename)
        .map_err(|e| (StatusCode::NOT_FOUND, format!("Key share not found: {}", e)))
        .and_then(|content| {
            serde_json::from_str(&content).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Parse error: {}", e),
                )
            })
        })?;

    // Create or check session (replay protection + session tracking)
    // Only create session AFTER validating prerequisites (participant, key share)
    let metrics = {
        let s = state.read().await;
        s.metrics.clone()
    };

    {
        let mut s = state.write().await;
        match s.session_manager.create_session(
            request.session_id.clone(),
            grant.grant_id,
            grant.expires_at,
            request.wallet_id.clone(),
            "frost".to_string(),
            grant.participants.len(),
        ) {
            Ok(_) => {
                info!("Created new FROST signing session {}", request.session_id);
                LogEvent::new(EventType::SessionCreated)
                    .with_correlation_id(&request.session_id)
                    .with_party(party_index)
                    .with_protocol("frost")
                    .with_context("wallet_id", &request.wallet_id)
                    .with_context("participants", format!("{:?}", grant.participants))
                    .emit();
                // Increment metrics
                metrics.inc_sessions_started();
                metrics.inc_grants_validated();
            }
            Err(crate::session::SessionError::SessionExists { session_id, state }) => {
                // Session already exists - check if it's completed with a signature
                if let Some(session) = s.session_manager.get_session(&session_id) {
                    if let Some(ref sig) = session.signature {
                        info!("Returning cached signature for session {}", session_id);
                        // Return cached result (idempotency)
                        return Ok(Json(FrostSyncSignResponse {
                            success: true,
                            signature_r: None,
                            signature_s: None,
                            signature: Some(hex::encode(sig)),
                            error: None,
                            duration_ms: 0,
                        }));
                    }
                }
                // Session exists but not completed - could be in progress or failed
                return Err((
                    StatusCode::CONFLICT,
                    format!("Session {} already exists in state {:?}", session_id, state),
                ));
            }
            Err(crate::session::SessionError::GrantReplayed { grant_id }) => {
                return Err((
                    StatusCode::CONFLICT,
                    format!("Grant {} has already been used", grant_id),
                ));
            }
            Err(e) => {
                return Err((StatusCode::TOO_MANY_REQUESTS, e.to_string()));
            }
        }
    }

    // Parse message hash
    let message_hash: [u8; 32] = hex::decode(&request.message_hash)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("Invalid message hash: {}", e),
            )
        })?
        .try_into()
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "Message hash must be 32 bytes".to_string(),
            )
        })?;

    let coordinator_url = "http://mpc-coordinator:3000";

    // Create channels for the protocol
    let (outgoing_tx, outgoing_rx) =
        async_channel::bounded::<frost_signing_runner::ProtocolMessage>(1000);
    let (incoming_tx, incoming_rx) =
        async_channel::bounded::<frost_signing_runner::ProtocolMessage>(1000);

    // Spawn task to send outgoing messages
    // Use actual party_index for relay communication (not sequential signing_index)
    let client_for_outgoing = RelayClient::new(coordinator_url, party_index);
    let session_id_for_outgoing = request.session_id.clone();
    let outgoing_handle = tokio::spawn(async move {
        while let Ok(msg) = outgoing_rx.recv().await {
            let relay_msg = RelayMessage {
                session_id: session_id_for_outgoing.clone(),
                protocol: "frost_signing".to_string(),
                sender: msg.sender,
                recipient: msg.recipient,
                round: msg.round,
                payload: msg.payload,
                seq: msg.seq,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            };
            if let Err(e) = client_for_outgoing.submit_message(relay_msg).await {
                error!("Failed to submit FROST signing message: {}", e);
            }
        }
        debug!("Outgoing handler finished for FROST signing");
    });

    // Spawn task to poll incoming messages
    // Use actual party_index for relay communication
    let session_id_poll = request.session_id.clone();
    let poll_client = RelayClient::new(coordinator_url, party_index);
    let state_for_poll = state.clone();
    let party_index_for_poll = party_index;
    let metrics_for_poll = metrics.clone();
    let polling_handle = tokio::spawn(async move {
        let mut messages_received = false;
        loop {
            match poll_client.poll_messages(&session_id_poll).await {
                Ok(response) => {
                    if !response.session_active {
                        if !messages_received {
                            warn!("FROST signing session ended before any messages were processed");
                        }
                        debug!("FROST signing session no longer active, stopping poll");
                        break;
                    }
                    for relay_msg in response.messages {
                        messages_received = true;

                        // Record message for round tracking
                        {
                            let mut s = state_for_poll.write().await;
                            let _ = s.session_manager.record_message(
                                &session_id_poll,
                                relay_msg.sender,
                                relay_msg.round,
                            );
                        }

                        // Emit message received event
                        // Note: relay_msg.round is always 0 as round info is encoded in message payload
                        LogEvent::new(EventType::MessageReceived)
                            .with_correlation_id(&session_id_poll)
                            .with_party(party_index_for_poll)
                            .with_protocol("frost")
                            .with_context("from_party", relay_msg.sender.to_string())
                            .with_context("bytes", relay_msg.payload.len().to_string())
                            .with_context("seq", relay_msg.seq.to_string())
                            .emit();

                        // Increment message received metric
                        metrics_for_poll.inc_messages_received(relay_msg.payload.len() as u64);

                        let protocol_msg = frost_signing_runner::ProtocolMessage {
                            session_id: relay_msg.session_id,
                            sender: relay_msg.sender,
                            recipient: relay_msg.recipient,
                            round: relay_msg.round,
                            payload: relay_msg.payload,
                            seq: relay_msg.seq,
                        };
                        if let Err(e) = incoming_tx.send(protocol_msg).await {
                            error!("Failed to forward message: {}", e);
                            break;
                        }
                    }
                }
                Err(e) => {
                    debug!("Poll error: {}", e);
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    });

    // Run the signing protocol
    // Use actual party_index (not sequential signing_index) for protocol messages
    // The FROST protocol expects actual party indices that match key share indices
    let result = frost_signing_runner::run_frost_signing(
        party_index,
        parties,
        &request.session_id,
        &message_hash,
        &key_share.key_share,
        incoming_rx,
        outgoing_tx,
    )
    .await;

    // Let the outgoing queue drain
    debug!("FROST signing completed, waiting for message queue to drain...");
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Clean up
    polling_handle.abort();
    outgoing_handle.abort();

    let elapsed = start.elapsed();

    match result.signature {
        Some(signature) => {
            info!("FROST signing completed successfully in {:?}", elapsed);

            let sig_bytes = signature.to_bytes();

            // Mark session as completed
            {
                let mut s = state.write().await;
                if let Err(e) = s
                    .session_manager
                    .complete_session(&request.session_id, sig_bytes.to_vec())
                {
                    error!("Failed to mark session as completed: {}", e);
                }
            }

            // Emit session completed event
            LogEvent::new(EventType::SessionCompleted)
                .with_correlation_id(&request.session_id)
                .with_party(party_index)
                .with_protocol("frost")
                .with_duration(elapsed)
                .with_context("wallet_id", &request.wallet_id)
                .emit();

            // Record metrics
            metrics.inc_sessions_completed();
            metrics.record_session_duration("frost", elapsed);

            Ok(Json(FrostSyncSignResponse {
                success: true,
                signature_r: Some(hex::encode(&signature.r)),
                signature_s: Some(hex::encode(&signature.s)),
                signature: Some(hex::encode(&sig_bytes)),
                error: None,
                duration_ms: elapsed.as_millis() as u64,
            }))
        }
        None => {
            let error_msg = result
                .error
                .clone()
                .unwrap_or_else(|| "Unknown error".to_string());
            error!("FROST signing failed: {}", error_msg);

            // Mark session as failed
            {
                let mut s = state.write().await;
                if let Err(e) = s
                    .session_manager
                    .fail_session(&request.session_id, error_msg.clone())
                {
                    error!("Failed to mark session as failed: {}", e);
                }
            }

            // Emit session failed event
            LogEvent::new(EventType::SessionFailed)
                .with_correlation_id(&request.session_id)
                .with_party(party_index)
                .with_protocol("frost")
                .with_duration(elapsed)
                .with_context("wallet_id", &request.wallet_id)
                .with_error(&error_msg)
                .emit();

            // Record metrics
            metrics.inc_sessions_failed();

            Ok(Json(FrostSyncSignResponse {
                success: false,
                signature_r: None,
                signature_s: None,
                signature: None,
                error: result.error,
                duration_ms: elapsed.as_millis() as u64,
            }))
        }
    }
}
