//! REST API server for MPC Wallet
//!
//! Production-ready API with Axum framework providing:
//! - Transaction management (create, get, list)
//! - Wallet operations (balance, address)
//! - Cluster monitoring (health, nodes)
//! - CORS middleware for cross-origin requests
//! - Request logging with tracing
//! - Comprehensive error handling

use axum::{
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use tower::ServiceBuilder;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::info;

pub mod error;
pub mod handlers;
pub mod routes;
pub mod state;
pub mod middleware;

pub use error::{ApiError, ApiResult};
pub use state::AppState;
pub use middleware::{RateLimiter, RateLimitConfig};

/// Create and configure the API router with all endpoints
pub fn create_router(state: AppState) -> Router {
    // Create CORS middleware - allow all origins for development
    // In production, configure specific origins
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Create the main API router
    let api_v1 = Router::new()
        // Transaction endpoints
        .route("/transactions", post(routes::transactions::create_transaction))
        .route("/transactions", get(routes::transactions::list_transactions))
        .route("/transactions/:txid", get(routes::transactions::get_transaction))
        // Wallet endpoints
        .route("/wallet/balance", get(routes::wallet::get_balance))
        .route("/wallet/address", get(routes::wallet::get_address))
        // Cluster endpoints
        .route("/cluster/status", get(routes::cluster::get_cluster_status))
        .route("/cluster/nodes", get(routes::cluster::list_nodes))
        // DKG endpoints
        .nest("/dkg", routes::dkg::routes())
        // Aux info endpoints
        .nest("/aux-info", routes::aux_info::routes())
        // Presignature pool endpoints
        .nest("/presignatures", routes::presig::routes());

    // Build the complete router with middleware
    Router::new()
        .route("/health", get(routes::health::health_check))
        .nest("/api/v1", api_v1)
        .nest("/internal", routes::internal::routes())
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(cors),
        )
        .with_state(state)
}

/// Start the API server on the specified address
pub async fn start_server(state: AppState, addr: SocketAddr) -> anyhow::Result<()> {
    let app = create_router(state);

    info!("Starting API server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(listener, app)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "AppState::default() should not be used in production")]
    fn test_default_panics() {
        // Ensure that AppState::default() panics as expected
        let _state = AppState::default();
    }
}
