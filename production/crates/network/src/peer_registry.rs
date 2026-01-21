//! Peer registry for managing peer connections and discovery.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info};

use threshold_types::NodeId;
use crate::error::{NetworkError, NetworkResult};

/// Default QUIC port for peer communication.
pub const DEFAULT_QUIC_PORT: u16 = 4001;

/// Information about a peer node.
#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// Node ID of the peer.
    pub node_id: NodeId,
    /// Hostname or IP address.
    pub hostname: String,
    /// QUIC port.
    pub port: u16,
    /// Whether the peer is currently online.
    pub is_online: bool,
}

/// Peer registry for managing known peers.
///
/// This registry maintains a list of known peers and their connection information.
/// Peers can be discovered through a registry service or added manually.
pub struct PeerRegistry {
    /// Our node ID.
    local_node_id: NodeId,
    /// Known peers: node_id -> PeerInfo.
    peers: Arc<RwLock<HashMap<NodeId, PeerInfo>>>,
    /// Optional registry service URL for dynamic peer discovery.
    registry_url: Option<String>,
}

impl PeerRegistry {
    /// Create a new peer registry.
    ///
    /// # Arguments
    /// * `local_node_id` - Our node ID (to exclude from peer list)
    /// * `registry_url` - Optional URL of the registry service for peer discovery
    pub fn new(local_node_id: NodeId, registry_url: Option<String>) -> Self {
        Self {
            local_node_id,
            peers: Arc::new(RwLock::new(HashMap::new())),
            registry_url,
        }
    }

    /// Add a peer manually.
    pub async fn add_peer(&self, node_id: NodeId, hostname: String, port: u16) {
        if node_id == self.local_node_id {
            return; // Don't add ourselves
        }

        let mut peers = self.peers.write().await;
        peers.insert(
            node_id,
            PeerInfo {
                node_id,
                hostname,
                port,
                is_online: true,
            },
        );
        debug!("Added peer {} to registry", node_id);
    }

    /// Remove a peer.
    pub async fn remove_peer(&self, node_id: &NodeId) {
        let mut peers = self.peers.write().await;
        peers.remove(node_id);
        debug!("Removed peer {} from registry", node_id);
    }

    /// Get information about a peer.
    pub async fn get_peer(&self, node_id: &NodeId) -> NetworkResult<PeerInfo> {
        let peers = self.peers.read().await;
        peers
            .get(node_id)
            .cloned()
            .ok_or(NetworkError::PeerNotFound { node_id: *node_id })
    }

    /// Get all known peer IDs.
    pub async fn peer_ids(&self) -> Vec<NodeId> {
        let peers = self.peers.read().await;
        peers.keys().copied().collect()
    }

    /// Get socket address for a peer.
    pub async fn peer_address(&self, node_id: &NodeId) -> NetworkResult<SocketAddr> {
        let peer = self.get_peer(node_id).await?;

        // Resolve hostname to socket address
        let addr_str = format!("{}:{}", peer.hostname, peer.port);
        let mut addrs = tokio::net::lookup_host(&addr_str)
            .await
            .map_err(|e| NetworkError::InvalidAddress {
                node_id: *node_id,
                address: format!("{}: {}", addr_str, e),
            })?;

        addrs
            .next()
            .ok_or(NetworkError::InvalidAddress {
                node_id: *node_id,
                address: format!("{}: no addresses found", addr_str),
            })
    }

    /// Refresh the peer list from the registry service.
    ///
    /// This fetches the current list of online nodes from the registry service.
    /// Only available if a registry_url was provided.
    pub async fn refresh_from_registry(&self) -> NetworkResult<()> {
        let registry_url = self.registry_url.as_ref().ok_or_else(|| {
            NetworkError::RegistryError("No registry URL configured".to_string())
        })?;

        let url = format!("{}/registry/nodes", registry_url);
        debug!("Fetching peer list from {}", url);

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| NetworkError::RegistryError(e.to_string()))?;

        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| NetworkError::RegistryError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(NetworkError::RegistryError(format!(
                "registry returned status {}",
                response.status()
            )));
        }

        // Parse response (placeholder - adjust based on actual registry API)
        let list: RegistryResponse = response
            .json()
            .await
            .map_err(|e| NetworkError::RegistryError(e.to_string()))?;

        let mut peers = self.peers.write().await;
        peers.clear();

        for node in list.nodes {
            if node.node_id == self.local_node_id.0 {
                continue; // Skip ourselves
            }

            if !node.is_online {
                debug!("Skipping offline node {}", node.node_id);
                continue;
            }

            // Extract hostname from endpoint
            let hostname = extract_hostname(&node.endpoint);

            peers.insert(
                NodeId(node.node_id),
                PeerInfo {
                    node_id: NodeId(node.node_id),
                    hostname,
                    port: node.quic_port.unwrap_or(DEFAULT_QUIC_PORT),
                    is_online: true,
                },
            );
        }

        info!(
            "Updated peer list from registry: {} peers (excluding self)",
            peers.len()
        );

        Ok(())
    }

    /// Mark a peer as offline.
    pub async fn mark_offline(&self, node_id: &NodeId) {
        let mut peers = self.peers.write().await;
        if let Some(peer) = peers.get_mut(node_id) {
            peer.is_online = false;
            debug!("Marked peer {} as offline", node_id);
        }
    }

    /// Mark a peer as online.
    pub async fn mark_online(&self, node_id: &NodeId) {
        let mut peers = self.peers.write().await;
        if let Some(peer) = peers.get_mut(node_id) {
            peer.is_online = true;
            debug!("Marked peer {} as online", node_id);
        }
    }
}

/// Response from registry service (placeholder - adjust based on actual API).
#[derive(serde::Deserialize)]
struct RegistryResponse {
    nodes: Vec<RegistryNode>,
}

#[derive(serde::Deserialize)]
struct RegistryNode {
    node_id: u64,
    endpoint: String,
    quic_port: Option<u16>,
    is_online: bool,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_peer_registry_add_remove() {
        let registry = PeerRegistry::new(NodeId(0), None);

        // Add a peer
        registry.add_peer(NodeId(1), "node-1".to_string(), 4001).await;

        // Should be able to get peer info
        let peer = registry.get_peer(&NodeId(1)).await.unwrap();
        assert_eq!(peer.hostname, "node-1");
        assert_eq!(peer.port, 4001);

        // Remove the peer
        registry.remove_peer(&NodeId(1)).await;

        // Should no longer exist
        assert!(registry.get_peer(&NodeId(1)).await.is_err());
    }

    #[tokio::test]
    async fn test_peer_registry_excludes_self() {
        let registry = PeerRegistry::new(NodeId(0), None);

        // Try to add ourselves
        registry.add_peer(NodeId(0), "self".to_string(), 4001).await;

        // Should not be in the peer list
        assert!(registry.get_peer(&NodeId(0)).await.is_err());
        assert_eq!(registry.peer_ids().await.len(), 0);
    }

    #[test]
    fn test_extract_hostname() {
        assert_eq!(extract_hostname("http://mpc-node-1:3000"), "mpc-node-1");
        assert_eq!(extract_hostname("https://localhost:8080"), "localhost");
        assert_eq!(extract_hostname("http://192.168.1.1:3000"), "192.168.1.1");
        assert_eq!(extract_hostname("myhost"), "myhost");
    }
}
