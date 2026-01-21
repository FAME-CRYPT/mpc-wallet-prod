use lazy_static::lazy_static;
use prometheus::{
    register_counter_vec, register_gauge_vec, register_histogram_vec, CounterVec, Encoder,
    GaugeVec, HistogramVec, TextEncoder,
};

lazy_static! {
    // Transaction metrics
    pub static ref TRANSACTIONS_TOTAL: CounterVec = register_counter_vec!(
        "mpc_transactions_total",
        "Total number of transactions by state",
        &["state", "node_id"]
    )
    .expect("Failed to create mpc_transactions_total metric");

    // Voting metrics
    pub static ref VOTES_TOTAL: CounterVec = register_counter_vec!(
        "mpc_votes_total",
        "Total number of votes cast",
        &["node_id", "result"]
    )
    .expect("Failed to create mpc_votes_total metric");

    // Byzantine violation metrics
    pub static ref BYZANTINE_VIOLATIONS_TOTAL: CounterVec = register_counter_vec!(
        "mpc_byzantine_violations_total",
        "Total number of Byzantine violations detected",
        &["type", "node_id"]
    )
    .expect("Failed to create mpc_byzantine_violations_total metric");

    // Signature metrics
    pub static ref SIGNATURES_TOTAL: CounterVec = register_counter_vec!(
        "mpc_signatures_total",
        "Total number of signatures generated",
        &["protocol", "result", "node_id"]
    )
    .expect("Failed to create mpc_signatures_total metric");

    // Network error metrics
    pub static ref NETWORK_ERRORS_TOTAL: CounterVec = register_counter_vec!(
        "mpc_network_errors_total",
        "Total number of network errors",
        &["node_id", "error_type"]
    )
    .expect("Failed to create mpc_network_errors_total metric");

    // TLS handshake failure metrics
    pub static ref TLS_HANDSHAKE_FAILURES_TOTAL: CounterVec = register_counter_vec!(
        "mpc_tls_handshake_failures_total",
        "Total number of TLS handshake failures",
        &["node_id"]
    )
    .expect("Failed to create mpc_tls_handshake_failures_total metric");

    // Gauge metrics
    pub static ref ACTIVE_NODES: GaugeVec = register_gauge_vec!(
        "mpc_active_nodes",
        "Number of active MPC nodes",
        &["cluster"]
    )
    .expect("Failed to create mpc_active_nodes metric");

    pub static ref CONSENSUS_THRESHOLD: GaugeVec = register_gauge_vec!(
        "mpc_consensus_threshold",
        "Required number of votes for consensus",
        &["cluster"]
    )
    .expect("Failed to create mpc_consensus_threshold metric");

    pub static ref PRESIGNATURE_POOL_SIZE: GaugeVec = register_gauge_vec!(
        "mpc_presignature_pool_size",
        "Number of presignatures available in the pool",
        &["node_id", "protocol"]
    )
    .expect("Failed to create mpc_presignature_pool_size metric");

    pub static ref CONNECTED_PEERS: GaugeVec = register_gauge_vec!(
        "mpc_connected_peers",
        "Number of connected peer nodes",
        &["node_id"]
    )
    .expect("Failed to create mpc_connected_peers metric");

    pub static ref TRANSACTION_QUEUE_SIZE: GaugeVec = register_gauge_vec!(
        "mpc_transaction_queue_size",
        "Number of transactions in the queue",
        &["node_id"]
    )
    .expect("Failed to create mpc_transaction_queue_size metric");

    // Histogram metrics for latency/duration tracking
    pub static ref SIGNATURE_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "mpc_signature_duration_seconds",
        "Time taken to generate a signature",
        &["protocol", "node_id"],
        vec![0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0]
    )
    .expect("Failed to create mpc_signature_duration_seconds metric");

    pub static ref DKG_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "mpc_dkg_duration_seconds",
        "Time taken to complete a DKG ceremony",
        &["protocol", "node_id"],
        vec![1.0, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0]
    )
    .expect("Failed to create mpc_dkg_duration_seconds metric");

    pub static ref VOTE_PROCESSING_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "mpc_vote_processing_duration_seconds",
        "Time taken to process a vote",
        &["node_id"],
        vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0]
    )
    .expect("Failed to create mpc_vote_processing_duration_seconds metric");

    pub static ref CONSENSUS_ROUND_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "mpc_consensus_round_duration_seconds",
        "Time taken to complete a consensus round",
        &["node_id"],
        vec![0.5, 1.0, 5.0, 10.0, 30.0, 60.0, 120.0]
    )
    .expect("Failed to create mpc_consensus_round_duration_seconds metric");
}

/// Initialize metrics with default values for the given node
pub fn initialize_metrics(node_id: &str, threshold: u32, total_nodes: u32) {
    // Set consensus parameters
    CONSENSUS_THRESHOLD
        .with_label_values(&["mpc-wallet"])
        .set(threshold as f64);

    ACTIVE_NODES
        .with_label_values(&["mpc-wallet"])
        .set(total_nodes as f64);

    // Initialize gauges to zero
    CONNECTED_PEERS.with_label_values(&[node_id]).set(0.0);
    TRANSACTION_QUEUE_SIZE.with_label_values(&[node_id]).set(0.0);

    // Initialize presignature pools
    PRESIGNATURE_POOL_SIZE
        .with_label_values(&[node_id, "cggmp24"])
        .set(0.0);
    PRESIGNATURE_POOL_SIZE
        .with_label_values(&[node_id, "frost"])
        .set(0.0);
}

/// Record a transaction state change
pub fn record_transaction(node_id: &str, state: &str) {
    TRANSACTIONS_TOTAL
        .with_label_values(&[state, node_id])
        .inc();
}

/// Record a vote
pub fn record_vote(node_id: &str, result: &str) {
    VOTES_TOTAL.with_label_values(&[node_id, result]).inc();
}

/// Record a Byzantine violation
pub fn record_byzantine_violation(node_id: &str, violation_type: &str) {
    BYZANTINE_VIOLATIONS_TOTAL
        .with_label_values(&[violation_type, node_id])
        .inc();
}

/// Record a signature generation (success or failure)
pub fn record_signature(node_id: &str, protocol: &str, result: &str, duration_secs: f64) {
    SIGNATURES_TOTAL
        .with_label_values(&[protocol, result, node_id])
        .inc();

    SIGNATURE_DURATION_SECONDS
        .with_label_values(&[protocol, node_id])
        .observe(duration_secs);
}

/// Record a DKG ceremony completion
pub fn record_dkg(node_id: &str, protocol: &str, duration_secs: f64) {
    DKG_DURATION_SECONDS
        .with_label_values(&[protocol, node_id])
        .observe(duration_secs);
}

/// Record vote processing time
pub fn record_vote_processing(node_id: &str, duration_secs: f64) {
    VOTE_PROCESSING_DURATION_SECONDS
        .with_label_values(&[node_id])
        .observe(duration_secs);
}

/// Record consensus round duration
pub fn record_consensus_round(node_id: &str, duration_secs: f64) {
    CONSENSUS_ROUND_DURATION_SECONDS
        .with_label_values(&[node_id])
        .observe(duration_secs);
}

/// Record a network error
pub fn record_network_error(node_id: &str, error_type: &str) {
    NETWORK_ERRORS_TOTAL
        .with_label_values(&[node_id, error_type])
        .inc();
}

/// Record a TLS handshake failure
pub fn record_tls_handshake_failure(node_id: &str) {
    TLS_HANDSHAKE_FAILURES_TOTAL
        .with_label_values(&[node_id])
        .inc();
}

/// Update the number of connected peers
pub fn update_connected_peers(node_id: &str, count: usize) {
    CONNECTED_PEERS
        .with_label_values(&[node_id])
        .set(count as f64);
}

/// Update the transaction queue size
pub fn update_transaction_queue_size(node_id: &str, size: usize) {
    TRANSACTION_QUEUE_SIZE
        .with_label_values(&[node_id])
        .set(size as f64);
}

/// Update the presignature pool size
pub fn update_presignature_pool_size(node_id: &str, protocol: &str, size: usize) {
    PRESIGNATURE_POOL_SIZE
        .with_label_values(&[node_id, protocol])
        .set(size as f64);
}

/// Gather and encode all metrics in Prometheus text format
pub fn gather_metrics() -> Result<String, Box<dyn std::error::Error>> {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer)?;
    Ok(String::from_utf8(buffer)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize_metrics() {
        initialize_metrics("test-node", 3, 5);
        let metrics = gather_metrics().unwrap();
        assert!(metrics.contains("mpc_consensus_threshold"));
        assert!(metrics.contains("mpc_active_nodes"));
    }

    #[test]
    fn test_record_transaction() {
        record_transaction("test-node", "pending");
        let metrics = gather_metrics().unwrap();
        assert!(metrics.contains("mpc_transactions_total"));
    }

    #[test]
    fn test_record_vote() {
        record_vote("test-node", "approve");
        let metrics = gather_metrics().unwrap();
        assert!(metrics.contains("mpc_votes_total"));
    }

    #[test]
    fn test_record_byzantine_violation() {
        record_byzantine_violation("test-node", "double_vote");
        let metrics = gather_metrics().unwrap();
        assert!(metrics.contains("mpc_byzantine_violations_total"));
    }

    #[test]
    fn test_record_signature() {
        record_signature("test-node", "cggmp24", "success", 2.5);
        let metrics = gather_metrics().unwrap();
        assert!(metrics.contains("mpc_signatures_total"));
        assert!(metrics.contains("mpc_signature_duration_seconds"));
    }

    #[test]
    fn test_update_gauges() {
        update_connected_peers("test-node", 4);
        update_transaction_queue_size("test-node", 10);
        update_presignature_pool_size("test-node", "frost", 25);

        let metrics = gather_metrics().unwrap();
        assert!(metrics.contains("mpc_connected_peers"));
        assert!(metrics.contains("mpc_transaction_queue_size"));
        assert!(metrics.contains("mpc_presignature_pool_size"));
    }
}
