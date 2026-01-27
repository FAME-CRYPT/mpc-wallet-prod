//! Internal API routes for node-to-node communication

use crate::handlers::internal;
use crate::state::AppState;
use axum::{routing::post, Router};

/// Create internal routes
pub fn routes() -> Router<AppState> {
    Router::new().route("/vote-request", post(internal::receive_vote_request))
}
