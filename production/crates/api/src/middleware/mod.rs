//! API Middleware modules
//!
//! Provides authentication, rate limiting, and other middleware

pub mod auth;
pub mod rate_limit;

pub use auth::{jwt_auth, is_public_endpoint, Claims};
pub use rate_limit::{rate_limit, RateLimiter, RateLimitConfig};
