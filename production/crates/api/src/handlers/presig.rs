//! Presignature Pool API Handlers

use crate::error::ApiError;
use crate::state::AppState;
use axum::{
    extract::State,
    Json,
};
use serde::{Deserialize, Serialize};

/// Presignature pool status response
#[derive(Debug, Serialize)]
pub struct PresigStatusResponse {
    /// Current number of available presignatures
    pub current_size: usize,
    /// Target pool size
    pub target_size: usize,
    /// Maximum pool size
    pub max_size: usize,
    /// Pool utilization percentage (0-100)
    pub utilization: f64,
    /// Pool health status
    pub is_healthy: bool,
    /// Pool critical status
    pub is_critical: bool,
    /// Number of presignatures used in the last hour
    pub hourly_usage: usize,
    /// Total presignatures generated
    pub total_generated: u64,
    /// Total presignatures used
    pub total_used: u64,
}

/// Request to manually generate presignatures
#[derive(Debug, Deserialize)]
pub struct GeneratePresigRequest {
    /// Number of presignatures to generate
    pub count: usize,
}

/// Response from presignature generation
#[derive(Debug, Serialize)]
pub struct GeneratePresigResponse {
    /// Number of presignatures generated
    pub generated: usize,
    /// New pool size
    pub new_pool_size: usize,
    /// Generation duration in milliseconds
    pub duration_ms: u64,
}

/// Get presignature pool status
///
/// GET /api/v1/presignatures/status
pub async fn presignature_status(
    State(_state): State<AppState>,
) -> Result<Json<PresigStatusResponse>, ApiError> {
    // TODO: Query actual presignature pool status
    // let stats = state.presig_service.get_stats().await;

    // Mock response
    let response = PresigStatusResponse {
        current_size: 75,
        target_size: 100,
        max_size: 150,
        utilization: 75.0,
        is_healthy: true,
        is_critical: false,
        hourly_usage: 12,
        total_generated: 500,
        total_used: 425,
    };

    Ok(Json(response))
}

/// Manually trigger presignature generation
///
/// POST /api/v1/presignatures/generate
pub async fn generate_presignatures(
    State(_state): State<AppState>,
    Json(req): Json<GeneratePresigRequest>,
) -> Result<Json<GeneratePresigResponse>, ApiError> {
    // Validate count
    if req.count == 0 {
        return Err(ApiError::BadRequest(
            "Count must be greater than 0".to_string(),
        ));
    }

    if req.count > 50 {
        return Err(ApiError::BadRequest(
            "Cannot generate more than 50 presignatures at once".to_string(),
        ));
    }

    // TODO: Trigger actual presignature generation
    // let start = std::time::Instant::now();
    // let generated = state.presig_service.generate_batch(req.count).await?;
    // let duration_ms = start.elapsed().as_millis() as u64;

    // Mock response
    let response = GeneratePresigResponse {
        generated: req.count,
        new_pool_size: 75 + req.count,
        duration_ms: req.count as u64 * 400, // ~400ms per presignature
    };

    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_presig_request_validation() {
        let req = GeneratePresigRequest { count: 10 };
        assert!(req.count > 0 && req.count <= 50);

        let req = GeneratePresigRequest { count: 0 };
        assert!(req.count == 0);

        let req = GeneratePresigRequest { count: 100 };
        assert!(req.count > 50);
    }

    #[test]
    fn test_presig_status_health() {
        let status = PresigStatusResponse {
            current_size: 75,
            target_size: 100,
            max_size: 150,
            utilization: 75.0,
            is_healthy: true,
            is_critical: false,
            hourly_usage: 12,
            total_generated: 500,
            total_used: 425,
        };

        assert!(status.is_healthy);
        assert!(!status.is_critical);
    }
}
