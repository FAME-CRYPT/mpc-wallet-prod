//! TCP connection management for P2P messaging.
//!
//! Manages connections to peer nodes, with automatic reconnection
//! and peer discovery via the node registry.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncWriteExt, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};

use super::error::P2pError;
use super::protocol::P2pMessage;

/// Default P2P port.
pub const DEFAULT_P2P_PORT: u16 = 4000;

/// Connection timeout.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Peer information from the registry.
#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// Party index.
    pub party_index: u16,
    /// HTTP endpoint (e.g., "http://mpc-node-1:3000").
    pub http_endpoint: String,
    /// P2P address (derived from HTTP endpoint).
    pub p2p_addr: SocketAddr,
}

impl PeerInfo {
    /// Create peer info from HTTP endpoint.
    ///
    /// Converts "http://mpc-node-1:3000" to P2P address "mpc-node-1:4000".
    pub fn from_http_endpoint(
        party_index: u16,
        http_endpoint: &str,
        p2p_port: u16,
    ) -> Option<Self> {
        // Parse the HTTP endpoint to extract host
        let url = http_endpoint.trim_start_matches("http://");
        let url = url.trim_start_matches("https://");

        // Split host:port
        let host = if let Some(idx) = url.find(':') {
            &url[..idx]
        } else {
            url
        };

        // Construct P2P address
        let p2p_addr_str = format!("{}:{}", host, p2p_port);

        // Try to resolve the address
        // Note: In Docker, hostnames are resolved at connection time
        // For now, we store the string and resolve later
        match p2p_addr_str.parse() {
            Ok(addr) => Some(Self {
                party_index,
                http_endpoint: http_endpoint.to_string(),
                p2p_addr: addr,
            }),
            Err(_) => {
                // Can't parse as SocketAddr - might be a hostname
                // We'll try to resolve at connection time
                debug!(
                    "P2P address {} is a hostname, will resolve at connection time",
                    p2p_addr_str
                );
                None
            }
        }
    }

    /// Get the P2P address as a string (for DNS resolution).
    pub fn p2p_addr_string(&self, p2p_port: u16) -> String {
        let url = self.http_endpoint.trim_start_matches("http://");
        let url = url.trim_start_matches("https://");
        let host = if let Some(idx) = url.find(':') {
            &url[..idx]
        } else {
            url
        };
        format!("{}:{}", host, p2p_port)
    }
}

/// Manages TCP connections to peer nodes.
pub struct ConnectionManager {
    /// Our party index.
    party_index: u16,
    /// P2P port used by all nodes.
    p2p_port: u16,
    /// Active connections: party_index -> write half of TcpStream.
    connections: RwLock<HashMap<u16, Arc<Mutex<WriteHalf<TcpStream>>>>>,
    /// Known peers: party_index -> peer info.
    peers: RwLock<HashMap<u16, PeerInfo>>,
    /// Registry URL for fetching peer list.
    registry_url: String,
}

impl ConnectionManager {
    /// Create a new connection manager.
    pub fn new(party_index: u16, registry_url: String, p2p_port: u16) -> Self {
        Self {
            party_index,
            p2p_port,
            connections: RwLock::new(HashMap::new()),
            peers: RwLock::new(HashMap::new()),
            registry_url,
        }
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

            // Store peer info (we'll resolve the address at connection time)
            peers.insert(
                node.party_index,
                PeerInfo {
                    party_index: node.party_index,
                    http_endpoint: node.endpoint.clone(),
                    p2p_addr: "0.0.0.0:0".parse().unwrap(), // Placeholder
                },
            );
        }

        info!("Updated peer list: {} peers (excluding self)", peers.len());

        Ok(())
    }

    /// Get the P2P address for a peer.
    async fn get_peer_addr(&self, party_index: u16) -> Result<String, P2pError> {
        let peers = self.peers.read().await;
        let peer = peers
            .get(&party_index)
            .ok_or(P2pError::PeerNotFound { party_index })?;

        Ok(peer.p2p_addr_string(self.p2p_port))
    }

    /// Connect to a peer.
    async fn connect(
        &self,
        party_index: u16,
    ) -> Result<Arc<Mutex<WriteHalf<TcpStream>>>, P2pError> {
        // Check if we already have a connection
        {
            let connections = self.connections.read().await;
            if let Some(conn) = connections.get(&party_index) {
                return Ok(conn.clone());
            }
        }

        // Get peer address
        let addr = self.get_peer_addr(party_index).await?;
        debug!("Connecting to peer {} at {}", party_index, addr);

        // Connect with timeout
        let stream = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(&addr))
            .await
            .map_err(|_| P2pError::Timeout(format!("connecting to peer {}", party_index)))?
            .map_err(|e| P2pError::ConnectionFailed {
                party_index,
                source: e,
            })?;

        // Disable Nagle's algorithm for lower latency
        stream.set_nodelay(true).ok();

        info!("Connected to peer {} at {}", party_index, addr);

        // Split the stream - we only keep the write half for sending
        // The read half is handled by the receiver task
        let (read_half, write_half) = tokio::io::split(stream);

        // Store the connection (write half only)
        let write_arc = Arc::new(Mutex::new(write_half));
        {
            let mut connections = self.connections.write().await;
            connections.insert(party_index, write_arc.clone());
        }

        // Note: read_half is dropped here - incoming messages come from listener
        drop(read_half);

        Ok(write_arc)
    }

    /// Send a message to a specific peer.
    pub async fn send(&self, party_index: u16, msg: &P2pMessage) -> Result<(), P2pError> {
        let conn = self.connect(party_index).await?;

        let encoded = msg.encode()?;

        let mut writer = conn.lock().await;
        writer
            .write_all(&encoded)
            .await
            .map_err(|e| P2pError::SendFailed {
                party_index,
                source: e,
            })?;
        writer.flush().await.map_err(|e| P2pError::SendFailed {
            party_index,
            source: e,
        })?;

        debug!(
            "Sent message to peer {}: session={}, round={}, seq={}",
            party_index, msg.session_id, msg.round, msg.seq
        );

        Ok(())
    }

    /// Broadcast a message to all peers (except self and optionally one other).
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
                warn!("Failed to send to peer {}: {}", party_index, e);
                errors.push((party_index, e));
            }
        }

        if errors.len() == self.peers.read().await.len() {
            // All sends failed
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
        if connections.remove(&party_index).is_some() {
            debug!("Removed connection to peer {}", party_index);
        }
    }

    /// Get the list of known peer indices.
    #[allow(dead_code)]
    pub async fn peer_indices(&self) -> Vec<u16> {
        let peers = self.peers.read().await;
        peers.keys().copied().collect()
    }

    /// Check if we have a peer registered.
    #[allow(dead_code)]
    pub async fn has_peer(&self, party_index: u16) -> bool {
        let peers = self.peers.read().await;
        peers.contains_key(&party_index)
    }

    /// Manually add a peer (for testing or static configuration).
    #[allow(dead_code)]
    pub async fn add_peer(&self, party_index: u16, http_endpoint: String) {
        let mut peers = self.peers.write().await;
        peers.insert(
            party_index,
            PeerInfo {
                party_index,
                http_endpoint,
                p2p_addr: "0.0.0.0:0".parse().unwrap(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_info_from_http_endpoint() {
        // IP address
        let peer = PeerInfo::from_http_endpoint(0, "http://192.168.1.1:3000", 4000);
        assert!(peer.is_some());
        let peer = peer.unwrap();
        assert_eq!(peer.p2p_addr.port(), 4000);

        // Localhost
        let peer = PeerInfo::from_http_endpoint(1, "http://127.0.0.1:3000", 4000);
        assert!(peer.is_some());
    }

    #[test]
    fn test_peer_addr_string() {
        let peer = PeerInfo {
            party_index: 0,
            http_endpoint: "http://mpc-node-1:3000".to_string(),
            p2p_addr: "0.0.0.0:0".parse().unwrap(),
        };

        let addr = peer.p2p_addr_string(4000);
        assert_eq!(addr, "mpc-node-1:4000");
    }
}
