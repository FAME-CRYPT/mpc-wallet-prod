//! Prometheus Metrics for Orchestrator
//!
//! This module defines and exports Prometheus metrics for monitoring the
//! MPC wallet orchestration system.

use lazy_static::lazy_static;
use prometheus::{
    register_histogram, register_int_counter_vec, register_int_gauge, register_int_gauge_vec,
    Histogram, IntCounterVec, IntGauge, IntGaugeVec,
};

lazy_static! {
    /// Presignature pool size (number of available presignatures)
    pub static ref PRESIG_POOL_SIZE: IntGauge = register_int_gauge!(
        "presignature_pool_size",
        "Number of presignatures available in the pool"
    )
    .expect("Failed to register presignature_pool_size metric");

    /// Presignature pool target size
    pub static ref PRESIG_POOL_TARGET: IntGauge = register_int_gauge!(
        "presignature_pool_target",
        "Target number of presignatures in the pool"
    )
    .expect("Failed to register presignature_pool_target metric");

    /// Total presignatures generated
    pub static ref PRESIG_GENERATED_TOTAL: IntGauge = register_int_gauge!(
        "presignature_generated_total",
        "Total number of presignatures generated since startup"
    )
    .expect("Failed to register presignature_generated_total metric");

    /// Total presignatures used
    pub static ref PRESIG_USED_TOTAL: IntGauge = register_int_gauge!(
        "presignature_used_total",
        "Total number of presignatures used for signing"
    )
    .expect("Failed to register presignature_used_total metric");

    /// MPC signing duration histogram (seconds)
    pub static ref SIGNING_DURATION: Histogram = register_histogram!(
        "mpc_signing_duration_seconds",
        "MPC signing duration in seconds"
    )
    .expect("Failed to register mpc_signing_duration_seconds metric");

    /// Transaction state counts (gauge per state)
    pub static ref TX_STATE_COUNT: IntGaugeVec = register_int_gauge_vec!(
        "transaction_state_count",
        "Number of transactions in each state",
        &["state"]
    )
    .expect("Failed to register transaction_state_count metric");

    /// Transaction state transitions (counter)
    pub static ref TX_STATE_TRANSITIONS: IntCounterVec = register_int_counter_vec!(
        "transaction_state_transitions_total",
        "Total number of transaction state transitions",
        &["from_state", "to_state"]
    )
    .expect("Failed to register transaction_state_transitions_total metric");

    /// MPC signing success/failure counter
    pub static ref SIGNING_RESULT: IntCounterVec = register_int_counter_vec!(
        "mpc_signing_result_total",
        "MPC signing results (success/failure) by protocol",
        &["protocol", "result"]
    )
    .expect("Failed to register mpc_signing_result_total metric");

    /// Error counter by type
    pub static ref ERROR_COUNT: IntCounterVec = register_int_counter_vec!(
        "orchestrator_error_total",
        "Total number of errors by type",
        &["error_type"]
    )
    .expect("Failed to register orchestrator_error_total metric");

    /// DKG ceremony count by protocol and result
    pub static ref DKG_CEREMONY_RESULT: IntCounterVec = register_int_counter_vec!(
        "dkg_ceremony_result_total",
        "DKG ceremony results by protocol",
        &["protocol", "result"]
    )
    .expect("Failed to register dkg_ceremony_result_total metric");

    /// Active signing sessions
    pub static ref ACTIVE_SIGNING_SESSIONS: IntGauge = register_int_gauge!(
        "active_signing_sessions",
        "Number of active MPC signing sessions"
    )
    .expect("Failed to register active_signing_sessions metric");

    /// Orchestration loop iterations
    pub static ref ORCHESTRATION_ITERATIONS: IntGauge = register_int_gauge!(
        "orchestration_iterations_total",
        "Total number of orchestration loop iterations"
    )
    .expect("Failed to register orchestration_iterations_total metric");
}

/// Update presignature pool metrics
pub fn update_presig_pool_metrics(current: usize, target: usize, generated: u64, used: u64) {
    PRESIG_POOL_SIZE.set(current as i64);
    PRESIG_POOL_TARGET.set(target as i64);
    PRESIG_GENERATED_TOTAL.set(generated as i64);
    PRESIG_USED_TOTAL.set(used as i64);
}

/// Update transaction state metrics
pub fn update_tx_state_metrics(
    pending: usize,
    voting: usize,
    approved: usize,
    signing: usize,
    signed: usize,
    broadcasting: usize,
    confirmed: usize,
) {
    TX_STATE_COUNT.with_label_values(&["pending"]).set(pending as i64);
    TX_STATE_COUNT.with_label_values(&["voting"]).set(voting as i64);
    TX_STATE_COUNT.with_label_values(&["approved"]).set(approved as i64);
    TX_STATE_COUNT.with_label_values(&["signing"]).set(signing as i64);
    TX_STATE_COUNT.with_label_values(&["signed"]).set(signed as i64);
    TX_STATE_COUNT.with_label_values(&["broadcasting"]).set(broadcasting as i64);
    TX_STATE_COUNT.with_label_values(&["confirmed"]).set(confirmed as i64);
}

/// Record transaction state transition
pub fn record_tx_state_transition(from_state: &str, to_state: &str) {
    TX_STATE_TRANSITIONS
        .with_label_values(&[from_state, to_state])
        .inc();
}

/// Record MPC signing result
pub fn record_signing_result(protocol: &str, success: bool) {
    let result = if success { "success" } else { "failure" };
    SIGNING_RESULT
        .with_label_values(&[protocol, result])
        .inc();
}

/// Record error
pub fn record_error(error_type: &str) {
    ERROR_COUNT.with_label_values(&[error_type]).inc();
}

/// Record DKG ceremony result
pub fn record_dkg_result(protocol: &str, success: bool) {
    let result = if success { "success" } else { "failure" };
    DKG_CEREMONY_RESULT
        .with_label_values(&[protocol, result])
        .inc();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_registration() {
        // Just verify metrics can be accessed without panicking
        let _ = &*PRESIG_POOL_SIZE;
        let _ = &*SIGNING_DURATION;
        let _ = &*TX_STATE_COUNT;
    }
}
