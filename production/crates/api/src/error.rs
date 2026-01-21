//! Centralized error handling with proper HTTP status codes

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use threshold_types::Error as ThresholdError;

/// API Result type
pub type ApiResult<T> = Result<T, ApiError>;

/// API error types with appropriate HTTP status codes
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Internal server error: {0}")]
    InternalError(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),
}

impl ApiError {
    /// Get the appropriate HTTP status code for this error
    pub fn status_code(&self) -> StatusCode {
        match self {
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            ApiError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
        }
    }

    /// Get error type as string
    pub fn error_type(&self) -> &str {
        match self {
            ApiError::NotFound(_) => "not_found",
            ApiError::BadRequest(_) => "bad_request",
            ApiError::InternalError(_) => "internal_error",
            ApiError::ServiceUnavailable(_) => "service_unavailable",
            ApiError::Conflict(_) => "conflict",
            ApiError::Unauthorized(_) => "unauthorized",
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let error_type = self.error_type();
        let message = self.to_string();

        let body = Json(json!({
            "error": {
                "type": error_type,
                "message": message,
            }
        }));

        (status, body).into_response()
    }
}

// Convert threshold types errors to API errors
impl From<ThresholdError> for ApiError {
    fn from(err: ThresholdError) -> Self {
        match err {
            ThresholdError::TransactionNotFound(txid) => {
                ApiError::NotFound(format!("Transaction not found: {}", txid))
            }
            ThresholdError::NodeNotFound(node_id) => {
                ApiError::NotFound(format!("Node not found: {}", node_id))
            }
            ThresholdError::StorageError(msg) => ApiError::InternalError(msg),
            ThresholdError::NetworkError(msg) => ApiError::ServiceUnavailable(msg),
            ThresholdError::ConsensusFailed(msg) => ApiError::InternalError(msg),
            ThresholdError::SigningFailed(msg) => ApiError::InternalError(msg),
            ThresholdError::ByzantineViolation(msg) => ApiError::Conflict(msg),
            ThresholdError::InvalidVote(msg) => ApiError::BadRequest(msg),
            ThresholdError::Timeout(msg) => ApiError::ServiceUnavailable(msg),
            ThresholdError::ConfigError(msg) => ApiError::InternalError(msg),
            ThresholdError::CryptoError(msg) => ApiError::InternalError(msg),
            ThresholdError::NodeBanned { peer_id } => {
                ApiError::Unauthorized(format!("Node banned: {}", peer_id))
            }
            ThresholdError::TransactionAlreadyProcessed { tx_id } => {
                ApiError::Conflict(format!("Transaction already processed: {}", tx_id))
            }
            ThresholdError::Other(err) => ApiError::InternalError(err.to_string()),
        }
    }
}

// Convert anyhow errors to API errors
impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        ApiError::InternalError(err.to_string())
    }
}
