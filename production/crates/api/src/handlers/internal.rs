//! Internal API handlers for node-to-node communication

use crate::error::ApiError;
use crate::state::AppState;
use axum::{extract::State, Json};
use threshold_types::VoteRequest;
use tracing::info;

/// Receive a vote request from orchestrator
///
/// POST /internal/vote-request
pub async fn receive_vote_request(
    State(state): State<AppState>,
    Json(req): Json<VoteRequest>,
) -> Result<Json<&'static str>, ApiError> {
    info!("Received vote request for tx_id={}", req.tx_id);

    // Trigger automatic voting mechanism
    state
        .vote_trigger
        .send(req)
        .await
        .map_err(|_| ApiError::InternalError("Vote trigger channel closed".into()))?;

    Ok(Json("Vote request received"))
}
