//! HTTP handlers for the node registry API.
//!
//! These handlers allow nodes to register themselves, send heartbeats,
//! and allow clients to query the registry state.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use tokio::sync::RwLock;
use tracing::{info, trace, warn};

use common::discovery::{
    HeartbeatRequest, HeartbeatResponse, ListNodesResponse, NodeInfo, RegisterNodeRequest,
    RegisterNodeResponse,
};

use crate::state::AppState;

/// Register a new node with the coordinator.
///
/// POST /registry/register
///
/// ## Authentication
///
/// Requires a valid pre-shared key (PSK) in the request. The PSK must match
/// the coordinator's configured NODE_REGISTRATION_PSK environment variable.
/// This prevents unauthorized parties from registering nodes.
///
/// ## Response
///
/// For new registrations, returns a `cert_token` that must be used when
/// requesting TLS certificates. This token is only returned once and should
/// be stored securely by the node.
pub async fn register_node(
    State(state): State<Arc<RwLock<AppState>>>,
    Json(request): Json<RegisterNodeRequest>,
) -> Result<Json<RegisterNodeResponse>, (StatusCode, String)> {
    info!(
        "Node registration request: node_id={}, party_index={}, endpoint={}",
        request.node_id, request.party_index, request.endpoint
    );

    // Verify PSK before allowing registration
    {
        let s = state.read().await;
        if !s.verify_node_psk(&request.psk) {
            warn!(
                "Node registration rejected: invalid PSK for party {}",
                request.party_index
            );
            return Err((
                StatusCode::UNAUTHORIZED,
                "Invalid pre-shared key".to_string(),
            ));
        }
    }

    let (cert_token, heartbeat_interval) = {
        let mut s = state.write().await;

        let cert_token = s
            .node_registry
            .register(
                request.node_id,
                request.party_index,
                request.endpoint.clone(),
                request.capabilities,
            )
            .map_err(|e| (StatusCode::CONFLICT, e.to_string()))?;

        (cert_token, s.node_registry.heartbeat_interval())
    };

    if cert_token.is_some() {
        info!("New node {} registered, cert_token issued", request.node_id);
    }

    Ok(Json(RegisterNodeResponse {
        success: true,
        message: format!("Node {} registered successfully", request.node_id),
        heartbeat_interval_secs: heartbeat_interval,
        cert_token,
    }))
}

/// Process a heartbeat from a registered node.
///
/// POST /registry/heartbeat
///
/// Requires valid authentication:
/// 1. `cert_token` must be valid for the claimed `party_index`
/// 2. `node_id` must match the registered node for that party
///
/// This prevents:
/// - Third parties from keeping stale nodes online (DoS prevention)
/// - Valid token holders from impersonating other nodes
pub async fn heartbeat(
    State(state): State<Arc<RwLock<AppState>>>,
    Json(request): Json<HeartbeatRequest>,
) -> Result<Json<HeartbeatResponse>, (StatusCode, String)> {
    trace!(
        "Heartbeat from node {} (party {})",
        request.node_id,
        request.party_index
    );

    let (health_score, registered_nodes) = {
        let mut s = state.write().await;

        // Verify both cert_token AND node_id match the registered party
        // This prevents:
        // 1. Invalid tokens from being accepted
        // 2. Valid token holders from keeping OTHER nodes online by supplying their node_id
        if let Err(reason) = s.node_registry.verify_heartbeat_auth(
            request.party_index,
            request.node_id,
            &request.cert_token,
        ) {
            warn!(
                "Heartbeat rejected for party {}: {}",
                request.party_index, reason
            );
            return Err((StatusCode::UNAUTHORIZED, reason.to_string()));
        }

        // We don't have the actual latency here since this is the node calling us
        // The latency measurement happens when coordinator pings nodes
        let health_score = s
            .node_registry
            .heartbeat(
                request.node_id,
                request.capabilities,
                None, // Latency measured separately by health checker
            )
            .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

        let registered_nodes = s.node_registry.all_nodes().len();

        (health_score, registered_nodes)
    };

    Ok(Json(HeartbeatResponse {
        success: true,
        message: "Heartbeat acknowledged".to_string(),
        health_score,
        registered_nodes,
    }))
}

/// List all registered nodes.
///
/// GET /registry/nodes
pub async fn list_nodes(State(state): State<Arc<RwLock<AppState>>>) -> Json<ListNodesResponse> {
    let s = state.read().await;

    let nodes: Vec<NodeInfo> = s.node_registry.all_nodes().into_iter().cloned().collect();

    let online_count = nodes.iter().filter(|n| n.is_online).count();
    let total_count = nodes.len();

    Json(ListNodesResponse {
        nodes,
        online_count,
        total_count,
    })
}

/// Get a specific node by party index.
///
/// GET /registry/node/:party_index
pub async fn get_node(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(party_index): Path<u16>,
) -> Result<Json<NodeInfo>, (StatusCode, String)> {
    let s = state.read().await;

    let node = s
        .node_registry
        .get_by_party(party_index)
        .ok_or((
            StatusCode::NOT_FOUND,
            format!("No node registered for party {}", party_index),
        ))?
        .clone();

    Ok(Json(node))
}

/// Get registry statistics.
///
/// GET /registry/stats
pub async fn registry_stats(State(state): State<Arc<RwLock<AppState>>>) -> Json<serde_json::Value> {
    let s = state.read().await;
    let stats = s.node_registry.stats();

    Json(serde_json::json!({
        "total_nodes": stats.total_nodes,
        "online_nodes": stats.online_nodes,
        "avg_health_score": stats.avg_health_score,
        "heartbeat_interval_secs": s.node_registry.heartbeat_interval(),
    }))
}
