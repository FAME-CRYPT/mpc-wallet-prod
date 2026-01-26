//! QUIC connection engine with connection pooling and mTLS support.
//!
//! This module provides the core QUIC networking layer with:
//! - Connection pooling (max 50 concurrent connections per peer)
//! - mTLS certificate-based authentication
//! - Stream ID conventions for different message types
//! - Automatic reconnection and connection management

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use quinn::{ClientConfig, Connection, Endpoint, ServerConfig};
use threshold_security::{CertificateManager, create_mesh_configs};
use threshold_types::{NetworkMessage, NodeId};

use crate::error::{NetworkError, NetworkResult};
use crate::peer_registry::PeerRegistry;

/// Maximum concurrent connections per peer.
const MAX_CONNECTIONS_PER_PEER: usize = 50;

/// Connection timeout.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Idle timeout for connections.
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// Keep-alive interval.
const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(15);

/// QUIC connection pool for a single peer.
struct ConnectionPool {
    /// Active connections to this peer.
    connections: Vec<Connection>,
    /// Maximum number of connections allowed.
    max_connections: usize,
}

impl ConnectionPool {
    fn new(max_connections: usize) -> Self {
        Self {
            connections: Vec::new(),
            max_connections,
        }
    }

    /// Get an available connection, removing closed ones.
    fn get_connection(&mut self) -> Option<Connection> {
        // Remove closed connections
        self.connections.retain(|conn| conn.close_reason().is_none());

        // Return the first available connection
        self.connections.first().cloned()
    }

    /// Add a new connection to the pool.
    fn add_connection(&mut self, conn: Connection) -> NetworkResult<()> {
        // Remove closed connections first
        self.connections.retain(|c| c.close_reason().is_none());

        if self.connections.len() >= self.max_connections {
            return Err(NetworkError::PoolExhausted {
                node_id: NodeId(0), // Will be filled by caller
            });
        }

        self.connections.push(conn);
        Ok(())
    }

    /// Close all connections in the pool.
    fn close_all(&mut self) {
        for conn in self.connections.drain(..) {
            conn.close(0u32.into(), b"pool shutdown");
        }
    }
}

/// QUIC connection engine with mTLS support.
pub struct QuicEngine {
    /// Our node ID.
    local_node_id: NodeId,
    /// Peer registry for peer discovery.
    peer_registry: Arc<PeerRegistry>,
    /// Client endpoint for outgoing connections.
    client_endpoint: Option<Endpoint>,
    /// Server endpoint for incoming connections.
    server_endpoint: Arc<RwLock<Option<Endpoint>>>,
    /// Connection pools: node_id -> ConnectionPool.
    connection_pools: Arc<RwLock<HashMap<NodeId, ConnectionPool>>>,
    /// mTLS configuration placeholder.
    /// TODO: This will be filled by cert_manager integration.
    mtls_config: Option<MtlsConfig>,
}

/// mTLS configuration for QUIC connections.
///
/// This struct holds the certificate manager and pre-built QUIC configurations
/// for both client and server sides.
pub struct MtlsConfig {
    /// Certificate manager for loading and validating certificates
    pub cert_manager: CertificateManager,
    /// Pre-built QUIC client configuration
    pub client_config: ClientConfig,
    /// Pre-built QUIC server configuration
    pub server_config: ServerConfig,
}

impl MtlsConfig {
    /// Create a new mTLS configuration from certificate paths.
    ///
    /// # Arguments
    /// * `ca_cert_path` - Path to CA certificate
    /// * `node_cert_path` - Path to node certificate
    /// * `node_key_path` - Path to node private key
    /// * `skip_hostname_verification` - Whether to skip hostname verification (useful for mesh networks)
    ///
    /// # Errors
    /// Returns an error if certificates cannot be loaded or configs cannot be created.
    pub fn new(
        ca_cert_path: &str,
        node_cert_path: &str,
        node_key_path: &str,
        skip_hostname_verification: bool,
    ) -> NetworkResult<Self> {
        info!("Creating mTLS configuration");
        debug!("CA cert: {}", ca_cert_path);
        debug!("Node cert: {}", node_cert_path);
        debug!("Node key: {}", node_key_path);

        let cert_manager = CertificateManager::new(ca_cert_path, node_cert_path, node_key_path);

        // Verify certificates can be loaded
        let _ = cert_manager.load_certificates()
            .map_err(|e| NetworkError::CertificateError(e.to_string()))?;

        // Create QUIC configs
        let (client_config, server_config) = create_mesh_configs(
            ca_cert_path,
            node_cert_path,
            node_key_path,
            skip_hostname_verification,
        ).map_err(|e| NetworkError::TlsConfigError(e.to_string()))?;

        info!("mTLS configuration created successfully");

        Ok(Self {
            cert_manager,
            client_config,
            server_config,
        })
    }

    /// Extract node ID from a certificate.
    ///
    /// # Arguments
    /// * `cert_der` - The certificate in DER format
    ///
    /// # Errors
    /// Returns an error if the node ID cannot be extracted.
    pub fn extract_node_id(&self, cert_der: &rustls::pki_types::CertificateDer) -> NetworkResult<NodeId> {
        self.cert_manager
            .extract_node_id(cert_der)
            .map(NodeId)
            .map_err(|e| NetworkError::CertificateError(e.to_string()))
    }

    /// Check if certificate is about to expire (within 30 days).
    pub fn check_expiry(&self) -> NetworkResult<bool> {
        self.cert_manager
            .verify_certificate_expiry()
            .map_err(|e| NetworkError::CertificateError(e.to_string()))
    }
}

impl QuicEngine {
    /// Create a new QUIC engine.
    ///
    /// # Arguments
    /// * `local_node_id` - Our node ID
    /// * `peer_registry` - Shared peer registry
    /// * `mtls_config` - Optional mTLS configuration (required for production use)
    pub fn new(
        local_node_id: NodeId,
        peer_registry: Arc<PeerRegistry>,
        mtls_config: Option<MtlsConfig>,
    ) -> Self {
        Self {
            local_node_id,
            peer_registry,
            client_endpoint: None,
            server_endpoint: Arc::new(RwLock::new(None)),
            connection_pools: Arc::new(RwLock::new(HashMap::new())),
            mtls_config,
        }
    }

    /// Create a new QUIC engine with mTLS certificates.
    ///
    /// This is a convenience constructor that creates the mTLS config from certificate paths.
    ///
    /// # Arguments
    /// * `local_node_id` - Our node ID
    /// * `peer_registry` - Shared peer registry
    /// * `ca_cert_path` - Path to CA certificate
    /// * `node_cert_path` - Path to node certificate
    /// * `node_key_path` - Path to node private key
    /// * `skip_hostname_verification` - Whether to skip hostname verification (useful for mesh networks)
    ///
    /// # Errors
    /// Returns an error if certificates cannot be loaded or configs cannot be created.
    pub fn with_certificates(
        local_node_id: NodeId,
        peer_registry: Arc<PeerRegistry>,
        ca_cert_path: &str,
        node_cert_path: &str,
        node_key_path: &str,
        skip_hostname_verification: bool,
    ) -> NetworkResult<Self> {
        let mtls_config = MtlsConfig::new(
            ca_cert_path,
            node_cert_path,
            node_key_path,
            skip_hostname_verification,
        )?;

        Ok(Self::new(local_node_id, peer_registry, Some(mtls_config)))
    }

    /// Initialize the client endpoint for outgoing connections.
    pub async fn init_client(&mut self) -> NetworkResult<()> {
        let client_config = if let Some(ref mtls) = self.mtls_config {
            // Use provided mTLS configuration
            info!("Using mTLS certificates for QUIC client");
            mtls.client_config.clone()
        } else {
            // TEMPORARY: Use insecure config for development/testing only
            // This will be removed once cert_manager integration is complete
            warn!("INSECURE: Using self-signed certificates for QUIC (development only)");
            Self::create_temporary_client_config()?
        };

        // Create endpoint bound to any address
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).map_err(|e| {
            NetworkError::TlsConfigError(format!("Failed to create client endpoint: {}", e))
        })?;

        endpoint.set_default_client_config(client_config);

        self.client_endpoint = Some(endpoint);
        info!("QUIC client endpoint initialized for node {}", self.local_node_id);

        Ok(())
    }

    /// Initialize the server endpoint for incoming connections.
    ///
    /// # Arguments
    /// * `bind_addr` - Address to bind the server to (e.g., "0.0.0.0:5000")
    ///
    /// # Errors
    /// Returns an error if the server cannot be started.
    pub async fn init_server(&self, bind_addr: &str) -> NetworkResult<()> {
        let server_config = self.server_config()?;

        let socket_addr: std::net::SocketAddr = bind_addr.parse()
            .map_err(|e| NetworkError::TlsConfigError(format!("Invalid bind address: {}", e)))?;

        let endpoint = Endpoint::server(server_config, socket_addr)
            .map_err(|e| NetworkError::TlsConfigError(format!("Failed to create server endpoint: {}", e)))?;

        *self.server_endpoint.write().await = Some(endpoint);
        info!("QUIC server endpoint initialized for node {} on {}", self.local_node_id, bind_addr);

        Ok(())
    }

    /// Start listening for incoming QUIC connections and messages.
    ///
    /// This spawns a background task that continuously accepts connections
    /// and processes incoming messages. Messages are dispatched via the
    /// provided callback function.
    ///
    /// # Arguments
    /// * `message_handler` - Callback function to handle incoming messages
    ///
    /// # Returns
    /// A join handle for the listener task.
    pub fn start_listener<F>(&self, message_handler: F) -> tokio::task::JoinHandle<()>
    where
        F: Fn(NodeId, NetworkMessage) -> () + Send + Sync + 'static,
    {
        let server_endpoint = Arc::clone(&self.server_endpoint);
        let local_node_id = self.local_node_id;
        let handler = Arc::new(message_handler);

        tokio::spawn(async move {
            info!("QUIC listener started for node {}", local_node_id);

            loop {
                let endpoint_guard = server_endpoint.read().await;
                let endpoint = match endpoint_guard.as_ref() {
                    Some(ep) => ep,
                    None => {
                        warn!("Server endpoint not initialized, listener exiting");
                        break;
                    }
                };

                // Accept incoming connection
                let connecting = match endpoint.accept().await {
                    Some(conn) => conn,
                    None => {
                        info!("Server endpoint closed, listener exiting");
                        break;
                    }
                };

                // Drop the read guard before awaiting
                drop(endpoint_guard);

                let handler_clone = Arc::clone(&handler);
                // Spawn task to handle this connection
                tokio::spawn(async move {
                    match connecting.await {
                        Ok(connection) => {
                            debug!("Accepted connection from {}", connection.remote_address());
                            Self::handle_connection(connection, local_node_id, handler_clone).await;
                        }
                        Err(e) => {
                            warn!("Failed to establish connection: {}", e);
                        }
                    }
                });
            }

            info!("QUIC listener stopped for node {}", local_node_id);
        })
    }

    /// Handle an incoming QUIC connection.
    async fn handle_connection<F>(
        connection: Connection,
        local_node_id: NodeId,
        message_handler: Arc<F>,
    )
    where
        F: Fn(NodeId, NetworkMessage) -> () + Send + Sync + 'static,
    {
        loop {
            // Accept incoming unidirectional stream (matches open_uni on sender side)
            let mut recv = match connection.accept_uni().await {
                Ok(stream) => stream,
                Err(quinn::ConnectionError::ApplicationClosed { .. }) => {
                    debug!("Connection closed by peer");
                    break;
                }
                Err(e) => {
                    warn!("Failed to accept stream: {}", e);
                    break;
                }
            };

            // Spawn task to handle this stream
            let handler_clone = Arc::clone(&message_handler);
            tokio::spawn(async move {
                info!("📥 QuicEngine: Receiving message on unidirectional stream");

                // Read stream ID (8 bytes) + message length (4 bytes) + message
                let mut header = vec![0u8; 12];
                match recv.read_exact(&mut header).await {
                    Ok(_) => {},
                    Err(e) => {
                        warn!("Failed to read stream header: {}", e);
                        return;
                    }
                }

                let _stream_id = u64::from_be_bytes(header[0..8].try_into().unwrap());
                let msg_len = u32::from_be_bytes(header[8..12].try_into().unwrap()) as usize;

                // Read message payload
                let mut buffer = vec![0u8; msg_len];
                match recv.read_exact(&mut buffer).await {
                    Ok(_) => {},
                    Err(e) => {
                        warn!("Failed to read message payload: {}", e);
                        return;
                    }
                }

                info!("📥 QuicEngine: Read {} bytes from stream", buffer.len());

                // Deserialize NetworkMessage
                let message: NetworkMessage = match bincode::deserialize(&buffer) {
                    Ok(msg) => msg,
                    Err(e) => {
                        warn!("Failed to deserialize message: {}", e);
                        return;
                    }
                };

                // Extract sender NodeId from message
                let sender_node_id = match &message {
                    NetworkMessage::Protocol { from, .. } => *from,
                    NetworkMessage::DkgRound(msg) => msg.from,
                    _ => {
                        debug!("Ignoring non-protocol message");
                        return;
                    }
                };

                info!("📥 QuicEngine: Dispatching message from {}", sender_node_id);

                // Dispatch message via callback
                handler_clone(sender_node_id, message);
            });
        }
    }

    /// Create server configuration for incoming connections.
    pub fn server_config(&self) -> NetworkResult<ServerConfig> {
        if let Some(ref mtls) = self.mtls_config {
            // Use provided mTLS configuration
            info!("Using mTLS certificates for QUIC server");
            Ok(mtls.server_config.clone())
        } else {
            // TEMPORARY: Use insecure config for development/testing only
            warn!("INSECURE: Using self-signed certificates for QUIC (development only)");
            Self::create_temporary_server_config()
        }
    }

    /// Get or create a connection to a peer.
    async fn get_or_create_connection(&self, node_id: &NodeId) -> NetworkResult<Connection> {
        // Try to get existing connection from pool
        {
            let mut pools = self.connection_pools.write().await;
            let pool = pools
                .entry(*node_id)
                .or_insert_with(|| ConnectionPool::new(MAX_CONNECTIONS_PER_PEER));

            if let Some(conn) = pool.get_connection() {
                return Ok(conn);
            }
        }

        // No available connection, create a new one
        self.create_connection(node_id).await
    }

    /// Create a new connection to a peer.
    async fn create_connection(&self, node_id: &NodeId) -> NetworkResult<Connection> {
        let peer_info = self.peer_registry.get_peer(node_id).await?;
        let addr = self.peer_registry.peer_address(node_id).await?;

        debug!("Connecting to peer {} at {}", node_id, addr);

        // Get client endpoint
        let endpoint = self.client_endpoint.as_ref().ok_or_else(|| {
            NetworkError::TlsConfigError("Client endpoint not initialized".to_string())
        })?;

        // Connect with timeout
        let connection = tokio::time::timeout(
            CONNECT_TIMEOUT,
            endpoint.connect(addr, &peer_info.hostname).map_err(|e| {
                NetworkError::ConnectionFailed {
                    node_id: *node_id,
                    source: std::io::Error::other(e.to_string()),
                }
            })?,
        )
        .await
        .map_err(|_| NetworkError::Timeout(format!("connecting to peer {}", node_id)))?
        .map_err(|e| NetworkError::ConnectionLost {
            node_id: *node_id,
            reason: e.to_string(),
        })?;

        info!("QUIC connection established to peer {}", node_id);

        // Add to pool
        {
            let mut pools = self.connection_pools.write().await;
            let pool = pools
                .entry(*node_id)
                .or_insert_with(|| ConnectionPool::new(MAX_CONNECTIONS_PER_PEER));

            pool.add_connection(connection.clone()).map_err(|_| {
                NetworkError::PoolExhausted {
                    node_id: *node_id,
                }
            })?;
        }

        Ok(connection)
    }

    /// Send a message to a specific peer.
    ///
    /// # Arguments
    /// * `node_id` - Target node ID
    /// * `message` - Network message to send
    /// * `stream_id` - Stream ID to use (must follow conventions)
    pub async fn send(&self, node_id: &NodeId, message: &NetworkMessage, stream_id: u64) -> NetworkResult<()> {
        let connection = self.get_or_create_connection(node_id).await?;
        // Use JSON instead of bincode for NetworkMessage to support serde's deserialize_any
        let encoded = serde_json::to_vec(message).map_err(|e| {
            NetworkError::CodecError(format!("Failed to serialize message: {}", e))
        })?;

        // Open a unidirectional stream with the specified stream ID
        // Note: Quinn doesn't support explicit stream IDs, so we encode it in the message
        let mut send_stream = connection.open_uni().await.map_err(|e| {
            NetworkError::ConnectionLost {
                node_id: *node_id,
                reason: format!("Failed to open stream: {}", e),
            }
        })?;

        // Write stream ID (8 bytes) + message length (4 bytes) + message
        let mut payload = Vec::with_capacity(8 + 4 + encoded.len());
        payload.extend_from_slice(&stream_id.to_be_bytes());
        payload.extend_from_slice(&(encoded.len() as u32).to_be_bytes());
        payload.extend_from_slice(&encoded);

        send_stream.write_all(&payload).await.map_err(|e| {
            NetworkError::SendFailed {
                node_id: *node_id,
                source: e.into(),
            }
        })?;

        send_stream.finish().map_err(|e| {
            NetworkError::SendFailed {
                node_id: *node_id,
                source: std::io::Error::other(e.to_string()),
            }
        })?;

        debug!("Sent message to peer {} on stream {}", node_id, stream_id);
        Ok(())
    }

    /// Broadcast a message to all known peers.
    pub async fn broadcast(&self, message: &NetworkMessage, stream_id: u64, exclude: Option<NodeId>) -> NetworkResult<()> {
        let peer_ids = self.peer_registry.peer_ids().await;
        let peer_count = peer_ids.len();

        let mut errors = Vec::new();
        for node_id in peer_ids {
            if Some(node_id) == exclude {
                continue;
            }

            if let Err(e) = self.send(&node_id, message, stream_id).await {
                warn!("Failed to broadcast to peer {}: {}", node_id, e);
                errors.push((node_id, e));
            }
        }

        if !errors.is_empty() && errors.len() == peer_count {
            return Err(NetworkError::ConnectionLost {
                node_id: NodeId(0),
                reason: "all broadcast sends failed".to_string(),
            });
        }

        Ok(())
    }

    /// Close a connection to a peer.
    pub async fn close_connection(&self, node_id: &NodeId) {
        let mut pools = self.connection_pools.write().await;
        if let Some(pool) = pools.get_mut(node_id) {
            pool.close_all();
            debug!("Closed all connections to peer {}", node_id);
        }
    }

    /// Close all connections.
    pub async fn close_all(&self) {
        let mut pools = self.connection_pools.write().await;
        for (node_id, pool) in pools.iter_mut() {
            pool.close_all();
            debug!("Closed all connections to peer {}", node_id);
        }
        pools.clear();
    }

    // ========================================================================
    // TEMPORARY: Development-only certificate creation
    // These methods are only used when mTLS config is not provided
    // In production, always provide an MtlsConfig
    // ========================================================================

    fn create_temporary_client_config() -> NetworkResult<ClientConfig> {
        // Create a self-signed certificate (INSECURE - for development only)
        use rustls::pki_types::{CertificateDer, PrivateKeyDer};

        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
            .map_err(|e| NetworkError::CertificateError(e.to_string()))?;

        let cert_der = CertificateDer::from(cert.cert.der().to_vec());
        let key_der = PrivateKeyDer::try_from(cert.key_pair.serialize_der())
            .map_err(|e| NetworkError::CertificateError(format!("Invalid private key: {:?}", e)))?;

        // Create rustls config that accepts any certificate (INSECURE)
        let mut rustls_config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
            .with_client_auth_cert(vec![cert_der], key_der)
            .map_err(|e| NetworkError::TlsConfigError(e.to_string()))?;

        rustls_config.alpn_protocols = vec![b"mpc".to_vec()];

        // Convert to quinn config
        let mut config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(rustls_config)
                .map_err(|e| NetworkError::TlsConfigError(format!("Failed to create QUIC client config: {}", e)))?,
        ));

        // Configure transport
        let mut transport = quinn::TransportConfig::default();
        transport.max_idle_timeout(Some(IDLE_TIMEOUT.try_into().unwrap()));
        transport.keep_alive_interval(Some(KEEP_ALIVE_INTERVAL));
        config.transport_config(Arc::new(transport));

        Ok(config)
    }

    fn create_temporary_server_config() -> NetworkResult<ServerConfig> {
        // Create a self-signed certificate (INSECURE - for development only)
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
            .map_err(|e| NetworkError::CertificateError(e.to_string()))?;

        let cert_der = rustls::pki_types::CertificateDer::from(cert.cert.der().to_vec());
        let key_der = rustls::pki_types::PrivateKeyDer::try_from(cert.key_pair.serialize_der())
            .map_err(|e| NetworkError::CertificateError(format!("Invalid private key: {:?}", e)))?;

        // Create rustls config that accepts any client certificate (INSECURE)
        let mut rustls_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der], key_der)
            .map_err(|e| NetworkError::TlsConfigError(e.to_string()))?;

        rustls_config.alpn_protocols = vec![b"mpc".to_vec()];

        // Convert to quinn config
        let mut config = ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(rustls_config)
                .map_err(|e| NetworkError::TlsConfigError(format!("Failed to create QUIC server config: {}", e)))?,
        ));

        // Configure transport
        let mut transport = quinn::TransportConfig::default();
        transport.max_idle_timeout(Some(IDLE_TIMEOUT.try_into().unwrap()));
        config.transport_config(Arc::new(transport));

        Ok(config)
    }
}

// TEMPORARY: Skip server certificate verification (INSECURE - for development only)
#[derive(Debug)]
struct SkipServerVerification;

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_pool() {
        // Test will be implemented once we have mock connections
    }

    #[test]
    fn test_stream_id_usage() {
        // Control messages should use stream IDs 0-99
        assert!(stream_id::control(0) <= stream_id::CONTROL_MAX);

        // DKG rounds should use stream IDs 100-999
        assert!(stream_id::dkg_round(0) >= stream_id::DKG_MIN);
        assert!(stream_id::dkg_round(0) <= stream_id::DKG_MAX);

        // Signing rounds should use stream IDs 1000-9999
        assert!(stream_id::signing_round(0) >= stream_id::SIGNING_MIN);
        assert!(stream_id::signing_round(0) <= stream_id::SIGNING_MAX);

        // Presignature should use stream IDs 10000+
        assert!(stream_id::presignature(0) >= stream_id::PRESIGNATURE_MIN);
    }
}
