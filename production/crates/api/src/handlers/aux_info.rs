//! Aux Info API handlers

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Request to generate aux_info
#[derive(Debug, Deserialize)]
pub struct GenerateAuxInfoRequest {
    /// Number of parties
    pub num_parties: u16,
    /// Node IDs of participants
    pub participants: Vec<u64>,
}

/// Response for aux_info generation
#[derive(Debug, Serialize)]
pub struct AuxInfoResponse {
    pub success: bool,
    pub session_id: String,
    pub party_index: u16,
    pub num_parties: u16,
    pub aux_info_size_bytes: usize,
    pub error: Option<String>,
}

/// Status response for aux_info
#[derive(Debug, Serialize)]
pub struct AuxInfoStatusResponse {
    pub has_aux_info: bool,
    pub latest_session_id: Option<String>,
    pub aux_info_size_bytes: usize,
    pub total_ceremonies: usize,
}

/// Generate aux_info via multi-party protocol
pub async fn generate_aux_info(
    State(state): State<AppState>,
    Json(req): Json<GenerateAuxInfoRequest>,
) -> Result<Json<AuxInfoResponse>, ApiError> {
    // Validate request
    if req.num_parties < 2 {
        return Err(ApiError::BadRequest(
            "Number of parties must be at least 2".to_string(),
        ));
    }

    if req.participants.len() != req.num_parties as usize {
        return Err(ApiError::BadRequest(format!(
            "Participants list length ({}) doesn't match num_parties ({})",
            req.participants.len(),
            req.num_parties
        )));
    }

    // Convert participants to NodeId
    let participants: Vec<threshold_types::NodeId> = req
        .participants
        .iter()
        .map(|&id| threshold_types::NodeId(id))
        .collect();

    // Call aux_info service
    let result = state
        .aux_info_service
        .initiate_aux_info_gen(req.num_parties, participants)
        .await
        .map_err(|e| ApiError::InternalError(format!("Aux_info generation failed: {}", e)))?;

    let response = AuxInfoResponse {
        success: result.success,
        session_id: result.session_id.to_string(),
        party_index: result.party_index,
        num_parties: result.num_parties,
        aux_info_size_bytes: result.aux_info_data.as_ref().map(|d| d.len()).unwrap_or(0),
        error: result.error,
    };

    Ok(Json(response))
}

/// Get aux_info status
pub async fn aux_info_status(
    State(state): State<AppState>,
) -> Result<Json<AuxInfoStatusResponse>, ApiError> {
    // Get latest aux_info
    let latest = state.aux_info_service.get_latest_aux_info().await;

    // Get all ceremonies
    let ceremonies = state.aux_info_service.list_ceremonies().await;

    let (has_aux_info, latest_session_id, aux_info_size) = if let Some((session_id, data)) = latest
    {
        (true, Some(session_id.to_string()), data.len())
    } else {
        (false, None, 0)
    };

    let response = AuxInfoStatusResponse {
        has_aux_info,
        latest_session_id,
        aux_info_size_bytes: aux_info_size,
        total_ceremonies: ceremonies.len(),
    };

    Ok(Json(response))
}

/// Join an existing aux_info generation ceremony
///
/// POST /internal/aux-info-join
///
/// This endpoint is called by participant nodes when they receive a join request
/// from the coordinator. This fixes SORUN #15.
pub async fn join_aux_info(
    State(state): State<AppState>,
    Json(req): Json<JoinAuxInfoRequest>,
) -> Result<Json<AuxInfoResponse>, ApiError> {
    // Parse session ID
    let session_uuid = uuid::Uuid::parse_str(&req.session_id)
        .map_err(|_| ApiError::BadRequest("Invalid session ID format".to_string()))?;

    // Join the aux_info ceremony (participant node)
    let result = state
        .aux_info_service
        .join_aux_info_ceremony(session_uuid)
        .await
        .map_err(|e| ApiError::InternalError(format!("Failed to join aux_info ceremony: {}", e)))?;

    let response = AuxInfoResponse {
        success: result.success,
        session_id: result.session_id.to_string(),
        party_index: result.party_index,
        num_parties: result.num_parties,
        aux_info_size_bytes: result.aux_info_data.as_ref().map(|d| d.len()).unwrap_or(0),
        error: result.error,
    };

    Ok(Json(response))
}

/// Request to join an aux_info ceremony
#[derive(Debug, Deserialize)]
pub struct JoinAuxInfoRequest {
    pub session_id: String,
    pub num_parties: u16,
}
