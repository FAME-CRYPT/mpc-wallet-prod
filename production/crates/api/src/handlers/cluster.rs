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
    postgres: &PostgresStorage,
    etcd: &Mutex<EtcdStorage>,
) -> Result<ClusterStatus, ApiError> {
    use threshold_types::NodeId;

    info!("Fetching cluster status");

    // Step 1: Get cluster configuration from etcd (with fallback to defaults)
    let (total_nodes, threshold) = {
        let mut etcd_client = etcd.lock().await;
        match etcd_client.get("/cluster/config").await {
            Ok(Some(config_bytes)) => {
                match String::from_utf8(config_bytes) {
                    Ok(config_str) => {
                        match serde_json::from_str::<serde_json::Value>(&config_str) {
                            Ok(config) => {
                                let total = config["total_nodes"].as_u64().unwrap_or(5) as u32;
                                let thresh = config["threshold"].as_u64().unwrap_or(3) as u32;
                                (total, thresh)
                            }
                            Err(_) => (5, 3), // Default configuration
                        }
                    }
                    Err(_) => (5, 3), // Failed to parse UTF-8
                }
            }
            Ok(None) | Err(_) => (5, 3), // Default if not set in etcd or error
        }
    };

    // Step 2: Query actual node health from PostgreSQL
    let mut healthy_count = 0;
    let health_threshold_seconds = 60.0; // Consider node healthy if seen in last 60 seconds

    for node_id in 1..=total_nodes {
        match postgres.get_node_health(NodeId(node_id as u64)).await {
            Ok(Some(health_data)) => {
                let seconds_since_heartbeat = health_data["seconds_since_heartbeat"]
                    .as_f64()
                    .unwrap_or(9999.0);

                if seconds_since_heartbeat < health_threshold_seconds {
                    healthy_count += 1;
                    info!("Node {} is healthy (last seen {:.1}s ago)", node_id, seconds_since_heartbeat);
                } else {
                    info!("Node {} is unhealthy (last seen {:.1}s ago)", node_id, seconds_since_heartbeat);
                }
            }
            Ok(None) => {
                info!("Node {} has no health data - treating as inactive", node_id);
            }
            Err(e) => {
                info!("Error checking health for node {}: {} - treating as inactive", node_id, e);
            }
        }
    }

    // Step 3: Determine overall cluster health status
    let status = if healthy_count >= threshold {
        "healthy"
    } else if healthy_count > 0 {
        "degraded"
    } else {
        "critical"
    };

    info!(
        "Cluster status: {}/{} healthy nodes (threshold: {}, status: {})",
        healthy_count, total_nodes, threshold, status
    );

    Ok(ClusterStatus {
        total_nodes,
        healthy_nodes: healthy_count,
        threshold,
        status: status.to_string(),
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
