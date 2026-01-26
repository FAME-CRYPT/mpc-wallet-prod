//! DKG API Routes

use crate::handlers::dkg;
use crate::state::AppState;
use axum::{
    routing::{get, post},
    Router,
};

/// Create DKG routes
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/initiate", post(dkg::initiate_dkg))
        .route("/join/:session_id", post(dkg::join_dkg))
        .route("/status", get(dkg::dkg_status))
}
