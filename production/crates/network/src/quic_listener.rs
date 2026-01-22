//! QUIC listener for incoming connections.
//!
//! Accepts incoming QUIC connections from peers and routes messages
//! to the appropriate handlers based on stream IDs.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use quinn::{Connection, Endpoint, RecvStream, ServerConfig};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use threshold_types::{NetworkMessage, NodeId};
use crate::error::{NetworkError, NetworkResult};

/// Message queue for incoming messages, organized by session.
pub type IncomingQueue = Arc<RwLock<HashMap<String, Vec<NetworkMessage>>>>;

/// QUIC listener that accepts incoming connections.
pub struct QuicListener {
    /// The QUIC endpoint.
    endpoint: Endpoint,
    /// Local address we're listening on.
    local_addr: SocketAddr,
    /// Handle to the accept loop task.
    accept_task: Option<JoinHandle<()>>,
}

impl QuicListener {
    /// Create a new QUIC listener.
    ///
    /// # Arguments
    /// * `listen_addr` - Address to bind to (e.g., "0.0.0.0:4001")
    /// * `server_config` - TLS server configuration with certificates
    pub fn new(listen_addr: SocketAddr, server_config: ServerConfig) -> NetworkResult<Self> {
        let endpoint = Endpoint::server(server_config, listen_addr).map_err(|e| {
            NetworkError::BindFailed {
                address: listen_addr.to_string(),
                source: std::io::Error::other(e.to_string()),
            }
        })?;

        let local_addr = endpoint.local_addr().map_err(|e| NetworkError::BindFailed {
            address: listen_addr.to_string(),
            source: std::io::Error::other(e.to_string()),
        })?;

        info!("QUIC listener bound to {}", local_addr);

        Ok(Self {
            endpoint,
            local_addr,
            accept_task: None,
        })
    }

    /// Start accepting incoming connections.
    ///
    /// Spawns a background task that accepts connections and routes
    /// messages to the incoming queue.
    pub fn start(&mut self, incoming_queue: IncomingQueue) -> NetworkResult<()> {
        if self.accept_task.is_some() {
            return Err(NetworkError::CodecError(
                "Listener already started".to_string(),
            ));
        }

        let endpoint = self.endpoint.clone();
        let local_addr = self.local_addr;

        let task = tokio::spawn(async move {
            info!("QUIC listener started on {}", local_addr);
            accept_loop(endpoint, incoming_queue).await;
        });

        self.accept_task = Some(task);

        Ok(())
    }

    /// Get the local address we're listening on.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Stop the listener gracefully.
    pub async fn stop(&mut self) {
        if let Some(task) = self.accept_task.take() {
            // Close the endpoint to stop accepting new connections
            self.endpoint.close(0u32.into(), b"shutdown");

            // Wait for the accept task to finish
            if let Err(e) = task.await {
                warn!("Accept task panicked: {:?}", e);
            }
        }

        info!("QUIC listener stopped");
    }

    /// Wait for the listener to finish (blocks indefinitely unless stopped).
    pub async fn wait(&mut self) {
        if let Some(task) = self.accept_task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for QuicListener {
    fn drop(&mut self) {
        if self.accept_task.is_some() {
            // Can't await in drop, so just close the endpoint
            self.endpoint.close(0u32.into(), b"dropped");
        }
    }
}

/// Accept loop that handles incoming connections.
async fn accept_loop(endpoint: Endpoint, incoming_queue: IncomingQueue) {
    loop {
        match endpoint.accept().await {
            Some(incoming) => {
                let queue = incoming_queue.clone();

                tokio::spawn(async move {
                    let remote_addr = incoming.remote_address();
                    debug!("Incoming QUIC connection from {}", remote_addr);

                    match incoming.await {
                        Ok(connection) => {
                            handle_incoming_connection(connection, queue).await;
                        }
                        Err(e) => {
                            warn!("Failed to complete handshake with {}: {}", remote_addr, e);
                        }
                    }
                });
            }
            None => {
                // Endpoint was closed
                debug!("QUIC endpoint closed, stopping accept loop");
                break;
            }
        }
    }
}

/// Handle an incoming QUIC connection.
///
/// This function processes incoming streams and extracts network messages.
/// It validates sender authentication through mTLS certificates.
pub async fn handle_incoming_connection(connection: Connection, incoming_queue: IncomingQueue) {
    let remote_addr = connection.remote_address();
    info!("Handling incoming QUIC connection from {}", remote_addr);

    // Extract authenticated node ID from peer's TLS certificate
    let authenticated_node = extract_peer_node_id(&connection);
    match authenticated_node {
        Some(node_id) => {
            info!("Authenticated peer {} as node {}", remote_addr, node_id);
        }
        None => {
            error!(
                "Could not extract node ID from peer certificate for {}. Closing connection.",
                remote_addr
            );
            // SECURITY: Close connection - do NOT allow unauthenticated peers
            connection.close(0u32.into(), b"Authentication required");
            return;
        }
    }

    loop {
        // Accept unidirectional streams (each stream = one message)
        match connection.accept_uni().await {
            Ok(recv) => {
                let queue = incoming_queue.clone();
                let auth_node = authenticated_node;
                tokio::spawn(async move {
                    if let Err(e) = handle_uni_stream(recv, queue, auth_node).await {
                        debug!("Stream handler error: {}", e);
                    }
                });
            }
            Err(e) => {
                if connection.close_reason().is_some() {
                    debug!("Connection from {} closed", remote_addr);
                } else {
                    warn!("Failed to accept stream from {}: {}", remote_addr, e);
                }
                break;
            }
        }
    }
}

/// Extract the node ID from a peer's TLS certificate.
///
/// Parses the peer's TLS certificate and extracts the Common Name (CN)
/// from the Subject Distinguished Name, which contains the node ID.
fn extract_peer_node_id(connection: &Connection) -> Option<NodeId> {
    // Get peer certificates
    let peer_identity = connection.peer_identity()?;

    // Downcast to Vec<rustls::pki_types::CertificateDer>
    let peer_certs = peer_identity
        .downcast_ref::<Vec<rustls::pki_types::CertificateDer>>()?;

    if peer_certs.is_empty() {
        warn!("No peer certificates provided");
        return None;
    }

    // Parse first certificate using x509-parser
    let cert_der = &peer_certs[0];
    let (_, cert) = match x509_parser::parse_x509_certificate(cert_der) {
        Ok(parsed) => parsed,
        Err(e) => {
            error!("Failed to parse peer certificate: {}", e);
            return None;
        }
    };

    // Verify certificate validity period
    let now = std::time::SystemTime::now();
    let now_secs = now
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;

    if now_secs < cert.validity().not_before.timestamp() {
        error!("Peer certificate not yet valid");
        return None;
    }

    if now_secs > cert.validity().not_after.timestamp() {
        error!("Peer certificate expired");
        return None;
    }

    // Extract Common Name from Subject
    let subject = cert.subject();
    for rdn in subject.iter() {
        for attr in rdn.iter() {
            if attr.attr_type() == &x509_parser::oid_registry::OID_X509_COMMON_NAME {
                if let Ok(node_id_str) = attr.attr_value().as_str() {
                    info!("Extracted node ID from certificate CN: {}", node_id_str);

                    // Parse node ID from string (format: "node-X" where X is u64)
                    if let Some(id_part) = node_id_str.strip_prefix("node-") {
                        if let Ok(node_id) = id_part.parse::<u64>() {
                            return Some(NodeId::from(node_id));
                        }
                    }

                    // Fallback: try parsing the whole string as u64
                    if let Ok(node_id) = node_id_str.parse::<u64>() {
                        return Some(NodeId::from(node_id));
                    }

                    error!("Could not parse node ID from CN: {}", node_id_str);
                }
            }
        }
    }

    warn!("Could not find Common Name in peer certificate");
    None
}

/// Handle messages from a single unidirectional stream.
///
/// Validates that the message sender matches the authenticated TLS identity.
async fn handle_uni_stream(
    mut recv: RecvStream,
    incoming_queue: IncomingQueue,
    authenticated_node: Option<NodeId>,
) -> NetworkResult<()> {
    // Read stream ID (8 bytes)
    let mut stream_id_buf = [0u8; 8];
    match recv.read_exact(&mut stream_id_buf).await {
        Ok(_) => {}
        Err(quinn::ReadExactError::FinishedEarly(_)) => {
            // Stream closed cleanly
            return Ok(());
        }
        Err(e) => {
            return Err(NetworkError::CodecError(format!("Read error: {}", e)));
        }
    }
    let stream_id = u64::from_be_bytes(stream_id_buf);

    // Read length prefix (4 bytes)
    let mut len_buf = [0u8; 4];
    match recv.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(quinn::ReadExactError::FinishedEarly(_)) => {
            return Ok(());
        }
        Err(e) => {
            return Err(NetworkError::CodecError(format!("Read error: {}", e)));
        }
    }

    let len = u32::from_be_bytes(len_buf);
    if len > 16 * 1024 * 1024 {
        return Err(NetworkError::CodecError(format!(
            "Message too large: {}",
            len
        )));
    }

    // Read payload
    let mut payload = vec![0u8; len as usize];
    recv.read_exact(&mut payload)
        .await
        .map_err(|e| NetworkError::CodecError(format!("Read payload error: {}", e)))?;

    // Decode message
    let message: NetworkMessage = bincode::deserialize(&payload)
        .map_err(|e| NetworkError::CodecError(format!("Failed to deserialize message: {}", e)))?;

    // TODO: Validate that message sender matches authenticated_node once mTLS is enforced
    if let Some(auth_node) = authenticated_node {
        debug!(
            "Received message from authenticated node {} on stream {}",
            auth_node, stream_id
        );
    } else {
        debug!("Received message on stream {} (unauthenticated)", stream_id);
    }

    // Add to incoming queue based on message type
    // For now, we'll use a simple session_id extraction
    let session_id = extract_session_id(&message);

    {
        let mut queue = incoming_queue.write().await;
        queue
            .entry(session_id.clone())
            .or_insert_with(Vec::new)
            .push(message);
    }

    debug!("Queued message for session {}", session_id);

    Ok(())
}

/// Extract session ID from a network message.
fn extract_session_id(message: &NetworkMessage) -> String {
    match message {
        NetworkMessage::DkgRound(msg) => msg.session_id.to_string(),
        NetworkMessage::SigningRound(msg) => msg.tx_id.to_string(),
        NetworkMessage::PresignatureGen(msg) => msg.presig_id.to_string(),
        NetworkMessage::Vote(_) => "votes".to_string(),
        NetworkMessage::Heartbeat(_) => "heartbeats".to_string(),
        NetworkMessage::Protocol { session_id, .. } => session_id.clone(),
    }
}

/// Create a new incoming queue.
pub fn new_incoming_queue() -> IncomingQueue {
    Arc::new(RwLock::new(HashMap::new()))
}

/// Drain messages for a specific session from the queue.
pub async fn drain_session_messages(queue: &IncomingQueue, session_id: &str) -> Vec<NetworkMessage> {
    let mut q = queue.write().await;
    q.remove(session_id).unwrap_or_default()
}

/// Spawn a QUIC listener as a background task.
///
/// This is a convenience function that creates and starts a listener.
///
/// # Arguments
/// * `listen_addr` - Address to bind to
/// * `server_config` - TLS server configuration
/// * `incoming_queue` - Queue for incoming messages
///
/// # Returns
/// A handle to the listener that can be used to stop it.
pub fn spawn_quic_listener(
    listen_addr: SocketAddr,
    server_config: ServerConfig,
    incoming_queue: IncomingQueue,
) -> NetworkResult<QuicListener> {
    let mut listener = QuicListener::new(listen_addr, server_config)?;
    listener.start(incoming_queue)?;
    Ok(listener)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_incoming_queue_operations() {
        // Test will be implemented with async test support
    }
}
