//! JWT Authentication Middleware
//!
//! Provides JWT-based authentication for API endpoints

use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use tracing::warn;

/// JWT claims structure
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID or node ID)
    pub sub: String,
    /// Role (admin, user, node)
    pub role: String,
    /// Expiration time (Unix timestamp)
    pub exp: usize,
}

/// JWT authentication middleware
///
/// Validates the Bearer token in the Authorization header
pub async fn jwt_auth(
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Extract Authorization header
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok());

    let token = match auth_header {
        Some(header) if header.starts_with("Bearer ") => {
            &header[7..] // Remove "Bearer " prefix
        }
        _ => {
            warn!("Missing or invalid Authorization header");
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    // Validate JWT
    // In production, load secret from environment variable
    let secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| {
            warn!("JWT_SECRET not set, using development key (INSECURE)");
            "dev-secret-key-change-in-production".to_string()
        });

    let decoding_key = DecodingKey::from_secret(secret.as_bytes());
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true; // Validate expiration

    match decode::<Claims>(token, &decoding_key, &validation) {
        Ok(token_data) => {
            // Authentication successful
            // You could add claims to request extensions here if needed
            tracing::info!("Authenticated user: {} with role: {}", token_data.claims.sub, token_data.claims.role);
            Ok(next.run(req).await)
        }
        Err(e) => {
            warn!("JWT validation failed: {}", e);
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

/// Check if an endpoint is public (doesn't require authentication)
pub fn is_public_endpoint(path: &str) -> bool {
    matches!(
        path,
        "/health" | "/api/v1/cluster/status" | "/api/v1/wallet/address"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_endpoint_detection() {
        assert!(is_public_endpoint("/health"));
        assert!(is_public_endpoint("/api/v1/cluster/status"));
        assert!(!is_public_endpoint("/api/v1/transactions"));
    }
}
