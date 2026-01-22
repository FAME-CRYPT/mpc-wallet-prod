//! Aux Info API routes

use axum::{routing::get, routing::post, Router};

use crate::handlers::aux_info::{aux_info_status, generate_aux_info};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/generate", post(generate_aux_info))
        .route("/status", get(aux_info_status))
}
