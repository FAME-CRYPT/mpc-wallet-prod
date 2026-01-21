use anyhow::{Context, Result};
use threshold_types::TransactionState;

/// Custom assertions for E2E tests

pub fn assert_transaction_state(
    actual: &TransactionState,
    expected: &TransactionState,
    context: &str,
) -> Result<()> {
    if actual != expected {
        anyhow::bail!(
            "Transaction state mismatch in {}: expected {:?}, got {:?}",
            context,
            expected,
            actual
        );
    }
    Ok(())
}

pub fn assert_in_range(actual: u64, min: u64, max: u64, context: &str) -> Result<()> {
    if actual < min || actual > max {
        anyhow::bail!(
            "Value out of range in {}: expected {}..{}, got {}",
            context,
            min,
            max,
            actual
        );
    }
    Ok(())
}

pub fn assert_threshold_reached(vote_count: u64, threshold: u64, context: &str) -> Result<()> {
    if vote_count < threshold {
        anyhow::bail!(
            "Threshold not reached in {}: {} votes < {} threshold",
            context,
            vote_count,
            threshold
        );
    }
    Ok(())
}

pub fn assert_node_banned(is_banned: bool, context: &str) -> Result<()> {
    if !is_banned {
        anyhow::bail!("Node should be banned in {}", context);
    }
    Ok(())
}

pub fn assert_contains(text: &str, pattern: &str, context: &str) -> Result<()> {
    if !text.contains(pattern) {
        anyhow::bail!(
            "Expected pattern '{}' not found in {}: {}",
            pattern,
            context,
            text
        );
    }
    Ok(())
}

pub fn assert_metric_exists(metrics: &str, metric_name: &str) -> Result<()> {
    if !metrics.contains(metric_name) {
        anyhow::bail!("Metric '{}' not found in metrics output", metric_name);
    }
    Ok(())
}

pub fn assert_metric_value(
    metrics: &str,
    metric_name: &str,
    min_value: f64,
) -> Result<()> {
    // Simple Prometheus metrics parser
    for line in metrics.lines() {
        if line.starts_with(metric_name) && !line.starts_with('#') {
            if let Some(value_str) = line.split_whitespace().last() {
                if let Ok(value) = value_str.parse::<f64>() {
                    if value >= min_value {
                        return Ok(());
                    } else {
                        anyhow::bail!(
                            "Metric '{}' value {} is below minimum {}",
                            metric_name,
                            value,
                            min_value
                        );
                    }
                }
            }
        }
    }
    anyhow::bail!("Metric '{}' not found or could not be parsed", metric_name);
}

pub fn assert_no_errors_in_logs(logs: &str, service: &str) -> Result<()> {
    let error_patterns = vec!["ERROR", "FATAL", "panic", "thread panicked"];

    for pattern in error_patterns {
        if logs.contains(pattern) {
            anyhow::bail!(
                "Found error pattern '{}' in {} logs:\n{}",
                pattern,
                service,
                logs
            );
        }
    }
    Ok(())
}
