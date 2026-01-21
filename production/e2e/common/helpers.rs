use anyhow::Result;
use std::time::Duration;
use tokio::time::sleep;
use tracing::debug;

/// Retry a function with exponential backoff
pub async fn retry_with_backoff<F, Fut, T>(
    mut f: F,
    max_attempts: u32,
    initial_delay: Duration,
    context: &str,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut delay = initial_delay;

    for attempt in 1..=max_attempts {
        debug!(
            "Attempt {}/{} for {}",
            attempt, max_attempts, context
        );

        match f().await {
            Ok(result) => return Ok(result),
            Err(e) if attempt == max_attempts => {
                return Err(e).with_context(|| {
                    format!(
                        "Failed after {} attempts for {}", max_attempts, context
                    )
                });
            }
            Err(e) => {
                debug!(
                    "Attempt {}/{} failed for {}: {}. Retrying in {:?}...",
                    attempt, max_attempts, context, e, delay
                );
                sleep(delay).await;
                delay *= 2; // Exponential backoff
            }
        }
    }

    unreachable!()
}

/// Wait until a condition is true
pub async fn wait_until<F, Fut>(
    mut condition: F,
    timeout: Duration,
    check_interval: Duration,
    context: &str,
) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        if condition().await {
            debug!("Condition met for {}", context);
            return Ok(());
        }

        debug!(
            "Waiting for condition in {} (elapsed: {:?})",
            context,
            start.elapsed()
        );

        sleep(check_interval).await;
    }

    anyhow::bail!(
        "Timeout waiting for condition in {} after {:?}",
        context,
        timeout
    )
}

/// Generate a test Bitcoin address (testnet)
pub fn generate_test_address() -> String {
    // Generate a simple testnet P2WPKH address for testing
    // In production, use proper Bitcoin address generation
    format!("tb1q{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..32].to_lowercase())
}

/// Parse Prometheus metrics
pub fn parse_metric_value(metrics: &str, metric_name: &str) -> Option<f64> {
    for line in metrics.lines() {
        if line.starts_with(metric_name) && !line.starts_with('#') {
            if let Some(value_str) = line.split_whitespace().last() {
                if let Ok(value) = value_str.parse::<f64>() {
                    return Some(value);
                }
            }
        }
    }
    None
}

/// Calculate percentile from sorted values
pub fn calculate_percentile(sorted_values: &[f64], percentile: f64) -> f64 {
    if sorted_values.is_empty() {
        return 0.0;
    }

    let index = (percentile / 100.0 * sorted_values.len() as f64).ceil() as usize - 1;
    sorted_values[index.min(sorted_values.len() - 1)]
}

/// Generate unique test identifier
pub fn test_id() -> String {
    format!("test-{}", uuid::Uuid::new_v4().to_string()[..8].to_string())
}

use anyhow::Context as _;
