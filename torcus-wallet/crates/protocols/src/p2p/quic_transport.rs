//! QUIC Transport implementation.
//!
//! Implements the `Transport` trait for secure peer-to-peer messaging
//! using QUIC with TLS 1.3 and CA-signed certificates.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::certs::NodeCertificate;
use super::protocol::P2pMessage;
use super::quic::{QuicConnectionManager, DEFAULT_QUIC_PORT};
use super::quic_listener::{spawn_quic_listener, QuicListener};
use super::receiver::{drain_session_messages, new_incoming_queue, IncomingQueue};
use crate::relay::{PollMessagesResponse, RelayMessage};
use crate::transport::{Transport, TransportError, TransportType};

/// QUIC Transport for secure peer-to-peer messaging.
///
/// This transport uses QUIC with TLS 1.3 for all peer communication,
/// providing:
/// - Mandatory encryption (TLS 1.3)
/// - Mutual authentication (CA-signed certificates)
/// - Connection multiplexing (multiple streams per connection)
/// - Built-in congestion control
pub struct QuicTransport {
    /// Our party index.
    party_index: u16,
    /// QUIC connection manager for outgoing connections.
    connection_manager: Arc<RwLock<QuicConnectionManager>>,
    /// QUIC listener for incoming connections.
    listener: Arc<RwLock<Option<QuicListener>>>,
    /// Incoming message queue (populated by listener).
    incoming_queue: IncomingQueue,
    /// Active sessions we're tracking.
    active_sessions: Arc<RwLock<HashSet<String>>>,
    /// Listen address.
    listen_addr: SocketAddr,
}

impl QuicTransport {
    /// Create a new QUIC transport.
    ///
    /// # Arguments
    /// * `party_index` - Our party index
    /// * `registry_url` - URL of the node registry (e.g., "http://mpc-coordinator:3000")
    /// * `node_cert` - Our node certificate (signed by CA)
    /// * `listen_addr` - Address to listen on for incoming connections
    /// * `quic_port` - QUIC port used by peers (default: 4001)
    pub fn new(
        party_index: u16,
        registry_url: &str,
        node_cert: NodeCertificate,
        listen_addr: SocketAddr,
        quic_port: Option<u16>,
    ) -> Result<Self, TransportError> {
        let quic_port = quic_port.unwrap_or(DEFAULT_QUIC_PORT);
        let incoming_queue = new_incoming_queue();

        let connection_manager =
            QuicConnectionManager::new(party_index, registry_url.to_string(), quic_port, node_cert)
                .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;

        Ok(Self {
            party_index,
            connection_manager: Arc::new(RwLock::new(connection_manager)),
            listener: Arc::new(RwLock::new(None)),
            incoming_queue,
            active_sessions: Arc::new(RwLock::new(HashSet::new())),
            listen_addr,
        })
    }

    /// Initialize the transport (start client and listener).
    ///
    /// This must be called before sending or receiving messages.
    pub async fn init(&self) -> Result<(), TransportError> {
        // Initialize client endpoint
        {
            let mut manager = self.connection_manager.write().await;
            manager
                .init_client()
                .await
                .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;
        }

        // Start listener
        {
            let manager = self.connection_manager.read().await;
            let server_config = manager
                .server_config()
                .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;

            let listener =
                spawn_quic_listener(self.listen_addr, server_config, self.incoming_queue.clone())
                    .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;

            info!(
                "QUIC transport initialized: listening on {}",
                listener.local_addr()
            );

            let mut listener_guard = self.listener.write().await;
            *listener_guard = Some(listener);
        }

        Ok(())
    }

    /// Refresh the peer list from the registry.
    pub async fn refresh_peers(&self) -> Result<(), TransportError> {
        let manager = self.connection_manager.read().await;
        manager
            .refresh_peers()
            .await
            .map_err(|e| TransportError::Other(e.to_string()))
    }

    /// Start tracking a session.
    pub async fn start_session(&self, session_id: &str) {
        let mut sessions = self.active_sessions.write().await;
        sessions.insert(session_id.to_string());
        debug!("Started tracking QUIC session: {}", session_id);
    }

    /// Stop tracking a session.
    pub async fn end_session(&self, session_id: &str) {
        let mut sessions = self.active_sessions.write().await;
        sessions.remove(session_id);
        debug!("Stopped tracking QUIC session: {}", session_id);
    }

    /// Check if a session is active.
    pub async fn is_session_active(&self, session_id: &str) -> bool {
        let sessions = self.active_sessions.read().await;
        sessions.contains(session_id)
    }

    /// Get the incoming message queue (for external use).
    pub fn incoming_queue(&self) -> IncomingQueue {
        self.incoming_queue.clone()
    }

    /// Get the local listen address.
    pub async fn local_addr(&self) -> Option<SocketAddr> {
        let listener_guard = self.listener.read().await;
        listener_guard.as_ref().map(|l| l.local_addr())
    }

    /// Shutdown the transport gracefully.
    pub async fn shutdown(&self) {
        // Stop listener
        {
            let mut listener_guard = self.listener.write().await;
            if let Some(mut listener) = listener_guard.take() {
                listener.stop().await;
            }
        }

        // Close all connections
        {
            let manager = self.connection_manager.read().await;
            manager.close_all().await;
        }

        info!("QUIC transport shutdown complete");
    }

    /// Send a message to a specific peer.
    async fn send_to_peer(
        &self,
        party_index: u16,
        msg: &RelayMessage,
    ) -> Result<(), TransportError> {
        let p2p_msg = P2pMessage::from(msg.clone());

        let manager = self.connection_manager.read().await;
        manager
            .send(party_index, &p2p_msg)
            .await
            .map_err(|e| TransportError::SendFailed(e.to_string()))
    }

    /// Broadcast a message to all peers.
    async fn broadcast(&self, msg: &RelayMessage) -> Result<(), TransportError> {
        let p2p_msg = P2pMessage::from(msg.clone());

        let manager = self.connection_manager.read().await;
        manager
            .broadcast(&p2p_msg, Some(self.party_index))
            .await
            .map_err(|e| TransportError::SendFailed(e.to_string()))
    }
}

#[async_trait]
impl Transport for QuicTransport {
    fn party_index(&self) -> u16 {
        self.party_index
    }

    fn transport_type(&self) -> TransportType {
        // Reuse P2p type since QUIC is a P2P transport variant
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
                    "QUIC sending to party {}: session={}, round={}, seq={}",
                    party_index, msg.session_id, msg.round, msg.seq
                );
                self.send_to_peer(party_index, &msg).await
            }
            None => {
                debug!(
                    "QUIC broadcasting: session={}, round={}, seq={}",
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
                "QUIC poll for session {}: {} messages",
                session_id,
                messages.len()
            );
        }

        Ok(PollMessagesResponse {
            messages,
            session_active: true, // Always active in QUIC mode
        })
    }

    async fn notify_aux_info_complete(
        &self,
        session_id: &str,
        success: bool,
        aux_info_size: usize,
        error: Option<String>,
    ) -> Result<(), TransportError> {
        // In QUIC mode, aux_info completion is tracked locally
        if success {
            debug!(
                "Aux info complete for session {}: {} bytes (QUIC)",
                session_id, aux_info_size
            );
        } else {
            warn!(
                "Aux info failed for session {}: {:?} (QUIC)",
                session_id,
                error.unwrap_or_else(|| "unknown error".to_string())
            );
        }

        // End the session
        self.end_session(session_id).await;

        Ok(())
    }
}

impl std::fmt::Debug for QuicTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuicTransport")
            .field("party_index", &self.party_index)
            .field("listen_addr", &self.listen_addr)
            .field("transport_type", &"quic")
            .finish()
    }
}

/// Builder for QuicTransport with convenient configuration.
pub struct QuicTransportBuilder {
    party_index: u16,
    registry_url: String,
    node_cert: Option<NodeCertificate>,
    listen_addr: SocketAddr,
    quic_port: u16,
}

impl QuicTransportBuilder {
    /// Create a new builder.
    pub fn new(party_index: u16, registry_url: &str) -> Self {
        Self {
            party_index,
            registry_url: registry_url.to_string(),
            node_cert: None,
            listen_addr: format!("0.0.0.0:{}", DEFAULT_QUIC_PORT).parse().unwrap(),
            quic_port: DEFAULT_QUIC_PORT,
        }
    }

    /// Set the node certificate.
    pub fn with_cert(mut self, cert: NodeCertificate) -> Self {
        self.node_cert = Some(cert);
        self
    }

    /// Set the listen address.
    pub fn with_listen_addr(mut self, addr: SocketAddr) -> Self {
        self.listen_addr = addr;
        self
    }

    /// Set the QUIC port for peers.
    pub fn with_quic_port(mut self, port: u16) -> Self {
        self.quic_port = port;
        self
    }

    /// Build the transport.
    pub fn build(self) -> Result<QuicTransport, TransportError> {
        let cert = self.node_cert.ok_or_else(|| {
            TransportError::ConnectionFailed("Node certificate required".to_string())
        })?;

        QuicTransport::new(
            self.party_index,
            &self.registry_url,
            cert,
            self.listen_addr,
            Some(self.quic_port),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::p2p::certs::CaCertificate;

    fn create_test_cert(party_index: u16) -> NodeCertificate {
        let ca = CaCertificate::generate().unwrap();
        ca.sign_node_cert(party_index, &["localhost".to_string()])
            .unwrap()
    }

    #[tokio::test]
    async fn test_session_tracking() {
        let cert = create_test_cert(0);
        let transport = QuicTransport::new(
            0,
            "http://localhost:3000",
            cert,
            "127.0.0.1:0".parse().unwrap(),
            None,
        )
        .unwrap();

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
        let cert = create_test_cert(0);
        let transport = QuicTransport::new(
            0,
            "http://localhost:3000",
            cert,
            "127.0.0.1:0".parse().unwrap(),
            None,
        )
        .unwrap();

        // Add a message to the queue directly
        {
            let mut q = transport.incoming_queue.write().await;
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

        // Poll should return the message
        let response = transport.poll_messages("session-1").await.unwrap();
        assert_eq!(response.messages.len(), 1);
        assert_eq!(response.messages[0].sender, 1);
        assert!(response.session_active);

        // Polling again should return empty
        let response = transport.poll_messages("session-1").await.unwrap();
        assert_eq!(response.messages.len(), 0);
    }

    #[test]
    fn test_builder() {
        let cert = create_test_cert(0);
        let transport = QuicTransportBuilder::new(0, "http://localhost:3000")
            .with_cert(cert)
            .with_listen_addr("127.0.0.1:5000".parse().unwrap())
            .with_quic_port(5000)
            .build();

        assert!(transport.is_ok());
        let transport = transport.unwrap();
        assert_eq!(transport.party_index, 0);
    }
}
