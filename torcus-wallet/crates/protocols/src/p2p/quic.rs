//! QUIC connection management for secure P2P messaging.
//!
//! Uses QUIC with TLS 1.3 and CA-signed certificates for secure,
//! authenticated peer-to-peer communication between MPC nodes.
//!
//! ## Security Model
//!
//! - All connections use TLS 1.3 (mandatory in QUIC)
//! - Mutual TLS: both client and server present certificates
//! - Certificates are signed by a shared CA
//! - Each node's certificate contains its party index
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    QuicConnectionManager                     │
//! │  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐   │
//! │  │   Endpoint   │───>│  Connection  │───>│   SendStream │   │
//! │  │   (client)   │    │    Pool      │    │   (per-msg)  │   │
//! │  └──────────────┘    └──────────────┘    └──────────────┘   │
//! │                                                              │
//! │  ┌──────────────┐                                           │
//! │  │   Endpoint   │<── Incoming connections from peers        │
//! │  │   (server)   │                                           │
//! │  └──────────────┘                                           │
//! └─────────────────────────────────────────────────────────────┘
//! ```

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use quinn::{ClientConfig, Connection, Endpoint, RecvStream, ServerConfig};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::certs::NodeCertificate;
use super::error::P2pError;
use super::protocol::P2pMessage;

/// Default QUIC port (different from TCP P2P port to allow both).
pub const DEFAULT_QUIC_PORT: u16 = 4001;

/// Connection timeout.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Idle timeout for connections.
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// Keep-alive interval.
const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(15);

/// QUIC connection manager for secure P2P messaging.
pub struct QuicConnectionManager {
    /// Our party index.
    party_index: u16,
    /// QUIC port used by all nodes.
    quic_port: u16,
    /// Our node certificate for TLS.
    node_cert: Arc<NodeCertificate>,
    /// Client endpoint for outgoing connections.
    client_endpoint: Option<Endpoint>,
    /// Active connections: party_index -> connection.
    connections: RwLock<HashMap<u16, Connection>>,
    /// Known peers: party_index -> (hostname, quic_port).
    peers: RwLock<HashMap<u16, (String, u16)>>,
    /// Registry URL for fetching peer list.
    registry_url: String,
}

impl QuicConnectionManager {
    /// Create a new QUIC connection manager.
    pub fn new(
        party_index: u16,
        registry_url: String,
        quic_port: u16,
        node_cert: NodeCertificate,
    ) -> Result<Self, P2pError> {
        let node_cert = Arc::new(node_cert);

        Ok(Self {
            party_index,
            quic_port,
            node_cert,
            client_endpoint: None,
            connections: RwLock::new(HashMap::new()),
            peers: RwLock::new(HashMap::new()),
            registry_url,
        })
    }

    /// Initialize the client endpoint for outgoing connections.
    pub async fn init_client(&mut self) -> Result<(), P2pError> {
        let rustls_config = self.node_cert.client_config()?;

        // Create QUIC client config
        let mut config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(rustls_config).map_err(|e| {
                P2pError::CodecError(format!("Failed to create QUIC client config: {}", e))
            })?,
        ));

        // Configure transport
        let mut transport = quinn::TransportConfig::default();
        transport.max_idle_timeout(Some(IDLE_TIMEOUT.try_into().unwrap()));
        transport.keep_alive_interval(Some(KEEP_ALIVE_INTERVAL));
        config.transport_config(Arc::new(transport));

        // Create endpoint bound to any address
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).map_err(|e| {
            P2pError::CodecError(format!("Failed to create client endpoint: {}", e))
        })?;

        endpoint.set_default_client_config(config);

        self.client_endpoint = Some(endpoint);
        info!(
            "QUIC client endpoint initialized for party {}",
            self.party_index
        );

        Ok(())
    }

    /// Create server config for incoming connections.
    pub fn server_config(&self) -> Result<ServerConfig, P2pError> {
        let rustls_config = self.node_cert.server_config()?;

        let mut config = ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(rustls_config).map_err(|e| {
                P2pError::CodecError(format!("Failed to create QUIC server config: {}", e))
            })?,
        ));

        // Configure transport
        let mut transport = quinn::TransportConfig::default();
        transport.max_idle_timeout(Some(IDLE_TIMEOUT.try_into().unwrap()));
        config.transport_config(Arc::new(transport));

        Ok(config)
    }

    /// Update the peer list from the registry.
    pub async fn refresh_peers(&self) -> Result<(), P2pError> {
        let url = format!("{}/registry/nodes", self.registry_url);
        debug!("Fetching peer list from {}", url);

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| P2pError::RegistryError(e.to_string()))?;

        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| P2pError::RegistryError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(P2pError::RegistryError(format!(
                "registry returned status {}",
                response.status()
            )));
        }

        let list: common::ListNodesResponse = response
            .json()
            .await
            .map_err(|e| P2pError::RegistryError(e.to_string()))?;

        let mut peers = self.peers.write().await;
        peers.clear();

        for node in list.nodes {
            if node.party_index == self.party_index {
                continue; // Skip self
            }

            if !node.is_online {
                debug!("Skipping offline node {}", node.party_index);
                continue;
            }

            // Extract hostname from HTTP endpoint
            let hostname = extract_hostname(&node.endpoint);
            peers.insert(node.party_index, (hostname, self.quic_port));
        }

        info!(
            "Updated QUIC peer list: {} peers (excluding self)",
            peers.len()
        );

        Ok(())
    }

    /// Get connection to a peer, creating one if necessary.
    async fn get_connection(&self, party_index: u16) -> Result<Connection, P2pError> {
        // Check if we already have a connection
        {
            let connections = self.connections.read().await;
            if let Some(conn) = connections.get(&party_index) {
                // Check if connection is still alive
                if conn.close_reason().is_none() {
                    return Ok(conn.clone());
                }
                // Connection is closed, will reconnect
                debug!(
                    "Existing connection to peer {} is closed, reconnecting",
                    party_index
                );
            }
        }

        // Get peer address
        let (hostname, port) = {
            let peers = self.peers.read().await;
            peers
                .get(&party_index)
                .cloned()
                .ok_or(P2pError::PeerNotFound { party_index })?
        };

        let addr_str = format!("{}:{}", hostname, port);
        debug!("Connecting to peer {} at {}", party_index, addr_str);

        // Resolve address
        let addr: SocketAddr = tokio::net::lookup_host(&addr_str)
            .await
            .map_err(|e| P2pError::InvalidAddress {
                party_index,
                address: format!("{}: {}", addr_str, e),
            })?
            .next()
            .ok_or(P2pError::InvalidAddress {
                party_index,
                address: format!("{}: no addresses found", addr_str),
            })?;

        // Get client endpoint
        let endpoint = self
            .client_endpoint
            .as_ref()
            .ok_or_else(|| P2pError::CodecError("Client endpoint not initialized".to_string()))?;

        // Connect with timeout
        let connection = tokio::time::timeout(
            CONNECT_TIMEOUT,
            endpoint.connect(addr, &hostname).map_err(|e| {
                P2pError::CodecError(format!("Failed to initiate connection: {}", e))
            })?,
        )
        .await
        .map_err(|_| P2pError::Timeout(format!("connecting to peer {}", party_index)))?
        .map_err(|e| P2pError::ConnectionLost {
            party_index,
            reason: e.to_string(),
        })?;

        info!(
            "QUIC connection established to peer {} at {}",
            party_index, addr_str
        );

        // Store connection
        {
            let mut connections = self.connections.write().await;
            connections.insert(party_index, connection.clone());
        }

        Ok(connection)
    }

    /// Send a message to a specific peer.
    ///
    /// Opens a new unidirectional stream for each message.
    pub async fn send(&self, party_index: u16, msg: &P2pMessage) -> Result<(), P2pError> {
        let connection = self.get_connection(party_index).await?;
        let encoded = msg.encode()?;

        // Open a new unidirectional stream for this message
        let mut send_stream =
            connection
                .open_uni()
                .await
                .map_err(|e| P2pError::ConnectionLost {
                    party_index,
                    reason: format!("Failed to open stream: {}", e),
                })?;

        // Write the message
        send_stream
            .write_all(&encoded)
            .await
            .map_err(|e| P2pError::ConnectionLost {
                party_index,
                reason: format!("Write failed: {}", e),
            })?;

        // Finish the stream to signal end of message
        send_stream.finish().map_err(|e| P2pError::ConnectionLost {
            party_index,
            reason: format!("Finish failed: {}", e),
        })?;

        debug!(
            "Sent QUIC message to peer {}: session={}, round={}, seq={}",
            party_index, msg.session_id, msg.round, msg.seq
        );

        Ok(())
    }

    /// Broadcast a message to all peers.
    pub async fn broadcast(&self, msg: &P2pMessage, exclude: Option<u16>) -> Result<(), P2pError> {
        let peer_indices: Vec<u16> = {
            let peers = self.peers.read().await;
            peers.keys().copied().collect()
        };

        let mut errors = Vec::new();

        for party_index in peer_indices {
            if party_index == self.party_index {
                continue;
            }
            if exclude == Some(party_index) {
                continue;
            }

            if let Err(e) = self.send(party_index, msg).await {
                warn!("Failed to send QUIC message to peer {}: {}", party_index, e);
                errors.push((party_index, e));
            }
        }

        let total_peers = self.peers.read().await.len();
        if !errors.is_empty() && errors.len() >= total_peers {
            return Err(P2pError::ConnectionLost {
                party_index: 0,
                reason: "all broadcast sends failed".to_string(),
            });
        }

        Ok(())
    }

    /// Remove a connection (e.g., on error).
    pub async fn remove_connection(&self, party_index: u16) {
        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.remove(&party_index) {
            conn.close(0u32.into(), b"connection removed");
            debug!("Removed QUIC connection to peer {}", party_index);
        }
    }

    /// Close all connections gracefully.
    pub async fn close_all(&self) {
        let mut connections = self.connections.write().await;
        for (party_index, conn) in connections.drain() {
            conn.close(0u32.into(), b"shutdown");
            debug!("Closed QUIC connection to peer {}", party_index);
        }
    }

    /// Get the list of known peer indices.
    #[allow(dead_code)]
    pub async fn peer_indices(&self) -> Vec<u16> {
        let peers = self.peers.read().await;
        peers.keys().copied().collect()
    }

    /// Manually add a peer (for testing or static configuration).
    #[allow(dead_code)]
    pub async fn add_peer(&self, party_index: u16, hostname: String, port: u16) {
        let mut peers = self.peers.write().await;
        peers.insert(party_index, (hostname, port));
    }
}

/// Extract hostname from HTTP endpoint (e.g., "http://mpc-node-1:3000" -> "mpc-node-1").
fn extract_hostname(endpoint: &str) -> String {
    let url = endpoint.trim_start_matches("http://");
    let url = url.trim_start_matches("https://");

    if let Some(idx) = url.find(':') {
        url[..idx].to_string()
    } else {
        url.to_string()
    }
}

/// Handle an incoming QUIC connection.
///
/// This function processes incoming streams and extracts P2P messages.
/// It validates that message senders match the authenticated TLS identity.
pub async fn handle_incoming_connection(
    connection: Connection,
    incoming_queue: super::receiver::IncomingQueue,
) {
    let remote_addr = connection.remote_address();
    info!("Handling incoming QUIC connection from {}", remote_addr);

    // Extract authenticated party index from peer's TLS certificate
    let authenticated_party = extract_peer_party_index(&connection);
    match authenticated_party {
        Some(party) => {
            info!("Authenticated peer {} as party {}", remote_addr, party);
        }
        None => {
            warn!(
                "Could not extract party index from peer certificate for {}",
                remote_addr
            );
            // Close connection - we cannot trust messages without authentication
            connection.close(1u32.into(), b"invalid peer certificate");
            return;
        }
    }

    let authenticated_party = authenticated_party.unwrap();

    loop {
        // Accept unidirectional streams (each stream = one message)
        match connection.accept_uni().await {
            Ok(recv) => {
                let queue = incoming_queue.clone();
                let auth_party = authenticated_party;
                tokio::spawn(async move {
                    if let Err(e) = handle_uni_stream(recv, queue, auth_party).await {
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

/// Extract the party index from a peer's TLS certificate.
///
/// The party index is stored in the OrganizationalUnitName field
/// of the certificate subject as "party-{index}".
fn extract_peer_party_index(connection: &Connection) -> Option<u16> {
    use rustls::pki_types::CertificateDer;

    // Get peer identity (certificate chain)
    let peer_identity = connection.peer_identity()?;

    // Downcast to certificate chain (Vec<CertificateDer>)
    let certs: &Vec<CertificateDer<'_>> = peer_identity.downcast_ref()?;

    // Get the first certificate (peer's certificate, not CA)
    let peer_cert = certs.first()?;

    // Extract party index from certificate
    super::certs::extract_party_index_from_cert(peer_cert.as_ref())
}

/// Handle messages from a single unidirectional stream.
///
/// Validates that the message sender matches the authenticated TLS identity.
async fn handle_uni_stream(
    mut recv: RecvStream,
    incoming_queue: super::receiver::IncomingQueue,
    authenticated_party: u16,
) -> Result<(), P2pError> {
    // Read length prefix
    let mut len_buf = [0u8; 4];
    match recv.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(quinn::ReadExactError::FinishedEarly(_)) => {
            // Stream closed cleanly
            return Ok(());
        }
        Err(e) => {
            return Err(P2pError::CodecError(format!("Read error: {}", e)));
        }
    }

    let len = u32::from_be_bytes(len_buf);
    if len > 16 * 1024 * 1024 {
        return Err(P2pError::CodecError(format!("Message too large: {}", len)));
    }

    // Read payload
    let mut payload = vec![0u8; len as usize];
    recv.read_exact(&mut payload)
        .await
        .map_err(|e| P2pError::CodecError(format!("Read payload error: {}", e)))?;

    // Decode message
    let msg = P2pMessage::decode(&payload)?;

    // SECURITY: Validate that msg.sender matches the authenticated TLS identity.
    // This prevents a valid peer from spoofing messages as another party.
    if msg.sender != authenticated_party {
        warn!(
            "Message sender spoofing detected: msg.sender={} but TLS authenticated as party {}. Dropping message.",
            msg.sender, authenticated_party
        );
        return Err(P2pError::CodecError(format!(
            "Sender mismatch: claimed {} but authenticated as {}",
            msg.sender, authenticated_party
        )));
    }

    debug!(
        "Received QUIC message: session={}, sender={} (verified), round={}, seq={}",
        msg.session_id, msg.sender, msg.round, msg.seq
    );

    // Convert to RelayMessage and queue
    let relay_msg: crate::relay::RelayMessage = msg.into();

    // Add to incoming queue
    {
        let mut queue = incoming_queue.write().await;
        queue
            .entry(relay_msg.session_id.clone())
            .or_insert_with(Vec::new)
            .push(relay_msg);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_hostname() {
        assert_eq!(extract_hostname("http://mpc-node-1:3000"), "mpc-node-1");
        assert_eq!(extract_hostname("https://localhost:8080"), "localhost");
        assert_eq!(extract_hostname("http://192.168.1.1:3000"), "192.168.1.1");
        assert_eq!(extract_hostname("myhost"), "myhost");
    }
}
