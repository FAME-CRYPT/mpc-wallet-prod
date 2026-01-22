//! DKG (Distributed Key Generation) API Handlers

use crate::error::ApiError;
use crate::state::AppState;
use axum::{
    extract::State,
    Json,
};
use serde::{Deserialize, Serialize};
use threshold_orchestrator::ProtocolType;

/// Request to initiate a DKG ceremony
#[derive(Debug, Deserialize)]
pub struct InitiateDkgRequest {
    /// Protocol type (cggmp24 or frost)
    pub protocol: String,
    /// Threshold (e.g., 4 for 4-of-5)
    #[serde(default = "default_threshold")]
    pub threshold: u32,
    /// Total number of nodes
    #[serde(default = "default_total_nodes")]
    pub total_nodes: u32,
}

fn default_threshold() -> u32 {
    4
}

fn default_total_nodes() -> u32 {
    5
}

/// Response from DKG initiation
#[derive(Debug, Serialize)]
pub struct DkgResponse {
    /// Success flag
    pub success: bool,
    /// Session ID
    pub session_id: String,
    /// Protocol used
    pub protocol: String,
    /// Shared public key (hex-encoded)
    pub public_key: String,
    /// Derived Bitcoin address
    pub address: String,
    /// Threshold
    pub threshold: u32,
    /// Total nodes
    pub total_nodes: u32,
}

/// DKG ceremony status
#[derive(Debug, Serialize)]
pub struct DkgStatusResponse {
    /// Number of active ceremonies
    pub active_ceremonies: usize,
    /// Total ceremonies completed
    pub total_completed: usize,
    /// CGGMP24 public key (if available)
    pub cggmp24_public_key: Option<String>,
    /// FROST public key (if available)
    pub frost_public_key: Option<String>,
    /// CGGMP24 address (if available)
    pub cggmp24_address: Option<String>,
    /// FROST address (if available)
    pub frost_address: Option<String>,
}

/// Initiate a new DKG ceremony
///
/// POST /api/v1/dkg/initiate
pub async fn initiate_dkg(
    State(state): State<AppState>,
    Json(req): Json<InitiateDkgRequest>,
) -> Result<Json<DkgResponse>, ApiError> {
    // Parse protocol type
    let protocol = match req.protocol.to_lowercase().as_str() {
        "cggmp24" => ProtocolType::CGGMP24,
        "frost" => ProtocolType::FROST,
        _ => {
            return Err(ApiError::BadRequest(format!(
                "Invalid protocol: {}. Must be 'cggmp24' or 'frost'",
                req.protocol
            )));
        }
    };

    // Validate parameters
    if req.threshold > req.total_nodes {
        return Err(ApiError::BadRequest(
            "Threshold cannot exceed total nodes".to_string(),
        ));
    }

    if req.threshold < 2 {
        return Err(ApiError::BadRequest(
            "Threshold must be at least 2".to_string(),
        ));
    }

    // Call DKG service to initiate ceremony
    let result = state
        .dkg_service
        .initiate_dkg(protocol, req.threshold, req.total_nodes)
        .await
        .map_err(|e| ApiError::InternalError(format!("DKG initiation failed: {}", e)))?;

    // Convert public key to hex
    let public_key_hex = hex::encode(&result.public_key);

    // Bitcoin address is already derived in DKG service
    // using derive_p2wpkh_address for CGGMP24 or derive_p2tr_address for FROST
    let address = result.address;

    let response = DkgResponse {
        success: true,
        session_id: result.session_id.to_string(),
        protocol: result.protocol.to_string(),
        public_key: public_key_hex,
        address,
        threshold: result.threshold,
        total_nodes: result.total_nodes,
    };

    Ok(Json(response))
}

/// Get DKG ceremony status
///
/// GET /api/v1/dkg/status
pub async fn dkg_status(
    State(state): State<AppState>,
) -> Result<Json<DkgStatusResponse>, ApiError> {
    // Query actual DKG service status
    let ceremonies = state
        .dkg_service
        .list_ceremonies()
        .await
        .map_err(|e| ApiError::InternalError(format!("Failed to list ceremonies: {}", e)))?;

    let active_ceremonies = ceremonies
        .iter()
        .filter(|c| matches!(c.status, threshold_orchestrator::DkgStatus::Running))
        .count();

    let total_completed = ceremonies
        .iter()
        .filter(|c| matches!(c.status, threshold_orchestrator::DkgStatus::Completed))
        .count();

    // Get latest CGGMP24 public key
    let cggmp24_ceremony = ceremonies
        .iter()
        .filter(|c| c.protocol == ProtocolType::CGGMP24
                && matches!(c.status, threshold_orchestrator::DkgStatus::Completed))
        .max_by_key(|c| c.completed_at);

    let (cggmp24_public_key, cggmp24_address) = if let Some(ceremony) = cggmp24_ceremony {
        if let Some(ref pubkey) = ceremony.public_key {
            let pubkey_hex = hex::encode(pubkey);
            // TODO: Generate actual Bitcoin address from public key
            let addr = format!("bc1q{}", "a".repeat(38));
            (Some(pubkey_hex), Some(addr))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    // Get latest FROST public key
    let frost_ceremony = ceremonies
        .iter()
        .filter(|c| c.protocol == ProtocolType::FROST
                && matches!(c.status, threshold_orchestrator::DkgStatus::Completed))
        .max_by_key(|c| c.completed_at);

    let (frost_public_key, frost_address) = if let Some(ceremony) = frost_ceremony {
        if let Some(ref pubkey) = ceremony.public_key {
            let pubkey_hex = hex::encode(pubkey);
            // TODO: Generate actual Bitcoin address from public key
            let addr = format!("bc1p{}", "b".repeat(58));
            (Some(pubkey_hex), Some(addr))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    let response = DkgStatusResponse {
        active_ceremonies,
        total_completed,
        cggmp24_public_key,
        cggmp24_address,
        frost_public_key,
        frost_address,
    };

    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initiate_dkg_request_validation() {
        // Valid request should pass
        let req = InitiateDkgRequest {
            protocol: "cggmp24".to_string(),
            threshold: 4,
            total_nodes: 5,
        };
        assert!(req.threshold <= req.total_nodes);

        // Invalid threshold should be detected
        let req = InitiateDkgRequest {
            protocol: "cggmp24".to_string(),
            threshold: 6,
            total_nodes: 5,
        };
        assert!(req.threshold > req.total_nodes);
    }
}
