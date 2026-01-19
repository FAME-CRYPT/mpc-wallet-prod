//! P2P signing handlers for direct peer-to-peer session coordination.
//!
//! This module provides endpoints for P2P-initiated signing sessions,
//! where nodes coordinate directly without the coordinator relaying messages.
//!
//! ## Architecture (Phase 4)
//!
//! ```text
//! CLI ──[grant]──> Node (initiator)
//!                    │
//!                    │ SessionProposal (P2P)
//!                    ▼
//!            ┌───────────────┐
//!            │ Other nodes   │
//!            │ (via P2P)     │
//!            └───────────────┘
//!                    │
//!                    │ Protocol messages (P2P)
//!                    ▼
//!                Signature
//! ```
//!
//! ## Flow
//!
//! 1. CLI issues a grant from the coordinator
//! 2. CLI sends grant to any participating node via POST /p2p/sign
//! 3. Node validates grant and checks if it should be initiator
//! 4. If initiator: broadcasts SessionProposal, collects acks, runs protocol
//! 5. If not initiator: redirects to correct node or waits for proposal
//! 6. Signature returned to CLI

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use common::grant::SigningGrant;
use common::selection::select_initiator_from_grant;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::state::NodeState;

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request to initiate P2P signing.
#[derive(Debug, Clone, Deserialize)]
pub struct P2pSignRequest {
    /// The signed grant from the coordinator.
    pub grant: SigningGrant,
    /// Wallet ID (must match grant).
    pub wallet_id: String,
    /// Message hash to sign (hex-encoded, must match grant).
    pub message_hash: String,
}

/// Response from P2P signing request.
#[derive(Debug, Clone, Serialize)]
pub struct P2pSignResponse {
    /// Session ID derived from grant.
    pub session_id: String,
    /// Whether signing completed successfully.
    pub success: bool,
    /// Signature if complete (hex-encoded DER).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// Current session status.
    pub status: P2pSignStatus,
    /// Whether this node is the session initiator.
    pub is_initiator: bool,
    /// Error message if failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

/// Status of P2P signing session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum P2pSignStatus {
    /// Grant validated, checking if we're initiator.
    Validating,
    /// We're the initiator, sending proposals.
    Proposing,
    /// Waiting for acknowledgments.
    WaitingForAcks,
    /// Session started, protocol running.
    InProgress,
    /// Signing completed successfully.
    Completed,
    /// Signing failed.
    Failed,
    /// Redirecting to correct initiator.
    Redirect,
}

/// Response when redirecting to correct initiator.
#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct P2pRedirectResponse {
    /// Session ID.
    pub session_id: String,
    /// The correct initiator party index.
    pub initiator_party: u16,
    /// Message to display.
    pub message: String,
}

// ============================================================================
// Handlers
// ============================================================================

/// Initiate P2P signing session.
///
/// POST /p2p/sign
///
/// This endpoint receives a grant from the CLI and initiates a P2P
/// signing session. The node validates the grant, determines if it
/// should be the session initiator, and either starts the session
/// or redirects to the correct initiator.
///
/// ## Request
///
/// ```json
/// {
///     "grant": { ... },
///     "wallet_id": "abc123",
///     "message_hash": "deadbeef..."
/// }
/// ```
///
/// ## Response (Initiator)
///
/// ```json
/// {
///     "session_id": "...",
///     "success": true,
///     "signature": "3045...",
///     "status": "completed",
///     "is_initiator": true,
///     "duration_ms": 1234
/// }
/// ```
///
/// ## Response (Not Initiator)
///
/// ```json
/// {
///     "session_id": "...",
///     "success": false,
///     "status": "redirect",
///     "is_initiator": false,
///     "error": "Not the designated initiator. Send request to party 2."
/// }
/// ```
pub async fn p2p_sign(
    State(state): State<Arc<RwLock<NodeState>>>,
    Json(request): Json<P2pSignRequest>,
) -> Result<Json<P2pSignResponse>, (StatusCode, String)> {
    let start = std::time::Instant::now();

    let party_index = {
        let s = state.read().await;
        s.party_index
    };

    let session_id = request.grant.session_id();

    info!("========================================");
    info!("  P2P SIGNING REQUEST");
    info!("========================================");
    info!("Session ID: {}", session_id);
    info!("Grant ID: {}", request.grant.grant_id);
    info!("Wallet ID: {}", request.wallet_id);
    info!("Message hash: {}", request.message_hash);
    info!("Party index: {}", party_index);

    // Validate grant
    {
        let s = state.read().await;
        if let Err(e) = s.verify_grant(&request.grant) {
            error!("Grant verification failed: {}", e);
            return Err((
                StatusCode::FORBIDDEN,
                format!("Grant verification failed: {}", e),
            ));
        }
    }

    // Validate wallet_id matches grant
    if request.grant.wallet_id != request.wallet_id {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Wallet ID mismatch: grant={}, request={}",
                request.grant.wallet_id, request.wallet_id
            ),
        ));
    }

    // Validate message hash matches grant
    let message_hash_bytes: [u8; 32] = hex::decode(&request.message_hash)
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

    if request.grant.message_hash != message_hash_bytes {
        return Err((
            StatusCode::BAD_REQUEST,
            "Message hash does not match grant".to_string(),
        ));
    }

    // Check if we're a participant
    if !request.grant.participants.contains(&party_index) {
        error!("This node is not a participant in the grant");
        return Err((
            StatusCode::FORBIDDEN,
            "This node is not a participant in the grant".to_string(),
        ));
    }

    info!("Grant validated successfully");

    // Determine if we should be the initiator
    let expected_initiator = select_initiator_from_grant(&request.grant);
    let is_initiator = expected_initiator == party_index;

    info!(
        "Initiator selection: expected={}, this_node={}, is_initiator={}",
        expected_initiator, party_index, is_initiator
    );

    if !is_initiator {
        // Not the initiator - return redirect response
        info!(
            "Not the designated initiator. Redirecting to party {}",
            expected_initiator
        );

        return Ok(Json(P2pSignResponse {
            session_id,
            success: false,
            signature: None,
            status: P2pSignStatus::Redirect,
            is_initiator: false,
            error: Some(format!(
                "Not the designated initiator. Send request to party {}.",
                expected_initiator
            )),
            duration_ms: start.elapsed().as_millis() as u64,
        }));
    }

    // We are the initiator - start P2P session coordination
    info!("This node is the designated initiator. Starting P2P session coordination...");

    // Check if P2P is ready
    let p2p_components = {
        let s = state.read().await;
        s.p2p_components()
    };

    // Check if HTTP fallback is enabled
    let fallback_enabled = crate::p2p_init::is_p2p_fallback_enabled();

    match p2p_components {
        Some((coordinator, transport)) => {
            // Full P2P mode - use session coordinator
            info!("P2P mode active, using direct peer-to-peer communication");

            match execute_p2p_signing(
                state.clone(),
                coordinator,
                transport,
                request.grant.clone(),
                &session_id,
            )
            .await
            {
                Ok(signature) => {
                    info!("P2P signing completed successfully");
                    Ok(Json(P2pSignResponse {
                        session_id,
                        success: true,
                        signature: Some(hex::encode(&signature)),
                        status: P2pSignStatus::Completed,
                        is_initiator: true,
                        error: None,
                        duration_ms: start.elapsed().as_millis() as u64,
                    }))
                }
                Err(e) if fallback_enabled => {
                    // P2P failed but fallback enabled - try HTTP relay
                    error!("P2P signing failed: {}. Attempting HTTP fallback...", e);

                    let sync_request = super::cggmp24::SyncSignRequest {
                        session_id: session_id.clone(),
                        wallet_id: request.wallet_id.clone(),
                        message_hash: request.message_hash.clone(),
                        parties: request.grant.participants.clone(),
                        grant: Some(request.grant.clone()),
                    };

                    match super::cggmp24::cggmp24_sign_sync(State(state), Json(sync_request)).await
                    {
                        Ok(Json(result)) => {
                            info!(
                                "HTTP fallback succeeded after P2P failure (original error: {})",
                                e
                            );
                            Ok(Json(P2pSignResponse {
                                session_id,
                                success: result.success,
                                signature: result.signature_der,
                                status: if result.success {
                                    P2pSignStatus::Completed
                                } else {
                                    P2pSignStatus::Failed
                                },
                                is_initiator: true,
                                error: result.error,
                                duration_ms: start.elapsed().as_millis() as u64,
                            }))
                        }
                        Err((status, msg)) => {
                            error!("HTTP fallback also failed: {}", msg);
                            Err((
                                status,
                                format!("P2P failed: {}. HTTP fallback failed: {}", e, msg),
                            ))
                        }
                    }
                }
                Err(e) => {
                    // P2P failed and fallback is disabled
                    error!("P2P signing failed (fallback disabled): {}", e);
                    Ok(Json(P2pSignResponse {
                        session_id,
                        success: false,
                        signature: None,
                        status: P2pSignStatus::Failed,
                        is_initiator: true,
                        error: Some(e),
                        duration_ms: start.elapsed().as_millis() as u64,
                    }))
                }
            }
        }
        None => {
            // P2P not ready - fall back to HTTP relay (hybrid mode)
            info!("P2P not ready - falling back to HTTP relay");

            let sync_request = super::cggmp24::SyncSignRequest {
                session_id: session_id.clone(),
                wallet_id: request.wallet_id.clone(),
                message_hash: request.message_hash.clone(),
                parties: request.grant.participants.clone(),
                grant: Some(request.grant.clone()),
            };

            match super::cggmp24::cggmp24_sign_sync(State(state), Json(sync_request)).await {
                Ok(Json(result)) => Ok(Json(P2pSignResponse {
                    session_id,
                    success: result.success,
                    signature: result.signature_der,
                    status: if result.success {
                        P2pSignStatus::Completed
                    } else {
                        P2pSignStatus::Failed
                    },
                    is_initiator: true,
                    error: result.error,
                    duration_ms: start.elapsed().as_millis() as u64,
                })),
                Err((status, msg)) => Err((status, msg)),
            }
        }
    }
}

/// Execute P2P signing as the initiator.
///
/// This function:
/// 1. Prepares and broadcasts a SessionProposal to all participants
/// 2. Waits for threshold SessionAcks
/// 3. Broadcasts SessionStart when threshold reached
/// 4. Executes the signing protocol over P2P transport
/// 5. Returns the signature
async fn execute_p2p_signing(
    state: Arc<RwLock<NodeState>>,
    coordinator: Arc<protocols::p2p::P2pSessionCoordinator>,
    transport: Arc<protocols::p2p::QuicTransport>,
    grant: SigningGrant,
    session_id: &str,
) -> Result<Vec<u8>, String> {
    use protocols::p2p::{SessionControlMessage, SessionStart};
    use std::time::Duration;

    // Refresh peer registry before starting - ensures we have current addresses
    info!("Step 0: Refreshing peer registry...");
    transport
        .refresh_peers()
        .await
        .map_err(|e| format!("Failed to refresh peer registry: {}", e))?;

    info!("Step 1: Preparing session proposal...");

    // Prepare the proposal
    let (proposal, threshold_rx) = coordinator
        .prepare_proposal(grant.clone(), "cggmp24")
        .await
        .map_err(|e| format!("Failed to prepare proposal: {}", e))?;

    info!(
        "Proposal prepared: session_id={}, participants={:?}",
        session_id, grant.participants
    );

    // Broadcast SessionProposal to all participants
    info!(
        "Step 2: Broadcasting proposal to {} participants...",
        grant.participants.len() - 1
    );

    let ctrl_msg = SessionControlMessage::Proposal(proposal);
    broadcast_control_message(
        &transport,
        &ctrl_msg,
        &grant.participants,
        coordinator.party_index(),
    )
    .await?;

    // Wait for threshold acks (with configurable timeout)
    let timeout_secs = crate::p2p_init::get_proposal_timeout_secs();
    info!(
        "Step 3: Waiting for acknowledgments (timeout: {}s)...",
        timeout_secs
    );

    let participants = tokio::time::timeout(Duration::from_secs(timeout_secs), threshold_rx)
        .await
        .map_err(|_| {
            format!(
                "Timeout waiting for participant acknowledgments ({}s)",
                timeout_secs
            )
        })?
        .map_err(|_| "Threshold notification channel closed unexpectedly".to_string())?;

    info!(
        "Threshold reached with {} participants: {:?}",
        participants.len(),
        participants
    );

    // Broadcast SessionStart
    info!("Step 4: Broadcasting session start...");

    let start_msg = SessionStart::new(session_id.to_string(), participants.clone());
    let ctrl_msg = SessionControlMessage::Start(start_msg);
    broadcast_control_message(
        &transport,
        &ctrl_msg,
        &participants,
        coordinator.party_index(),
    )
    .await?;

    // Execute the signing protocol
    info!("Step 5: Executing signing protocol over P2P...");

    let signature = execute_signing_protocol(
        state,
        &coordinator,
        &transport,
        &grant,
        &participants,
        session_id,
    )
    .await?;

    // Mark session as complete
    let _ = coordinator.complete_session(session_id).await;

    info!("P2P signing completed successfully");

    Ok(signature)
}

/// Broadcast a control message to all participants.
async fn broadcast_control_message(
    transport: &Arc<protocols::p2p::QuicTransport>,
    ctrl_msg: &protocols::p2p::SessionControlMessage,
    participants: &[u16],
    our_party: u16,
) -> Result<(), String> {
    use protocols::p2p::P2pMessage;
    use protocols::Transport;

    let payload = bincode::serialize(ctrl_msg)
        .map_err(|e| format!("Failed to serialize control message: {}", e))?;

    for &party in participants {
        if party == our_party {
            continue; // Don't send to ourselves
        }

        let msg = P2pMessage {
            session_id: "__control__".to_string(),
            sender: our_party,
            recipient: Some(party),
            round: 0,
            payload: payload.clone(),
            seq: 0,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        };

        let relay_msg = protocols::relay::RelayMessage::from(msg);

        transport
            .send_message(relay_msg)
            .await
            .map_err(|e| format!("Failed to send to party {}: {}", party, e))?;
    }

    Ok(())
}

/// Execute the actual signing protocol over P2P transport.
#[allow(unused_variables)]
async fn execute_signing_protocol(
    state: Arc<RwLock<NodeState>>,
    _coordinator: &Arc<protocols::p2p::P2pSessionCoordinator>,
    transport: &Arc<protocols::p2p::QuicTransport>,
    grant: &SigningGrant,
    participants: &[u16],
    session_id: &str,
) -> Result<Vec<u8>, String> {
    use protocols::cggmp24::ProtocolMessage;
    use protocols::Transport;

    // Get the key share and aux_info
    let (party_index, key_share_bytes, aux_info) = {
        let s = state.read().await;

        let party_index = s.party_index;

        let key_share = s
            .cggmp24_key_shares
            .get(&grant.wallet_id)
            .ok_or_else(|| format!("No key share for wallet {}", grant.wallet_id))?;

        // Use the raw incomplete_key_share bytes (not the whole StoredKeyShare struct)
        // This matches what the HTTP handler does in cggmp24.rs:639
        let key_share_bytes = key_share.incomplete_key_share.clone();

        // Use aux_info from the key share (stored during keygen), not global aux_info
        let aux_info = key_share.aux_info.clone();

        (party_index, key_share_bytes, aux_info)
    };

    info!(
        "Executing signing: party={}, participants={:?}",
        party_index, participants
    );

    // Start tracking the session
    transport.start_session(session_id).await;

    // Create channels for protocol communication (using ProtocolMessage)
    let (outgoing_tx, outgoing_rx) = async_channel::bounded::<ProtocolMessage>(1000);
    let (incoming_tx, incoming_rx) = async_channel::bounded::<ProtocolMessage>(1000);

    // Spawn task to send outgoing messages (convert ProtocolMessage to RelayMessage)
    let transport_send = transport.clone();
    let session_id_send = session_id.to_string();
    let send_handle = tokio::spawn(async move {
        while let Ok(msg) = outgoing_rx.recv().await {
            // Convert ProtocolMessage to RelayMessage
            let relay_msg = protocols::relay::RelayMessage {
                session_id: session_id_send.clone(),
                protocol: "cggmp24".to_string(),
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
            if let Err(e) = transport_send.send_message(relay_msg).await {
                error!("Failed to send protocol message: {}", e);
                break;
            }
        }
    });

    // Spawn task to receive incoming messages (convert RelayMessage to ProtocolMessage)
    let transport_recv = transport.clone();
    let session_id_poll = session_id.to_string();
    let recv_handle = tokio::spawn(async move {
        loop {
            match transport_recv.poll_messages(&session_id_poll).await {
                Ok(response) => {
                    for relay_msg in response.messages {
                        // Convert RelayMessage to ProtocolMessage
                        let proto_msg = ProtocolMessage {
                            session_id: relay_msg.session_id,
                            sender: relay_msg.sender,
                            recipient: relay_msg.recipient,
                            round: relay_msg.round,
                            payload: relay_msg.payload,
                            seq: relay_msg.seq,
                        };
                        if incoming_tx.send(proto_msg).await.is_err() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    tracing::debug!("Poll error (may be normal): {}", e);
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    });

    // Run the signing protocol
    let signature_result = protocols::cggmp24::signing::run_signing(
        party_index,
        participants,
        session_id,
        &grant.message_hash,
        &key_share_bytes,
        &aux_info,
        incoming_rx,
        outgoing_tx,
    )
    .await;

    // Cleanup
    send_handle.abort();
    recv_handle.abort();
    transport.end_session(session_id).await;

    // Handle SigningResult (not Result<_, _>)
    if signature_result.success {
        if let Some(sig_data) = signature_result.signature {
            // Convert to DER format (to_der returns Vec<u8>, not Result)
            let der = sig_data.to_der();
            info!("Signing completed: signature_len={}", der.len());
            Ok(der)
        } else {
            Err("Signing succeeded but no signature data returned".to_string())
        }
    } else {
        let err_msg = signature_result
            .error
            .unwrap_or_else(|| "Unknown error".to_string());
        error!("Signing failed: {}", err_msg);
        Err(format!("Signing protocol failed: {}", err_msg))
    }
}

/// Get P2P signing session status.
///
/// GET /p2p/session/:session_id
///
/// Returns the current status of a P2P signing session.
pub async fn get_p2p_session_status(
    State(state): State<Arc<RwLock<NodeState>>>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let s = state.read().await;

    // Check if session exists in our session manager
    if let Some(session) = s.session_manager.get_session(&session_id) {
        Ok(Json(serde_json::json!({
            "session_id": session_id,
            "state": format!("{:?}", session.state),
            "protocol": session.protocol,
            "wallet_id": session.wallet_id,
            "has_signature": session.signature.is_some(),
            "age_secs": session.created_at.elapsed().as_secs(),
        })))
    } else {
        Err((StatusCode::NOT_FOUND, "Session not found".to_string()))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_p2p_sign_status_serialization() {
        let status = P2pSignStatus::Completed;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"completed\"");

        let status = P2pSignStatus::WaitingForAcks;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"waiting_for_acks\"");
    }
}
