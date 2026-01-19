//! CGGMP24 protocol handlers for the MPC node.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use common::observability::{EventType, LogEvent};
use common::SigningGrant;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use crate::state::NodeState;

// ============================================================================
// CGGMP24 Keygen
// ============================================================================

/// Request to start CGGMP24 keygen.
#[derive(Debug, Clone, Deserialize)]
pub struct StartCggmp24KeygenRequest {
    pub session_id: String,
    pub wallet_id: String,
    pub num_parties: u16,
    pub threshold: u16,
}

/// Start CGGMP24 key generation.
///
/// POST /cggmp24/keygen/start
pub async fn start_cggmp24_keygen(
    State(state): State<Arc<RwLock<NodeState>>>,
    Json(request): Json<StartCggmp24KeygenRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let (party_index, aux_info_data, has_aux_info) = {
        let s = state.read().await;
        (
            s.party_index,
            s.get_aux_info().map(|a| a.to_vec()),
            s.has_aux_info(),
        )
    };

    info!("========================================");
    info!("  CGGMP24 KEYGEN REQUEST");
    info!("========================================");
    info!("Session ID: {}", request.session_id);
    info!("Wallet ID: {}", request.wallet_id);
    info!("Party index: {}", party_index);
    info!("Num parties: {}", request.num_parties);
    info!("Threshold: {}", request.threshold);

    // Check if we have aux_info
    if !has_aux_info {
        error!("Cannot start keygen: aux_info not available");
        return Err((
            StatusCode::PRECONDITION_FAILED,
            "Aux info not generated. Call POST /aux-info/start first.".to_string(),
        ));
    }

    let aux_info = aux_info_data.unwrap();

    info!("Starting CGGMP24 keygen in background...");

    // Clone what we need for the background task
    let session_id = request.session_id.clone();
    let wallet_id = request.wallet_id.clone();
    let num_parties = request.num_parties;
    let threshold = request.threshold;
    let state_clone = state.clone();

    // Spawn background task to run the protocol
    tokio::spawn(async move {
        run_cggmp24_keygen_protocol(
            state_clone,
            session_id,
            wallet_id,
            party_index,
            num_parties,
            threshold,
            aux_info,
        )
        .await;
    });

    Ok(Json(serde_json::json!({
        "success": true,
        "party_index": party_index,
        "session_id": request.session_id,
        "wallet_id": request.wallet_id,
        "message": "CGGMP24 keygen started"
    })))
}

/// Background task to run the CGGMP24 keygen protocol.
async fn run_cggmp24_keygen_protocol(
    state: Arc<RwLock<NodeState>>,
    session_id: String,
    wallet_id: String,
    party_index: u16,
    num_parties: u16,
    threshold: u16,
    aux_info_data: Vec<u8>,
) {
    use protocols::cggmp24::keygen::{self as keygen_runner, StoredKeyShare};
    use protocols::cggmp24::runner::ProtocolMessage as MpcProtocolMessage;
    use protocols::relay::{RelayClient, RelayMessage};

    info!(
        "CGGMP24 keygen protocol task started for session {}",
        session_id
    );

    let coordinator_url = "http://mpc-coordinator:3000";
    let client = RelayClient::new(coordinator_url, party_index);

    // Create channels for the protocol
    let (outgoing_tx, outgoing_rx) = async_channel::bounded::<MpcProtocolMessage>(1000);
    let (incoming_tx, incoming_rx) = async_channel::bounded::<MpcProtocolMessage>(1000);

    // Spawn task to send outgoing messages to coordinator
    let outgoing_handle = tokio::spawn(async move {
        while let Ok(msg) = outgoing_rx.recv().await {
            let relay_msg = RelayMessage {
                session_id: msg.session_id.clone(),
                protocol: "keygen".to_string(),
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

            if let Err(e) = client.submit_message(relay_msg).await {
                error!("Failed to submit keygen message: {}", e);
            }
        }
    });

    // Spawn task to poll incoming messages from coordinator
    let session_id_for_poll = session_id.clone();
    let poll_client = RelayClient::new(coordinator_url, party_index);

    let polling_handle = tokio::spawn(async move {
        loop {
            match poll_client.poll_messages(&session_id_for_poll).await {
                Ok(response) => {
                    if !response.session_active {
                        info!("Keygen session no longer active, stopping poll");
                        break;
                    }

                    for msg in response.messages {
                        let protocol_msg = MpcProtocolMessage {
                            session_id: msg.session_id,
                            sender: msg.sender,
                            recipient: msg.recipient,
                            round: msg.round,
                            payload: msg.payload,
                            seq: msg.seq,
                        };

                        if let Err(e) = incoming_tx.send(protocol_msg).await {
                            error!("Failed to forward keygen message: {}", e);
                            break;
                        }
                    }
                }
                Err(e) => {
                    debug!("Keygen poll error (may be expected during startup): {}", e);
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    });

    // Run the actual protocol
    let result = keygen_runner::run_keygen(
        party_index,
        num_parties,
        threshold,
        &session_id,
        incoming_rx,
        outgoing_tx,
    )
    .await;

    // Let the outgoing queue drain before stopping
    info!("Keygen protocol completed, waiting for message queue to drain...");
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Stop tasks
    polling_handle.abort();
    outgoing_handle.abort();

    // Handle result
    if result.success {
        if let (Some(key_share_data), Some(public_key)) = (result.key_share_data, result.public_key)
        {
            // Save to disk
            let stored = StoredKeyShare {
                version: StoredKeyShare::CURRENT_VERSION,
                wallet_id: wallet_id.clone(),
                party_index,
                threshold,
                num_parties,
                incomplete_key_share: key_share_data.clone(),
                aux_info: aux_info_data,
                public_key: public_key.clone(),
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            };

            let path = keygen_runner::default_key_share_path(party_index, &wallet_id);
            if let Err(e) = keygen_runner::save_key_share(&path, &stored) {
                error!("Failed to save CGGMP24 key share: {}", e);
            } else {
                info!("CGGMP24 key share saved successfully");
                info!("Public key: {}", hex::encode(&public_key));
            }

            // Update state
            {
                let mut s = state.write().await;
                s.cggmp24_key_shares.insert(wallet_id.clone(), stored);
            }
        }
    } else {
        error!("CGGMP24 keygen failed: {:?}", result.error);
    }
}

/// List CGGMP24 key shares stored on this node.
///
/// GET /cggmp24/keyshares
pub async fn list_cggmp24_keyshares(
    State(state): State<Arc<RwLock<NodeState>>>,
) -> Json<serde_json::Value> {
    let s = state.read().await;

    let keyshares: Vec<serde_json::Value> = s
        .cggmp24_key_shares
        .iter()
        .map(|(wallet_id, ks)| {
            serde_json::json!({
                "wallet_id": wallet_id,
                "party_index": ks.party_index,
                "threshold": ks.threshold,
                "num_parties": ks.num_parties,
                "public_key": hex::encode(&ks.public_key),
                "created_at": ks.created_at,
            })
        })
        .collect();

    Json(serde_json::json!({
        "party_index": s.party_index,
        "keyshares": keyshares,
        "count": keyshares.len()
    }))
}

// ============================================================================
// CGGMP24 Signing
// ============================================================================

/// Request for synchronous CGGMP24 signing.
#[derive(Debug, Clone, Deserialize)]
pub struct SyncSignRequest {
    pub session_id: String,
    pub wallet_id: String,
    pub message_hash: String, // hex-encoded 32-byte hash
    pub parties: Vec<u16>,    // participating party indices
    /// Signing grant (required for authorization).
    pub grant: Option<SigningGrant>,
}

/// Response from synchronous signing.
#[derive(Debug, Clone, Serialize)]
pub struct SyncSignResponse {
    pub success: bool,
    pub signature_r: Option<String>,
    pub signature_s: Option<String>,
    pub signature_der: Option<String>,
    pub error: Option<String>,
    pub duration_ms: u64,
}

/// Synchronous CGGMP24 signing - waits for completion and returns signature.
///
/// POST /cggmp24/sign
pub async fn cggmp24_sign_sync(
    State(state): State<Arc<RwLock<NodeState>>>,
    Json(request): Json<SyncSignRequest>,
) -> Result<Json<SyncSignResponse>, (StatusCode, String)> {
    use protocols::cggmp24::runner::ProtocolMessage;
    use protocols::relay::{RelayClient, RelayMessage};

    let start = std::time::Instant::now();

    let (party_index, key_share) = {
        let s = state.read().await;
        let ks = s.cggmp24_key_shares.get(&request.wallet_id).cloned();
        (s.party_index, ks)
    };

    info!("========================================");
    info!("  CGGMP24 SYNC SIGNING REQUEST");
    info!("========================================");
    info!("Session ID: {}", request.session_id);
    info!("Wallet ID: {}", request.wallet_id);
    info!("Party index: {}", party_index);
    info!("Message hash: {}", request.message_hash);
    info!("Signing parties: {:?}", request.parties);

    // Verify grant and extract canonical participant order
    let grant = match &request.grant {
        Some(g) => {
            info!("Grant ID: {}", g.grant_id);

            // Verify grant
            let s = state.read().await;
            if let Err(e) = s.verify_grant(g) {
                error!("Grant verification failed: {}", e);
                LogEvent::new(EventType::GrantRejected)
                    .with_correlation_id(&request.session_id)
                    .with_party(party_index)
                    .with_protocol("cggmp24")
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
                .with_protocol("cggmp24")
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

            // Verify message hash matches (grant has bytes, request has hex)
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
            // Grant is REQUIRED - no backward compatibility bypass
            error!("No grant provided - signing request rejected");
            return Err((
                StatusCode::FORBIDDEN,
                "Signing grant required but not provided".to_string(),
            ));
        }
    };

    info!("Proceeding with grant-authorized signing");

    // Use the grant's canonical participant order for the protocol
    let parties = &grant.participants;

    // Check if we're a participant BEFORE creating session
    if !parties.contains(&party_index) {
        info!("This node is not a participant in signing");
        return Ok(Json(SyncSignResponse {
            success: false,
            signature_r: None,
            signature_s: None,
            signature_der: None,
            error: Some("This node is not a participant".to_string()),
            duration_ms: start.elapsed().as_millis() as u64,
        }));
    }

    // Check if we have the key share BEFORE creating session
    // This prevents blocking retries if key share is missing
    let key_share = match key_share {
        Some(ks) => ks,
        None => {
            error!("Key share not found for wallet {}", request.wallet_id);
            return Err((
                StatusCode::NOT_FOUND,
                format!("Key share not found for wallet {}", request.wallet_id),
            ));
        }
    };

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
            "cggmp24".to_string(),
            grant.participants.len(),
        ) {
            Ok(_) => {
                info!("Created new signing session {}", request.session_id);
                LogEvent::new(EventType::SessionCreated)
                    .with_correlation_id(&request.session_id)
                    .with_party(party_index)
                    .with_protocol("cggmp24")
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
                        return Ok(Json(SyncSignResponse {
                            success: true,
                            signature_r: None, // Would need to parse from cached sig
                            signature_s: None,
                            signature_der: Some(hex::encode(sig)),
                            error: None,
                            duration_ms: start.elapsed().as_millis() as u64,
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
    let message_hash_bytes = hex::decode(&request.message_hash).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid message hash: {}", e),
        )
    })?;

    if message_hash_bytes.len() != 32 {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Message hash must be 32 bytes, got {}",
                message_hash_bytes.len()
            ),
        ));
    }

    let mut message_hash = [0u8; 32];
    message_hash.copy_from_slice(&message_hash_bytes);

    // Run the signing protocol
    let coordinator_url = "http://mpc-coordinator:3000";

    // Create channels for the protocol
    let (outgoing_tx, outgoing_rx) = async_channel::bounded::<ProtocolMessage>(1000);
    let (incoming_tx, incoming_rx) = async_channel::bounded::<ProtocolMessage>(1000);

    // Spawn task to send outgoing messages to coordinator
    let session_id_out = request.session_id.clone();
    let outgoing_client = RelayClient::new(coordinator_url, party_index);
    let outgoing_handle = tokio::spawn(async move {
        while let Ok(msg) = outgoing_rx.recv().await {
            let relay_msg = RelayMessage {
                session_id: msg.session_id.clone(),
                protocol: "signing".to_string(),
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

            if let Err(e) = outgoing_client.submit_message(relay_msg).await {
                error!("Failed to submit signing message: {}", e);
            }
        }
        debug!(
            "Outgoing message handler finished for session {}",
            session_id_out
        );
    });

    // Spawn task to poll incoming messages from coordinator
    let session_id_poll = request.session_id.clone();
    let poll_client = RelayClient::new(coordinator_url, party_index);
    let state_for_poll = state.clone();
    let party_index_for_poll = party_index;
    let metrics_for_poll = metrics.clone();
    let polling_handle = tokio::spawn(async move {
        loop {
            match poll_client.poll_messages(&session_id_poll).await {
                Ok(response) => {
                    if !response.session_active {
                        debug!("Signing session no longer active, stopping poll");
                        break;
                    }

                    for msg in response.messages {
                        // Record message for round tracking
                        {
                            let mut s = state_for_poll.write().await;
                            let _ = s.session_manager.record_message(
                                &session_id_poll,
                                msg.sender,
                                msg.round,
                            );
                        }

                        // Emit message received event
                        // Note: msg.round is always 0 as round info is encoded in message payload
                        LogEvent::new(EventType::MessageReceived)
                            .with_correlation_id(&session_id_poll)
                            .with_party(party_index_for_poll)
                            .with_protocol("cggmp24")
                            .with_context("from_party", msg.sender.to_string())
                            .with_context("bytes", msg.payload.len().to_string())
                            .with_context("seq", msg.seq.to_string())
                            .emit();

                        // Increment message received metric
                        metrics_for_poll.inc_messages_received(msg.payload.len() as u64);

                        let protocol_msg = ProtocolMessage {
                            session_id: msg.session_id,
                            sender: msg.sender,
                            recipient: msg.recipient,
                            round: msg.round,
                            payload: msg.payload,
                            seq: msg.seq,
                        };

                        if incoming_tx.send(protocol_msg).await.is_err() {
                            debug!("Incoming channel closed");
                            break;
                        }
                    }
                }
                Err(e) => {
                    debug!("Signing poll error: {}", e);
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }
        debug!("Polling handler finished for session {}", session_id_poll);
    });

    // Run the actual signing protocol
    // Use aux_info from the key_share (it was stored during keygen)
    // Use canonical participant order from grant
    let result = protocols::cggmp24::signing::run_signing(
        party_index,
        parties,
        &request.session_id,
        &message_hash,
        &key_share.incomplete_key_share,
        &key_share.aux_info,
        incoming_rx,
        outgoing_tx,
    )
    .await;

    // Let the outgoing queue drain before stopping
    debug!("Signing protocol completed, waiting for message queue to drain...");
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Clean up
    polling_handle.abort();
    outgoing_handle.abort();

    let elapsed = start.elapsed();

    match result.signature {
        Some(signature) => {
            info!("Signing completed successfully in {:?}", elapsed);

            let der = signature.to_der();
            let der_bytes = der.clone();

            // Mark session as completed
            {
                let mut s = state.write().await;
                if let Err(e) = s
                    .session_manager
                    .complete_session(&request.session_id, der_bytes.to_vec())
                {
                    error!("Failed to mark session as completed: {}", e);
                }
            }

            // Emit session completed event
            LogEvent::new(EventType::SessionCompleted)
                .with_correlation_id(&request.session_id)
                .with_party(party_index)
                .with_protocol("cggmp24")
                .with_duration(elapsed)
                .with_context("wallet_id", &request.wallet_id)
                .emit();

            // Record metrics
            metrics.inc_sessions_completed();
            metrics.record_session_duration("cggmp24", elapsed);

            Ok(Json(SyncSignResponse {
                success: true,
                signature_r: Some(hex::encode(&signature.r)),
                signature_s: Some(hex::encode(&signature.s)),
                signature_der: Some(hex::encode(&der)),
                error: None,
                duration_ms: elapsed.as_millis() as u64,
            }))
        }
        None => {
            let error_msg = result
                .error
                .clone()
                .unwrap_or_else(|| "Unknown error".to_string());
            error!("Signing failed: {}", error_msg);

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
                .with_protocol("cggmp24")
                .with_duration(elapsed)
                .with_context("wallet_id", &request.wallet_id)
                .with_error(&error_msg)
                .emit();

            // Record metrics
            metrics.inc_sessions_failed();

            Ok(Json(SyncSignResponse {
                success: false,
                signature_r: None,
                signature_s: None,
                signature_der: None,
                error: result.error,
                duration_ms: elapsed.as_millis() as u64,
            }))
        }
    }
}
