//! P2P Transport implementation.
//!
//! Implements the `Transport` trait for direct peer-to-peer messaging,
//! eliminating the need for coordinator relay.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use super::connection::{ConnectionManager, DEFAULT_P2P_PORT};
use super::protocol::P2pMessage;
use super::receiver::{drain_session_messages, IncomingQueue};
use crate::relay::{PollMessagesResponse, RelayMessage};
use crate::transport::{Transport, TransportError, TransportType};

/// P2P Transport for direct peer-to-peer messaging.
pub struct P2pTransport {
    /// Our party index.
    party_index: u16,
    /// Connection manager for outgoing connections.
    connection_manager: Arc<ConnectionManager>,
    /// Incoming message queue (populated by receiver task).
    incoming_queue: IncomingQueue,
    /// Active sessions we're tracking.
    active_sessions: Arc<RwLock<HashSet<String>>>,
}

impl P2pTransport {
    /// Create a new P2P transport.
    ///
    /// # Arguments
    /// * `party_index` - Our party index
    /// * `registry_url` - URL of the node registry (e.g., "http://mpc-coordinator:3000")
    /// * `incoming_queue` - Shared queue for incoming messages (from receiver task)
    /// * `p2p_port` - P2P port used by all nodes (default: 4000)
    pub fn new(
        party_index: u16,
        registry_url: &str,
        incoming_queue: IncomingQueue,
        p2p_port: Option<u16>,
    ) -> Self {
        let p2p_port = p2p_port.unwrap_or(DEFAULT_P2P_PORT);

        Self {
            party_index,
            connection_manager: Arc::new(ConnectionManager::new(
                party_index,
                registry_url.to_string(),
                p2p_port,
            )),
            incoming_queue,
            active_sessions: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Refresh the peer list from the registry.
    pub async fn refresh_peers(&self) -> Result<(), TransportError> {
        self.connection_manager
            .refresh_peers()
            .await
            .map_err(|e| TransportError::Other(e.to_string()))
    }

    /// Start tracking a session.
    pub async fn start_session(&self, session_id: &str) {
        let mut sessions = self.active_sessions.write().await;
        sessions.insert(session_id.to_string());
        debug!("Started tracking P2P session: {}", session_id);
    }

    /// Stop tracking a session.
    pub async fn end_session(&self, session_id: &str) {
        let mut sessions = self.active_sessions.write().await;
        sessions.remove(session_id);
        debug!("Stopped tracking P2P session: {}", session_id);
    }

    /// Check if a session is active.
    pub async fn is_session_active(&self, session_id: &str) -> bool {
        let sessions = self.active_sessions.read().await;
        sessions.contains(session_id)
    }

    /// Send a message to a specific peer.
    async fn send_to_peer(
        &self,
        party_index: u16,
        msg: &RelayMessage,
    ) -> Result<(), TransportError> {
        let p2p_msg = P2pMessage::from(msg.clone());

        self.connection_manager
            .send(party_index, &p2p_msg)
            .await
            .map_err(|e| TransportError::Other(e.to_string()))
    }

    /// Broadcast a message to all peers.
    async fn broadcast(&self, msg: &RelayMessage) -> Result<(), TransportError> {
        let p2p_msg = P2pMessage::from(msg.clone());

        self.connection_manager
            .broadcast(&p2p_msg, Some(self.party_index))
            .await
            .map_err(|e| TransportError::Other(e.to_string()))
    }

    /// Get the connection manager (for testing/advanced use).
    #[allow(dead_code)]
    pub fn connection_manager(&self) -> &Arc<ConnectionManager> {
        &self.connection_manager
    }
}

#[async_trait]
impl Transport for P2pTransport {
    fn party_index(&self) -> u16 {
        self.party_index
    }

    fn transport_type(&self) -> TransportType {
        TransportType::P2p
    }

    async fn send_message(&self, msg: RelayMessage) -> Result<(), TransportError> {
        // Ensure we're tracking this session
        if !self.is_session_active(&msg.session_id).await {
            self.start_session(&msg.session_id).await;
        }

        // Route based on recipient
        match msg.recipient {
            Some(party_index) => {
                debug!(
                    "P2P sending to party {}: session={}, round={}, seq={}",
                    party_index, msg.session_id, msg.round, msg.seq
                );
                self.send_to_peer(party_index, &msg).await
            }
            None => {
                debug!(
                    "P2P broadcasting: session={}, round={}, seq={}",
                    msg.session_id, msg.round, msg.seq
                );
                self.broadcast(&msg).await
            }
        }
    }

    async fn poll_messages(
        &self,
        session_id: &str,
    ) -> Result<PollMessagesResponse, TransportError> {
        // Ensure we're tracking this session
        if !self.is_session_active(session_id).await {
            self.start_session(session_id).await;
        }

        // Drain messages from the queue
        let messages = drain_session_messages(&self.incoming_queue, session_id).await;

        if !messages.is_empty() {
            debug!(
                "P2P poll for session {}: {} messages",
                session_id,
                messages.len()
            );
        }

        Ok(PollMessagesResponse {
            messages,
            session_active: true, // Always active in P2P mode
        })
    }

    async fn notify_aux_info_complete(
        &self,
        session_id: &str,
        success: bool,
        aux_info_size: usize,
        error: Option<String>,
    ) -> Result<(), TransportError> {
        // In P2P mode, aux_info completion is tracked locally
        // We could broadcast a completion message, but for now we just log
        if success {
            debug!(
                "Aux info complete for session {}: {} bytes",
                session_id, aux_info_size
            );
        } else {
            warn!(
                "Aux info failed for session {}: {:?}",
                session_id,
                error.unwrap_or_else(|| "unknown error".to_string())
            );
        }

        // End the session
        self.end_session(session_id).await;

        Ok(())
    }
}

impl std::fmt::Debug for P2pTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("P2pTransport")
            .field("party_index", &self.party_index)
            .field("transport_type", &TransportType::P2p)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::p2p::receiver::new_incoming_queue;

    #[tokio::test]
    async fn test_session_tracking() {
        let queue = new_incoming_queue();
        let transport = P2pTransport::new(0, "http://localhost:3000", queue, None);

        // Initially no sessions
        assert!(!transport.is_session_active("session-1").await);

        // Start a session
        transport.start_session("session-1").await;
        assert!(transport.is_session_active("session-1").await);

        // End the session
        transport.end_session("session-1").await;
        assert!(!transport.is_session_active("session-1").await);
    }

    #[tokio::test]
    async fn test_poll_returns_queued_messages() {
        let queue = new_incoming_queue();

        // Add a message to the queue
        {
            let mut q = queue.write().await;
            q.entry("session-1".to_string())
                .or_insert_with(Vec::new)
                .push(RelayMessage {
                    session_id: "session-1".to_string(),
                    protocol: "test".to_string(),
                    sender: 1,
                    recipient: Some(0),
                    round: 1,
                    payload: vec![1, 2, 3],
                    seq: 1,
                    timestamp: 0,
                });
        }

        let transport = P2pTransport::new(0, "http://localhost:3000", queue, None);

        // Poll should return the message
        let response = transport.poll_messages("session-1").await.unwrap();
        assert_eq!(response.messages.len(), 1);
        assert_eq!(response.messages[0].sender, 1);
        assert!(response.session_active);

        // Polling again should return empty
        let response = transport.poll_messages("session-1").await.unwrap();
        assert_eq!(response.messages.len(), 0);
    }
}
