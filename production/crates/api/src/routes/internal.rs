//! Internal API routes for node-to-node communication

use crate::handlers::internal;
use crate::state::AppState;
use axum::{routing::post, Router};

/// Create internal routes
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/vote-request", post(internal::receive_vote_request))
        .route("/dkg-join", post(internal::receive_dkg_join_request))
        .route("/aux-info-join", post(internal::receive_aux_info_join_request))
        .route("/signing-join", post(internal::receive_signing_join_request))
}
