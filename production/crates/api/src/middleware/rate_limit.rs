//! Rate Limiting Middleware
//!
//! Implements token bucket algorithm for rate limiting API requests

use axum::{
    body::Body,
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::warn;

/// Rate limiter configuration
#[derive(Clone, Debug)]
pub struct RateLimitConfig {
    /// Maximum requests per time window
    pub max_requests: usize,
    /// Time window duration
    pub window: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window: Duration::from_secs(60), // 100 requests per minute
        }
    }
}

/// Token bucket for rate limiting
#[derive(Debug)]
struct TokenBucket {
    /// Available tokens
    tokens: usize,
    /// Last time tokens were refilled
    last_refill: Instant,
}

/// Rate limiter state
pub struct RateLimiter {
    /// Token buckets per client IP
    buckets: Arc<Mutex<HashMap<String, TokenBucket>>>,
    /// Configuration
    config: RateLimitConfig,
}

impl RateLimiter {
    /// Create a new rate limiter with the given configuration
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            buckets: Arc::new(Mutex::new(HashMap::new())),
            config,
        }
    }

    /// Check if request from client_ip is allowed
    ///
    /// Returns true if request is allowed, false if rate limit exceeded
    async fn check_rate_limit(&self, client_ip: &str) -> bool {
        let mut buckets = self.buckets.lock().await;
        let now = Instant::now();

        let bucket = buckets.entry(client_ip.to_string()).or_insert(TokenBucket {
            tokens: self.config.max_requests,
            last_refill: now,
        });

        // Refill tokens based on time elapsed
        let elapsed = now.duration_since(bucket.last_refill);
        if elapsed >= self.config.window {
            bucket.tokens = self.config.max_requests;
            bucket.last_refill = now;
        }

        // Check if tokens available
        if bucket.tokens > 0 {
            bucket.tokens -= 1;
            true
        } else {
            false
        }
    }

    /// Cleanup old entries (run periodically to prevent memory leaks)
    pub async fn cleanup_old_entries(&self) {
        let mut buckets = self.buckets.lock().await;
        let now = Instant::now();
        let cleanup_threshold = self.config.window * 2; // Keep entries for 2x window

        buckets.retain(|_, bucket| {
            now.duration_since(bucket.last_refill) < cleanup_threshold
        });
    }
}

/// Rate limiting middleware
///
/// Limits requests per client IP based on token bucket algorithm
pub async fn rate_limit(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(limiter): State<Arc<RateLimiter>>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let client_ip = addr.ip().to_string();

    if limiter.check_rate_limit(&client_ip).await {
        Ok(next.run(req).await)
    } else {
        warn!("Rate limit exceeded for client: {}", client_ip);
        Err(StatusCode::TOO_MANY_REQUESTS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_allows_within_limit() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 10,
            window: Duration::from_secs(60),
        });

        // First 10 requests should be allowed
        for _ in 0..10 {
            assert!(limiter.check_rate_limit("127.0.0.1").await);
        }

        // 11th request should be blocked
        assert!(!limiter.check_rate_limit("127.0.0.1").await);
    }

    #[tokio::test]
    async fn test_rate_limiter_refills_after_window() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 1,
            window: Duration::from_millis(100),
        });

        // First request allowed
        assert!(limiter.check_rate_limit("127.0.0.1").await);

        // Second request blocked
        assert!(!limiter.check_rate_limit("127.0.0.1").await);

        // Wait for window to pass
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Third request should be allowed after refill
        assert!(limiter.check_rate_limit("127.0.0.1").await);
    }

    #[tokio::test]
    async fn test_rate_limiter_separate_clients() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_requests: 1,
            window: Duration::from_secs(60),
        });

        // First client
        assert!(limiter.check_rate_limit("127.0.0.1").await);
        assert!(!limiter.check_rate_limit("127.0.0.1").await);

        // Second client should have separate limit
        assert!(limiter.check_rate_limit("127.0.0.2").await);
    }
}
