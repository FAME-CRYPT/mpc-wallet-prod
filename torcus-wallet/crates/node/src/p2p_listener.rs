//! P2P Control Message Listener
//!
//! This module handles incoming P2P control messages (SessionProposal,
//! SessionAck, SessionStart, SessionAbort) and routes them to the
//! appropriate handlers.
//!
//! ## Message Flow
//!
//! ```text
//! Incoming QUIC message
//!     │
//!     ▼
//! Parse as SessionControlMessage
//!     │
//!     ├── Proposal → handle_incoming_proposal()
//!     ├── Ack      → handle_incoming_ack() (initiator only)
//!     ├── Start    → handle_incoming_start() → spawn_participant_signing()
//!     └── Abort    → handle_incoming_abort()
//! ```

#![allow(dead_code)] // Will be used when P2P listener is spawned

use std::sync::Arc;

use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use protocols::p2p::{
    P2pSessionCoordinator, QuicTransport, SessionAck, SessionControlMessage, SessionProposal,
    SessionStart,
};

use crate::state::NodeState;

/// Spawn the control message listener background task.
///
/// This task continuously polls for incoming P2P control messages
/// and routes them to the appropriate handlers.
pub fn spawn_control_message_listener(
    state: Arc<RwLock<NodeState>>,
    coordinator: Arc<P2pSessionCoordinator>,
    transport: Arc<QuicTransport>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        info!("P2P control message listener started");

        // Poll interval for control messages
        let poll_interval = std::time::Duration::from_millis(100);

        loop {
            // Poll for incoming control messages from the transport's queue
            match poll_control_messages(&transport).await {
                Ok(messages) => {
                    for msg in messages {
                        if let Err(e) =
                            handle_control_message(&state, &coordinator, &transport, msg).await
                        {
                            error!("Error handling control message: {}", e);
                        }
                    }
                }
                Err(e) => {
                    debug!("Error polling control messages: {}", e);
                    // Don't spam logs on expected errors
                }
            }

            tokio::time::sleep(poll_interval).await;
        }
    })
}

/// Poll for incoming control messages from the transport's queue.
///
/// Control messages are stored in the incoming queue with a special
/// session ID prefix "__control__".
async fn poll_control_messages(
    transport: &Arc<QuicTransport>,
) -> Result<Vec<SessionControlMessage>, String> {
    use protocols::p2p::drain_session_messages;

    // Get the incoming queue from the transport
    let queue = transport.incoming_queue();

    // Drain control messages (session_id starting with "__control__")
    let relay_messages = drain_session_messages(&queue, "__control__").await;

    // Parse the payload as SessionControlMessage
    let mut control_messages = Vec::new();
    for msg in relay_messages {
        // Try to deserialize the payload as a SessionControlMessage
        match bincode::deserialize::<SessionControlMessage>(&msg.payload) {
            Ok(ctrl) => {
                control_messages.push(ctrl);
            }
            Err(e) => {
                debug!("Failed to parse control message: {}", e);
            }
        }
    }

    Ok(control_messages)
}

/// Handle a single control message.
async fn handle_control_message(
    state: &Arc<RwLock<NodeState>>,
    coordinator: &Arc<P2pSessionCoordinator>,
    transport: &Arc<QuicTransport>,
    msg: SessionControlMessage,
) -> Result<(), String> {
    match msg {
        SessionControlMessage::Proposal(proposal) => {
            handle_incoming_proposal(state, coordinator, transport, proposal).await
        }
        SessionControlMessage::Ack(ack) => handle_incoming_ack(coordinator, ack).await,
        SessionControlMessage::Start(start) => {
            handle_incoming_start(state, coordinator, transport, start).await
        }
        SessionControlMessage::Abort(abort) => {
            handle_incoming_abort(coordinator, &abort.session_id, &abort.reason).await
        }
    }
}

/// Handle an incoming SessionProposal (we're a participant).
async fn handle_incoming_proposal(
    _state: &Arc<RwLock<NodeState>>,
    coordinator: &Arc<P2pSessionCoordinator>,
    transport: &Arc<QuicTransport>,
    proposal: SessionProposal,
) -> Result<(), String> {
    let session_id = proposal.session_id();
    info!(
        "Received session proposal: session_id={}, initiator={}, protocol={}",
        session_id, proposal.initiator_party, proposal.protocol
    );

    // Let the coordinator handle validation and create the ack
    let ack = coordinator
        .handle_proposal(&proposal)
        .await
        .map_err(|e| format!("Coordinator error: {}", e))?;

    // Send the ack back to the initiator
    send_ack_to_initiator(transport, &ack, proposal.initiator_party).await?;

    if ack.accepted {
        info!(
            "Accepted proposal and sent ack: session_id={}, initiator={}",
            session_id, proposal.initiator_party
        );
    } else {
        warn!(
            "Rejected proposal: session_id={}, reason={:?}",
            session_id, ack.error
        );
    }

    Ok(())
}

/// Handle an incoming SessionAck (we're the initiator).
async fn handle_incoming_ack(
    coordinator: &Arc<P2pSessionCoordinator>,
    ack: SessionAck,
) -> Result<(), String> {
    debug!(
        "Received session ack: session_id={}, party={}, accepted={}",
        ack.session_id, ack.party_index, ack.accepted
    );

    coordinator
        .handle_ack(ack)
        .await
        .map_err(|e| format!("Coordinator error: {}", e))?;

    Ok(())
}

/// Handle an incoming SessionStart (we're a participant, time to execute).
async fn handle_incoming_start(
    state: &Arc<RwLock<NodeState>>,
    coordinator: &Arc<P2pSessionCoordinator>,
    transport: &Arc<QuicTransport>,
    start: SessionStart,
) -> Result<(), String> {
    info!(
        "Received session start: session_id={}, participants={:?}",
        start.session_id, start.participants
    );

    // Update coordinator state
    coordinator
        .handle_start(&start)
        .await
        .map_err(|e| format!("Coordinator error: {}", e))?;

    // Get the grant for this session
    let grant = coordinator
        .get_session_grant(&start.session_id)
        .await
        .ok_or_else(|| format!("Grant not found for session {}", start.session_id))?;

    // Spawn the participant signing task
    spawn_participant_signing(
        state.clone(),
        coordinator.clone(),
        start.session_id,
        grant,
        start.participants,
        transport.clone(),
    );

    Ok(())
}

/// Handle an incoming SessionAbort.
async fn handle_incoming_abort(
    coordinator: &Arc<P2pSessionCoordinator>,
    session_id: &str,
    reason: &str,
) -> Result<(), String> {
    warn!(
        "Received session abort: session_id={}, reason={}",
        session_id, reason
    );

    let abort = protocols::p2p::SessionAbort::new(
        session_id.to_string(),
        coordinator.party_index(),
        reason.to_string(),
    );

    coordinator
        .handle_abort(&abort)
        .await
        .map_err(|e| format!("Coordinator error: {}", e))?;

    Ok(())
}

/// Send an ack back to the initiator via the transport.
async fn send_ack_to_initiator(
    transport: &Arc<QuicTransport>,
    ack: &SessionAck,
    initiator_party: u16,
) -> Result<(), String> {
    use protocols::p2p::P2pMessage;
    use protocols::Transport;

    // Ensure peer registry is up to date before sending
    // This handles the case where we received an incoming connection but haven't
    // refreshed peers yet
    if let Err(e) = transport.refresh_peers().await {
        warn!("Failed to refresh peers before sending ack: {}", e);
        // Continue anyway - peer might already be in registry
    }

    // Serialize the control message
    let ctrl_msg = SessionControlMessage::Ack(ack.clone());
    let payload =
        bincode::serialize(&ctrl_msg).map_err(|e| format!("Failed to serialize ack: {}", e))?;

    // Create a P2pMessage with the control payload
    let msg = P2pMessage {
        session_id: "__control__".to_string(),
        sender: ack.party_index,
        recipient: Some(initiator_party),
        round: 0, // Control messages don't have rounds
        payload,
        seq: 0,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
    };

    // Convert to RelayMessage for the Transport trait
    let relay_msg = protocols::relay::RelayMessage::from(msg);

    transport
        .send_message(relay_msg)
        .await
        .map_err(|e| format!("Failed to send ack: {}", e))?;

    Ok(())
}

/// Spawn a background task to execute signing as a participant.
///
/// This is called when we receive a SessionStart message and need to
/// participate in the signing protocol.
fn spawn_participant_signing(
    state: Arc<RwLock<NodeState>>,
    coordinator: Arc<P2pSessionCoordinator>,
    session_id: String,
    grant: common::SigningGrant,
    participants: Vec<u16>,
    transport: Arc<QuicTransport>,
) {
    tokio::spawn(async move {
        info!(
            "Starting participant signing: session_id={}, participants={:?}",
            session_id, participants
        );

        match execute_participant_signing(&state, &session_id, &grant, &participants, &transport)
            .await
        {
            Ok(()) => {
                info!("Participant signing completed: session_id={}", session_id);
                // Mark session as complete
                let _ = coordinator.complete_session(&session_id).await;
            }
            Err(e) => {
                error!(
                    "Participant signing failed: session_id={}, error={}",
                    session_id, e
                );

                // Send abort to other participants
                let _ = broadcast_abort(&transport, &session_id, &participants, &e).await;
            }
        }
    });
}

/// Execute the signing protocol as a participant.
async fn execute_participant_signing(
    state: &Arc<RwLock<NodeState>>,
    session_id: &str,
    grant: &common::SigningGrant,
    participants: &[u16],
    transport: &Arc<QuicTransport>,
) -> Result<(), String> {
    use protocols::cggmp24::ProtocolMessage;
    use protocols::Transport;

    // Get our state - use the same pattern as p2p.rs execute_signing_protocol
    let (party_index, key_share_bytes, aux_info) = {
        let s = state.read().await;

        let party_index = s.party_index;

        // Get key share for this wallet
        let key_share = s
            .cggmp24_key_shares
            .get(&grant.wallet_id)
            .ok_or_else(|| format!("No key share for wallet {}", grant.wallet_id))?;

        // Use the raw incomplete_key_share bytes (not the whole StoredKeyShare struct)
        let key_share_bytes = key_share.incomplete_key_share.clone();

        // Use aux_info from the key share (stored during keygen)
        let aux_info = key_share.aux_info.clone();

        (party_index, key_share_bytes, aux_info)
    };

    info!(
        "Executing signing protocol as participant: session_id={}, party={}, participants={:?}",
        session_id, party_index, participants
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
                    debug!("Poll error (may be normal): {}", e);
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

    // Log result
    if signature_result.success {
        info!(
            "Participant signing completed successfully: session_id={}, party={}",
            session_id, party_index
        );
        Ok(())
    } else {
        let err_msg = signature_result
            .error
            .unwrap_or_else(|| "Unknown error".to_string());
        error!(
            "Participant signing failed: session_id={}, party={}, error={}",
            session_id, party_index, err_msg
        );
        Err(format!("Signing protocol failed: {}", err_msg))
    }
}

/// Broadcast an abort message to all participants.
async fn broadcast_abort(
    transport: &Arc<QuicTransport>,
    session_id: &str,
    participants: &[u16],
    reason: &str,
) -> Result<(), String> {
    use protocols::p2p::{P2pMessage, SessionAbort};
    use protocols::Transport;

    let party_index = {
        // Get our party index from the transport
        transport.party_index()
    };

    let abort = SessionAbort::new(session_id.to_string(), party_index, reason.to_string());
    let ctrl_msg = SessionControlMessage::Abort(abort);
    let payload =
        bincode::serialize(&ctrl_msg).map_err(|e| format!("Failed to serialize abort: {}", e))?;

    for &party in participants {
        if party == party_index {
            continue; // Don't send to ourselves
        }

        let msg = P2pMessage {
            session_id: "__control__".to_string(),
            sender: party_index,
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

        if let Err(e) = transport.send_message(relay_msg).await {
            warn!("Failed to send abort to party {}: {}", party, e);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    // Basic smoke tests - real testing requires full P2P infrastructure
    #[test]
    fn test_module_compiles() {
        // Just verify the module compiles correctly
        // This test exists to catch compilation errors in this module
    }
}
