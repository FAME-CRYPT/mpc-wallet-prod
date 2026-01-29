//! Internal API handlers for node-to-node communication

use crate::error::ApiError;
use crate::state::AppState;
use axum::{extract::State, Json};
use threshold_types::VoteRequest;
use tracing::info;
use serde::{Deserialize, Serialize};

/// DKG join request from coordinator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DkgJoinRequest {
    pub session_id: String,
    pub protocol: String,
    pub threshold: u32,
    pub total_nodes: u32,
}

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

/// Receive a DKG join request from coordinator
///
/// POST /internal/dkg-join
pub async fn receive_dkg_join_request(
    State(state): State<AppState>,
    Json(req): Json<DkgJoinRequest>,
) -> Result<Json<&'static str>, ApiError> {
    info!(
        "Received DKG join request for session_id={} protocol={}",
        req.session_id, req.protocol
    );

    // Parse session ID
    let session_uuid = uuid::Uuid::parse_str(&req.session_id)
        .map_err(|_| ApiError::BadRequest("Invalid session ID format".into()))?;

    // Join the DKG ceremony automatically
    tokio::spawn(async move {
        match state.dkg_service.join_dkg_ceremony(session_uuid).await {
            Ok(result) => {
                info!(
                    "Successfully joined DKG ceremony: session_id={} address={}",
                    result.session_id, result.address
                );
            }
            Err(e) => {
                tracing::error!("Failed to join DKG ceremony: {}", e);
            }
        }
    });

    Ok(Json("DKG join request received"))
}

/// Aux_info join request from coordinator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxInfoJoinRequest {
    pub session_id: String,
    pub num_parties: u16,
}

/// Receive an aux_info join request from coordinator
///
/// POST /internal/aux-info-join
///
/// This fixes SORUN #15 by allowing participant nodes to join aux_info ceremonies.
pub async fn receive_aux_info_join_request(
    State(state): State<AppState>,
    Json(req): Json<AuxInfoJoinRequest>,
) -> Result<Json<&'static str>, ApiError> {
    info!(
        "Received aux_info join request for session_id={} num_parties={}",
        req.session_id, req.num_parties
    );

    // Parse session ID
    let session_uuid = uuid::Uuid::parse_str(&req.session_id)
        .map_err(|_| ApiError::BadRequest("Invalid session ID format".into()))?;

    // Join the aux_info ceremony automatically
    tokio::spawn(async move {
        match state.aux_info_service.join_aux_info_ceremony(session_uuid).await {
            Ok(result) => {
                info!(
                    "Successfully joined aux_info ceremony: session_id={} success={}",
                    result.session_id, result.success
                );
            }
            Err(e) => {
                tracing::error!("Failed to join aux_info ceremony: {}", e);
            }
        }
    });

    Ok(Json("Aux_info join request received"))
}

/// Signing join request from coordinator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigningJoinRequest {
    pub session_id: String,
    pub tx_id: String,
    pub protocol: String,
    pub unsigned_tx: Vec<u8>,
    pub message_hash: Vec<u8>,
}

/// Receive a signing join request from coordinator
///
/// POST /internal/signing-join
///
/// This fixes SORUN #17 by allowing participant nodes to join signing ceremonies.
pub async fn receive_signing_join_request(
    State(_state): State<AppState>,
    Json(req): Json<SigningJoinRequest>,
) -> Result<Json<&'static str>, ApiError> {
    info!(
        "Received signing join request for session_id={} tx_id={} protocol={}",
        req.session_id, req.tx_id, req.protocol
    );

    // For signing, we don't need to spawn a join task like DKG/aux_info
    // The signing coordinator will handle the protocol via QUIC messages
    // This HTTP request just ensures all nodes are "aware" of the signing session
    // and ready to receive QUIC protocol messages

    info!(
        "Acknowledged signing session: session_id={} tx_id={}",
        req.session_id, req.tx_id
    );

    Ok(Json("Signing join request received"))
}
