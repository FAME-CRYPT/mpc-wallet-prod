use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

/// Handler for the /metrics endpoint
/// This endpoint is scraped by Prometheus to collect metrics
pub async fn metrics_handler() -> Response {
    match crate::metrics::gather_metrics() {
        Ok(metrics) => (StatusCode::OK, metrics).into_response(),
        Err(e) => {
            tracing::error!("Failed to gather metrics: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to gather metrics").into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_handler() {
        // Initialize metrics
        crate::metrics::initialize_metrics("test-node", 3, 5);

        // Call handler
        let response = metrics_handler().await;

        // Verify response
        assert_eq!(response.status(), StatusCode::OK);
    }
}
