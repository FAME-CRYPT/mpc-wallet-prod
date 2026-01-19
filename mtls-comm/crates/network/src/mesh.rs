use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;
use threshold_types::NodeId;
use tracing::{info, warn};

use crate::mtls_node::MtlsNode;

pub struct MeshTopology {
    bootstrap_peers: Vec<SocketAddr>,
}

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub node_id: NodeId,
    pub addr: SocketAddr,
}

impl MeshTopology {
    pub fn new(bootstrap_peers: Vec<SocketAddr>) -> Self {
        Self { bootstrap_peers }
    }

    pub async fn discover_peers(&mut self, node: &MtlsNode) -> anyhow::Result<()> {
        // Connect to bootstrap peers
        for addr in &self.bootstrap_peers {
            match node.connect_to_peer(*addr, "peer.local").await {
                Ok(peer_id) => {
                    info!("Connected to bootstrap peer {} at {}", peer_id.0, addr);
                }
                Err(e) => {
                    warn!("Failed to connect to bootstrap {}: {}", addr, e);
                }
            }
        }

        Ok(())
    }

    pub async fn maintain_connections(node: &MtlsNode, bootstrap_peers: Vec<SocketAddr>) {
        // Periodic heartbeat
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;

            let connected = node.connected_peers().await;
            info!("Heartbeat: {} peers connected", connected.len());

            // Reconnect to bootstrap peers if disconnected
            for addr in &bootstrap_peers {
                // Check if any connected peer is at this address
                // If not, try to reconnect
                match node.connect_to_peer(*addr, "peer.local").await {
                    Ok(peer_id) => {
                        info!("Reconnected to peer {} at {}", peer_id.0, addr);
                    }
                    Err(_) => {
                        // Already connected or still unavailable
                    }
                }
            }
        }
    }
}
