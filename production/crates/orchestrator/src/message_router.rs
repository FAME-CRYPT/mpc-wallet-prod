//! QUIC Message Router
//!
//! Routes messages between QUIC network layer and protocol execution channels.
//! This is the critical bridge that enables all MPC protocols to communicate
//! across the network.
//!
//! # Architecture
//!
//! ```text
//! Protocol Channels          Message Router          QUIC Network
//! ┌─────────────────┐       ┌──────────────┐       ┌─────────────┐
//! │                 │       │              │       │             │
//! │  outgoing_tx ───┼──────►│ Route Out    │──────►│ QuicEngine  │
//! │                 │       │              │       │   .send()   │
//! │                 │       │              │       │             │
//! │  incoming_rx ◄──┼───────│ Route In     │◄──────│ QuicEngine  │
//! │                 │       │              │       │  .receive() │
//! └─────────────────┘       └──────────────┘       └─────────────┘
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use async_channel::{Sender, Receiver};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use threshold_network::QuicEngine;
use threshold_types::{NetworkMessage, NodeId};

use crate::error::{OrchestrationError, Result};

/// Protocol session information
#[derive(Debug, Clone)]
struct ProtocolSession {
    /// Session ID
    session_id: Uuid,
    /// Protocol type (for routing/debugging)
    protocol_type: ProtocolType,
    /// Sender for incoming messages from QUIC
    incoming_tx: Sender<ProtocolMessage>,
    /// Receiver for outgoing messages to QUIC
    outgoing_rx: Receiver<ProtocolMessage>,
    /// Participating nodes
    participants: Vec<NodeId>,
}

/// Protocol type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolType {
    DKG,
    AuxInfo,
    Presignature,
    Signing,
}

impl std::fmt::Display for ProtocolType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtocolType::DKG => write!(f, "DKG"),
            ProtocolType::AuxInfo => write!(f, "AuxInfo"),
            ProtocolType::Presignature => write!(f, "Presignature"),
            ProtocolType::Signing => write!(f, "Signing"),
        }
    }
}

/// Protocol message wrapper
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProtocolMessage {
    /// Session ID
    pub session_id: Uuid,
    /// Sender node ID
    pub from: NodeId,
    /// Recipient node ID
    pub to: NodeId,
    /// Protocol-specific payload
    pub payload: Vec<u8>,
    /// Message sequence number
    pub sequence: u64,
    /// Whether this message was originally a broadcast (true) or P2P (false)
    pub is_broadcast: bool,
}

/// QUIC Message Router
///
/// Manages bidirectional message routing between protocol execution channels
/// and the QUIC network layer.
pub struct MessageRouter {
    /// QUIC engine for network communication
    quic: Arc<QuicEngine>,
    /// Current node ID
    node_id: NodeId,
    /// Active protocol sessions (session_id -> session info)
    active_sessions: Arc<RwLock<HashMap<Uuid, ProtocolSession>>>,
    /// Shutdown signal
    shutdown: Arc<RwLock<bool>>,
}

impl MessageRouter {
    /// Create new message router
    pub fn new(quic: Arc<QuicEngine>, node_id: NodeId) -> Self {
        Self {
            quic,
            node_id,
            active_sessions: Arc::new(RwLock::new(HashMap::new())),
            shutdown: Arc::new(RwLock::new(false)),
        }
    }

    /// Register a new protocol session
    ///
    /// Returns (incoming_tx, outgoing_rx) for the protocol to use
    pub async fn register_session(
        &self,
        session_id: Uuid,
        protocol_type: ProtocolType,
        participants: Vec<NodeId>,
    ) -> Result<(Sender<ProtocolMessage>, Receiver<ProtocolMessage>)> {
        // Create channels for this session
        let (incoming_tx, incoming_rx) = async_channel::bounded(100);
        let (outgoing_tx, outgoing_rx) = async_channel::bounded(100);

        // Store session info
        let participant_count = participants.len();
        let session = ProtocolSession {
            session_id,
            protocol_type,
            incoming_tx: incoming_tx.clone(),
            outgoing_rx: outgoing_rx.clone(),
            participants,
        };

        {
            let mut sessions = self.active_sessions.write().await;
            sessions.insert(session_id, session);
        }

        info!(
            "Registered {} protocol session {} with {} participants",
            protocol_type,
            session_id,
            participant_count
        );

        // Start routing tasks for this session
        self.start_routing_tasks(session_id).await?;

        Ok((outgoing_tx, incoming_rx))
    }

    /// Unregister a protocol session
    pub async fn unregister_session(&self, session_id: Uuid) -> Result<()> {
        let mut sessions = self.active_sessions.write().await;
        if let Some(session) = sessions.remove(&session_id) {
            info!(
                "Unregistered {} protocol session {}",
                session.protocol_type, session_id
            );
        }
        Ok(())
    }

    /// Start routing tasks for a session
    async fn start_routing_tasks(&self, session_id: Uuid) -> Result<()> {
        // Get session info
        let session = {
            let sessions = self.active_sessions.read().await;
            sessions
                .get(&session_id)
                .ok_or_else(|| {
                    OrchestrationError::Internal(format!("Session {} not found", session_id))
                })?
                .clone()
        };

        // Spawn outgoing message router
        let quic_out = Arc::clone(&self.quic);
        let node_id_out = self.node_id;
        let session_id_out = session_id;
        let outgoing_rx = session.outgoing_rx.clone();
        let shutdown_out = Arc::clone(&self.shutdown);

        tokio::spawn(async move {
            Self::route_outgoing_messages(
                quic_out,
                node_id_out,
                session_id_out,
                outgoing_rx,
                shutdown_out,
            )
            .await;
        });

        // Note: Incoming message routing is handled by a global listener
        // that dispatches to the correct session based on message content

        Ok(())
    }

    /// Route outgoing messages from protocol to QUIC
    async fn route_outgoing_messages(
        quic: Arc<QuicEngine>,
        node_id: NodeId,
        session_id: Uuid,
        outgoing_rx: Receiver<ProtocolMessage>,
        shutdown: Arc<RwLock<bool>>,
    ) {
        info!("Starting outgoing message router for session {}", session_id);

        loop {
            // Check shutdown
            if *shutdown.read().await {
                debug!("Outgoing router shutting down for session {}", session_id);
                break;
            }

            // Try to receive message with timeout
            match tokio::time::timeout(
                std::time::Duration::from_millis(100),
                outgoing_rx.recv(),
            )
            .await
            {
                Ok(Ok(msg)) => {
                    info!(
                        "📬 MessageRouter received outgoing message: from={} to={} session={} seq={}",
                        msg.from, msg.to, session_id, msg.sequence
                    );

                    // Send message via QUIC
                    let network_msg = NetworkMessage::Protocol {
                        session_id: msg.session_id.to_string(),
                        from: msg.from,
                        to: msg.to,
                        payload: msg.payload.clone(),
                        is_broadcast: msg.is_broadcast,
                    };

                    // Use stream ID based on session ID hash for consistency
                    let stream_id = session_id.as_u128() as u64;

                    if let Err(e) = quic.send(&msg.to, &network_msg, stream_id).await {
                        error!(
                            "Failed to send message to {} for session {}: {}",
                            msg.to, session_id, e
                        );
                    } else {
                        info!(
                            "✅ Sent via QUIC to {} for session {} (seq: {})",
                            msg.to, session_id, msg.sequence
                        );
                    }
                }
                Ok(Err(e)) => {
                    debug!("Outgoing channel closed for session {}: {}", session_id, e);
                    break;
                }
                Err(_) => {
                    // Timeout, continue loop
                    continue;
                }
            }
        }

        info!("Outgoing message router stopped for session {}", session_id);
    }

    /// Handle incoming message from QUIC
    ///
    /// Called by the QUIC listener when a protocol message arrives
    pub async fn handle_incoming_message(
        &self,
        from: NodeId,
        to: NodeId,
        session_id_str: &str,
        payload: Vec<u8>,
        sequence: u64,
        is_broadcast: bool,
    ) -> Result<()> {
        info!(
            "📨 MessageRouter handling incoming from QUIC: from={} to={} session={}",
            from, to, session_id_str
        );

        // Parse session ID
        let session_id = Uuid::parse_str(session_id_str).map_err(|e| {
            OrchestrationError::Internal(format!("Invalid session ID {}: {}", session_id_str, e))
        })?;

        // Get session
        let session = {
            let sessions = self.active_sessions.read().await;
            sessions.get(&session_id).cloned()
        };

        match session {
            Some(session) => {
                let msg = ProtocolMessage {
                    session_id,
                    from,
                    to,
                    payload,
                    sequence,
                    is_broadcast,
                };

                info!(
                    "✅ Forwarding to protocol: from={} to={} session={}",
                    from, to, session_id
                );

                // Forward to protocol channel
                if let Err(e) = session.incoming_tx.send(msg).await {
                    warn!(
                        "Failed to forward message to protocol for session {}: {}",
                        session_id, e
                    );
                    return Err(OrchestrationError::Internal(format!(
                        "Channel send failed: {}",
                        e
                    )));
                }

                debug!(
                    "Routed incoming message from {} to protocol (session: {}, seq: {})",
                    from, session_id, sequence
                );
                Ok(())
            }
            None => {
                warn!("Received message for unknown session {}", session_id);
                Err(OrchestrationError::Internal(format!(
                    "Unknown session {}",
                    session_id
                )))
            }
        }
    }

    /// Broadcast message to all participants in a session
    pub async fn broadcast_message(
        &self,
        session_id: Uuid,
        message: ProtocolMessage,
    ) -> Result<()> {
        let session = {
            let sessions = self.active_sessions.read().await;
            sessions.get(&session_id).cloned()
        };

        match session {
            Some(session) => {
                let network_msg = NetworkMessage::Protocol {
                    session_id: session_id.to_string(),
                    from: message.from,
                    to: message.to,
                    payload: message.payload.clone(),
                    is_broadcast: message.is_broadcast,
                };

                let stream_id = session_id.as_u128() as u64;

                // Broadcast to all participants except sender
                for participant in &session.participants {
                    if *participant != message.from {
                        if let Err(e) = self.quic.send(participant, &network_msg, stream_id).await
                        {
                            error!(
                                "Failed to broadcast to {} for session {}: {}",
                                participant, session_id, e
                            );
                        }
                    }
                }

                debug!(
                    "Broadcast message for session {} to {} participants",
                    session_id,
                    session.participants.len() - 1
                );
                Ok(())
            }
            None => Err(OrchestrationError::Internal(format!(
                "Session {} not found",
                session_id
            ))),
        }
    }

    /// Get number of active sessions
    pub async fn active_session_count(&self) -> usize {
        let sessions = self.active_sessions.read().await;
        sessions.len()
    }

    /// Shutdown the message router
    pub async fn shutdown(&self) {
        info!("Shutting down message router");
        *self.shutdown.write().await = true;

        // Clear all sessions
        let mut sessions = self.active_sessions.write().await;
        sessions.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_registration() {
        // This test requires a QuicEngine which needs network setup
        // Keeping as placeholder for integration tests
    }

    #[tokio::test]
    async fn test_message_routing() {
        // Integration test placeholder
    }
}
