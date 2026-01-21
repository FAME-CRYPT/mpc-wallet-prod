//! Cluster monitoring endpoints

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::{state::AppState, ApiResult};

/// Cluster health status response
#[derive(Debug, Serialize, Deserialize)]
pub struct ClusterStatusResponse {
    /// Total number of nodes in the cluster
    pub total_nodes: u32,
    /// Number of healthy nodes
    pub healthy_nodes: u32,
    /// Consensus threshold
    pub threshold: u32,
    /// Overall cluster health status
    pub status: String,
    /// Timestamp of the status check
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Node information
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Node ID
    pub node_id: u64,
    /// Node status (active, inactive, banned)
    pub status: String,
    /// Last heartbeat timestamp
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
    /// Total votes cast by this node
    pub total_votes: i64,
    /// Total Byzantine violations
    pub total_violations: i64,
    /// Seconds since last heartbeat
    pub seconds_since_heartbeat: f64,
    /// Whether the node is currently banned
    pub is_banned: bool,
}

/// List of cluster nodes response
#[derive(Debug, Serialize, Deserialize)]
pub struct ListNodesResponse {
    pub nodes: Vec<NodeInfo>,
    pub total: usize,
}

/// GET /api/v1/cluster/status - Get cluster health status
///
/// Returns the overall health status of the MPC cluster including
/// node count and consensus threshold information
pub async fn get_cluster_status(
    State(state): State<AppState>,
) -> ApiResult<Json<ClusterStatusResponse>> {
    // Fetch cluster status from handler
    let status = crate::handlers::cluster::get_cluster_status(
        state.postgres.as_ref(),
        state.etcd.as_ref(),
    )
    .await?;

    Ok(Json(ClusterStatusResponse {
        total_nodes: status.total_nodes,
        healthy_nodes: status.healthy_nodes,
        threshold: status.threshold,
        status: status.status,
        timestamp: chrono::Utc::now(),
    }))
}

/// GET /api/v1/cluster/nodes - List all cluster nodes
///
/// Returns detailed information about all nodes in the cluster
/// including their health status and activity metrics
pub async fn list_nodes(State(state): State<AppState>) -> ApiResult<Json<ListNodesResponse>> {
    // Fetch node list from handler
    let nodes = crate::handlers::cluster::list_cluster_nodes(state.postgres.as_ref()).await?;

    let total = nodes.len();

    Ok(Json(ListNodesResponse { nodes, total }))
}
