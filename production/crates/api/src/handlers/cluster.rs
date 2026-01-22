//! Cluster monitoring business logic handlers

use threshold_storage::{EtcdStorage, PostgresStorage};
use tokio::sync::Mutex;
use tracing::info;

use crate::{error::ApiError, routes::cluster::NodeInfo};

/// Cluster status information
pub struct ClusterStatus {
    pub total_nodes: u32,
    pub healthy_nodes: u32,
    pub threshold: u32,
    pub status: String,
}

/// Get overall cluster health status
pub async fn get_cluster_status(
    _postgres: &PostgresStorage,
    _etcd: &Mutex<EtcdStorage>,
) -> Result<ClusterStatus, ApiError> {
    info!("Fetching cluster status");

    // In production, this would:
    // 1. Query etcd for cluster configuration
    // 2. Check node health from PostgreSQL
    // 3. Calculate overall cluster health

    // For now, return placeholder values
    // You would implement: etcd.get_cluster_config() and check node health

    Ok(ClusterStatus {
        total_nodes: 5,
        healthy_nodes: 5,
        threshold: 3,
        status: "healthy".to_string(),
    })
}

/// List all nodes in the cluster with their health information
pub async fn list_cluster_nodes(postgres: &PostgresStorage) -> Result<Vec<NodeInfo>, ApiError> {
    use threshold_types::NodeId;

    info!("Listing cluster nodes");

    let mut nodes = vec![];

    // Query all nodes (assuming node IDs 1-5 for a 5-node cluster)
    // In production, you might query etcd for the actual cluster configuration
    for node_id in 1..=5 {
        match postgres.get_node_health(NodeId(node_id)).await {
            Ok(Some(health_data)) => {
                let status = health_data["status"].as_str().unwrap_or("unknown").to_string();

                // Parse last_heartbeat - it's already a DateTime in the JSON
                let last_heartbeat: chrono::DateTime<chrono::Utc> = serde_json::from_value(
                    health_data["last_heartbeat"].clone()
                ).unwrap_or_else(|_| chrono::Utc::now());

                let total_votes = health_data["total_votes"].as_i64().unwrap_or(0);
                let total_violations = health_data["total_violations"].as_i64().unwrap_or(0);
                let seconds_since_heartbeat = health_data["seconds_since_heartbeat"].as_f64().unwrap_or(0.0);

                // Parse banned_until - it's either null or a DateTime
                let is_banned = health_data["banned_until"]
                    .as_str()
                    .is_some() &&
                    serde_json::from_value::<Option<chrono::DateTime<chrono::Utc>>>(
                        health_data["banned_until"].clone()
                    )
                    .ok()
                    .flatten()
                    .map(|dt| dt > chrono::Utc::now())
                    .unwrap_or(false);

                nodes.push(NodeInfo {
                    node_id,
                    status,
                    last_heartbeat,
                    total_votes,
                    total_violations,
                    seconds_since_heartbeat,
                    is_banned,
                });
            }
            Ok(None) => {
                // Node not found in database, include it as inactive
                info!("Node {} not found in database, marking as inactive", node_id);
                nodes.push(NodeInfo {
                    node_id,
                    status: "inactive".to_string(),
                    last_heartbeat: chrono::Utc::now(),
                    total_votes: 0,
                    total_violations: 0,
                    seconds_since_heartbeat: 0.0,
                    is_banned: false,
                });
            }
            Err(e) => {
                info!("Error fetching health for node {}: {}", node_id, e);
                // Continue with other nodes
            }
        }
    }

    Ok(nodes)
}
