//! Presignature Pool API Routes

use crate::handlers::presig;
use crate::state::AppState;
use axum::{
    routing::{get, post},
    Router,
};

/// Create presignature routes
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/status", get(presig::presignature_status))
        .route("/generate", post(presig::generate_presignatures))
}
