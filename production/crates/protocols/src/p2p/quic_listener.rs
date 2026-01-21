//! QUIC listener for incoming P2P connections.
//!
//! Spawns a background task that accepts incoming QUIC connections
//! from peer nodes and routes messages to session queues.

use std::net::SocketAddr;

use quinn::{Endpoint, ServerConfig};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use super::error::P2pError;
use super::quic::handle_incoming_connection;
use super::receiver::IncomingQueue;

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
    ///
    /// * `listen_addr` - Address to bind to (e.g., "0.0.0.0:4001")
    /// * `server_config` - TLS server configuration with certificates
    pub fn new(listen_addr: SocketAddr, server_config: ServerConfig) -> Result<Self, P2pError> {
        let endpoint =
            Endpoint::server(server_config, listen_addr).map_err(|e| P2pError::BindFailed {
                address: listen_addr.to_string(),
                source: std::io::Error::other(e.to_string()),
            })?;

        let local_addr = endpoint.local_addr().map_err(|e| P2pError::BindFailed {
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
    pub fn start(&mut self, incoming_queue: IncomingQueue) -> Result<(), P2pError> {
        if self.accept_task.is_some() {
            return Err(P2pError::CodecError("Listener already started".to_string()));
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

/// Spawn a QUIC listener as a background task.
///
/// This is a convenience function that creates and starts a listener.
///
/// # Arguments
///
/// * `listen_addr` - Address to bind to
/// * `server_config` - TLS server configuration
/// * `incoming_queue` - Queue for incoming messages
///
/// # Returns
///
/// A handle to the listener that can be used to stop it.
pub fn spawn_quic_listener(
    listen_addr: SocketAddr,
    server_config: ServerConfig,
    incoming_queue: IncomingQueue,
) -> Result<QuicListener, P2pError> {
    let mut listener = QuicListener::new(listen_addr, server_config)?;
    listener.start(incoming_queue)?;
    Ok(listener)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::p2p::certs::CaCertificate;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_listener_creation() {
        // Generate test certificates
        let ca = CaCertificate::generate().unwrap();
        let node_cert = ca.sign_node_cert(0, &["localhost".to_string()]).unwrap();
        let server_config = node_cert.server_config().unwrap();

        // Create a QUIC server config from rustls config
        let quic_server_config = quinn::ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(server_config).unwrap(),
        ));

        // Create listener on random port
        let listener = QuicListener::new("127.0.0.1:0".parse().unwrap(), quic_server_config);

        assert!(listener.is_ok());
        let listener = listener.unwrap();
        assert_ne!(listener.local_addr().port(), 0);
    }
}
