//! Health check endpoint

use axum::{extract::State, Json};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{state::AppState, ApiResult};

/// Health check response
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: chrono::DateTime<Utc>,
    pub version: String,
}

/// GET /health - Health check endpoint
///
/// Returns the current health status of the API server
pub async fn health_check(State(_state): State<AppState>) -> ApiResult<Json<HealthResponse>> {
    Ok(Json(HealthResponse {
        status: "healthy".to_string(),
        timestamp: Utc::now(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }))
}
