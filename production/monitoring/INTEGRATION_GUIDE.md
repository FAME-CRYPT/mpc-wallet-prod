# Metrics Integration Guide

This guide shows how to integrate Prometheus metrics into the MPC wallet API.

## Table of Contents

1. [Adding Metrics to Your Code](#adding-metrics-to-your-code)
2. [API Route Configuration](#api-route-configuration)
3. [Examples by Component](#examples-by-component)
4. [Testing Metrics](#testing-metrics)
5. [Best Practices](#best-practices)

## Adding Metrics to Your Code

### Step 1: Import the metrics module

Add to your `src/lib.rs`:

```rust
pub mod metrics;
```

### Step 2: Add metrics handler to your module

Create `src/handlers/mod.rs` if it doesn't exist:

```rust
pub mod metrics_handler;
```

### Step 3: Configure routes

In your `src/bin/server.rs` or main application file:

```rust
use axum::{
    routing::get,
    Router,
};
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Get node configuration
    let node_id = std::env::var("NODE_ID").unwrap_or_else(|_| "1".to_string());
    let threshold = std::env::var("THRESHOLD")
        .unwrap_or_else(|_| "4".to_string())
        .parse::<u32>()?;
    let total_nodes = std::env::var("TOTAL_NODES")
        .unwrap_or_else(|_| "5".to_string())
        .parse::<u32>()?;

    // Initialize metrics
    threshold_api::metrics::initialize_metrics(&node_id, threshold, total_nodes);

    // Build application routes
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/metrics", get(threshold_api::handlers::metrics_handler::metrics_handler))
        .route("/api/v1/transactions", post(create_transaction))
        .route("/api/v1/transactions/:id", get(get_transaction))
        .route("/api/v1/vote", post(cast_vote))
        .layer(TraceLayer::new_for_http());

    // Start server
    let addr = "0.0.0.0:8080";
    tracing::info!("Starting server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_check() -> &'static str {
    "OK"
}
```

## Examples by Component

### 1. Transaction Processing

```rust
use threshold_api::metrics;
use std::time::Instant;

pub async fn create_transaction(
    transaction_data: TransactionRequest,
) -> Result<TransactionResponse, Error> {
    let node_id = get_node_id(); // Your function to get node ID

    // Record transaction as pending
    metrics::record_transaction(&node_id, "pending");

    // Update queue size
    let queue_size = get_transaction_queue_size(); // Your function
    metrics::update_transaction_queue_size(&node_id, queue_size);

    // Process transaction
    match process_transaction_internal(transaction_data).await {
        Ok(tx) => {
            metrics::record_transaction(&node_id, "approved");
            Ok(tx)
        }
        Err(e) => {
            metrics::record_transaction(&node_id, "failed");
            Err(e)
        }
    }
}
```

### 2. Voting and Consensus

```rust
use threshold_api::metrics;
use std::time::Instant;

pub async fn cast_vote(
    vote_request: VoteRequest,
) -> Result<VoteResponse, Error> {
    let node_id = get_node_id();
    let start = Instant::now();

    // Process vote
    let result = match validate_and_cast_vote(vote_request).await {
        Ok(vote) => {
            // Record successful vote
            let result_str = if vote.approved { "approve" } else { "reject" };
            metrics::record_vote(&node_id, result_str);
            Ok(vote)
        }
        Err(e) => {
            tracing::error!("Vote failed: {}", e);
            Err(e)
        }
    };

    // Record processing time
    let duration = start.elapsed().as_secs_f64();
    metrics::record_vote_processing(&node_id, duration);

    result
}

pub async fn run_consensus_round(
    transaction_id: &str,
) -> Result<ConsensusResult, Error> {
    let node_id = get_node_id();
    let start = Instant::now();

    // Run consensus
    let result = execute_consensus(transaction_id).await?;

    // Record round duration
    let duration = start.elapsed().as_secs_f64();
    metrics::record_consensus_round(&node_id, duration);

    Ok(result)
}
```

### 3. Byzantine Fault Detection

```rust
use threshold_api::metrics;

pub async fn detect_byzantine_behavior(
    vote: &Vote,
    previous_votes: &[Vote],
) -> Result<(), ByzantineError> {
    let node_id = get_node_id();

    // Check for double voting
    if let Some(duplicate) = find_duplicate_vote(vote, previous_votes) {
        tracing::error!(
            "Double vote detected: node {} voted twice on transaction {}",
            vote.node_id,
            vote.transaction_id
        );

        metrics::record_byzantine_violation(&vote.node_id.to_string(), "double_vote");

        return Err(ByzantineError::DoubleVote);
    }

    // Check for invalid signature
    if !verify_vote_signature(vote) {
        tracing::error!(
            "Invalid vote signature from node {}",
            vote.node_id
        );

        metrics::record_byzantine_violation(&vote.node_id.to_string(), "invalid_signature");

        return Err(ByzantineError::InvalidSignature);
    }

    // Check for timeout violations
    if is_vote_expired(vote) {
        metrics::record_byzantine_violation(&vote.node_id.to_string(), "timeout");
        return Err(ByzantineError::Timeout);
    }

    Ok(())
}
```

### 4. Signature Generation

```rust
use threshold_api::metrics;
use std::time::Instant;

pub async fn generate_signature_cggmp24(
    message: &[u8],
    key_share: &KeyShare,
) -> Result<Signature, Error> {
    let node_id = get_node_id();
    let start = Instant::now();

    // Generate signature
    let result = match perform_cggmp24_signing(message, key_share).await {
        Ok(signature) => {
            let duration = start.elapsed().as_secs_f64();
            metrics::record_signature(&node_id, "cggmp24", "success", duration);
            Ok(signature)
        }
        Err(e) => {
            let duration = start.elapsed().as_secs_f64();
            metrics::record_signature(&node_id, "cggmp24", "failure", duration);
            Err(e)
        }
    };

    result
}

pub async fn generate_signature_frost(
    message: &[u8],
    key_share: &KeyShare,
) -> Result<Signature, Error> {
    let node_id = get_node_id();
    let start = Instant::now();

    // Generate signature
    let result = match perform_frost_signing(message, key_share).await {
        Ok(signature) => {
            let duration = start.elapsed().as_secs_f64();
            metrics::record_signature(&node_id, "frost", "success", duration);
            Ok(signature)
        }
        Err(e) => {
            let duration = start.elapsed().as_secs_f64();
            metrics::record_signature(&node_id, "frost", "failure", duration);
            Err(e)
        }
    };

    result
}
```

### 5. DKG Ceremony

```rust
use threshold_api::metrics;
use std::time::Instant;

pub async fn run_dkg_ceremony(
    protocol: &str,
    participants: &[NodeId],
) -> Result<KeyShare, Error> {
    let node_id = get_node_id();
    let start = Instant::now();

    tracing::info!("Starting DKG ceremony with {} protocol", protocol);

    // Run DKG
    let key_share = match protocol {
        "cggmp24" => perform_cggmp24_dkg(participants).await?,
        "frost" => perform_frost_dkg(participants).await?,
        _ => return Err(Error::UnsupportedProtocol),
    };

    // Record DKG duration
    let duration = start.elapsed().as_secs_f64();
    metrics::record_dkg(&node_id, protocol, duration);

    tracing::info!("DKG ceremony completed in {:.2}s", duration);

    Ok(key_share)
}
```

### 6. Presignature Pool Management

```rust
use threshold_api::metrics;

pub struct PresignaturePool {
    cggmp24_pool: Vec<Presignature>,
    frost_pool: Vec<Presignature>,
    node_id: String,
}

impl PresignaturePool {
    pub fn new(node_id: String) -> Self {
        Self {
            cggmp24_pool: Vec::new(),
            frost_pool: Vec::new(),
            node_id,
        }
    }

    pub fn add_presignature(&mut self, protocol: &str, presig: Presignature) {
        match protocol {
            "cggmp24" => self.cggmp24_pool.push(presig),
            "frost" => self.frost_pool.push(presig),
            _ => return,
        }

        self.update_metrics();
    }

    pub fn consume_presignature(&mut self, protocol: &str) -> Option<Presignature> {
        let presig = match protocol {
            "cggmp24" => self.cggmp24_pool.pop(),
            "frost" => self.frost_pool.pop(),
            _ => None,
        };

        self.update_metrics();
        presig
    }

    fn update_metrics(&self) {
        metrics::update_presignature_pool_size(
            &self.node_id,
            "cggmp24",
            self.cggmp24_pool.len(),
        );
        metrics::update_presignature_pool_size(
            &self.node_id,
            "frost",
            self.frost_pool.len(),
        );
    }

    // Background task to maintain pool
    pub async fn maintain_pool(&mut self, min_size: usize) {
        while self.cggmp24_pool.len() < min_size {
            match generate_presignature("cggmp24").await {
                Ok(presig) => self.add_presignature("cggmp24", presig),
                Err(e) => {
                    tracing::error!("Failed to generate CGGMP24 presignature: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
        }

        while self.frost_pool.len() < min_size {
            match generate_presignature("frost").await {
                Ok(presig) => self.add_presignature("frost", presig),
                Err(e) => {
                    tracing::error!("Failed to generate FROST presignature: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
        }
    }
}
```

### 7. Network Monitoring

```rust
use threshold_api::metrics;

pub struct PeerManager {
    peers: HashMap<NodeId, PeerConnection>,
    node_id: String,
}

impl PeerManager {
    pub fn new(node_id: String) -> Self {
        Self {
            peers: HashMap::new(),
            node_id,
        }
    }

    pub async fn connect_peer(&mut self, peer_id: NodeId, addr: SocketAddr) -> Result<(), Error> {
        match establish_quic_connection(addr).await {
            Ok(conn) => {
                self.peers.insert(peer_id, conn);
                self.update_peer_metrics();
                Ok(())
            }
            Err(e) => {
                metrics::record_network_error(&self.node_id, "connection");
                Err(e)
            }
        }
    }

    pub fn disconnect_peer(&mut self, peer_id: &NodeId) {
        self.peers.remove(peer_id);
        self.update_peer_metrics();
    }

    pub async fn handle_tls_handshake(&self, stream: TlsStream) -> Result<(), Error> {
        match complete_tls_handshake(stream).await {
            Ok(_) => Ok(()),
            Err(e) => {
                metrics::record_tls_handshake_failure(&self.node_id);
                Err(e)
            }
        }
    }

    fn update_peer_metrics(&self) {
        metrics::update_connected_peers(&self.node_id, self.peers.len());
    }

    pub async fn health_check_loop(&mut self) {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

            // Check all peer connections
            let mut disconnected = Vec::new();
            for (peer_id, conn) in &self.peers {
                if !conn.is_alive() {
                    disconnected.push(*peer_id);
                }
            }

            // Remove dead connections
            for peer_id in disconnected {
                self.disconnect_peer(&peer_id);
                metrics::record_network_error(&self.node_id, "timeout");
            }
        }
    }
}
```

### 8. Background Metrics Update Task

```rust
use threshold_api::metrics;
use std::time::Duration;

pub async fn metrics_update_task(
    node_id: String,
    tx_queue: Arc<Mutex<TransactionQueue>>,
    peer_manager: Arc<Mutex<PeerManager>>,
    presig_pool: Arc<Mutex<PresignaturePool>>,
) {
    loop {
        tokio::time::sleep(Duration::from_secs(10)).await;

        // Update transaction queue size
        if let Ok(queue) = tx_queue.lock() {
            metrics::update_transaction_queue_size(&node_id, queue.len());
        }

        // Update connected peers
        if let Ok(peers) = peer_manager.lock() {
            metrics::update_connected_peers(&node_id, peers.peer_count());
        }

        // Update presignature pools
        if let Ok(pool) = presig_pool.lock() {
            metrics::update_presignature_pool_size(
                &node_id,
                "cggmp24",
                pool.cggmp24_count(),
            );
            metrics::update_presignature_pool_size(
                &node_id,
                "frost",
                pool.frost_count(),
            );
        }
    }
}

// Spawn in your main function:
// tokio::spawn(metrics_update_task(node_id, tx_queue, peer_manager, presig_pool));
```

## Testing Metrics

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use threshold_api::metrics;

    #[test]
    fn test_transaction_metrics() {
        metrics::initialize_metrics("test-node", 3, 5);

        // Record some transactions
        metrics::record_transaction("test-node", "pending");
        metrics::record_transaction("test-node", "approved");

        // Gather metrics
        let metrics_text = metrics::gather_metrics().unwrap();

        // Verify metrics are present
        assert!(metrics_text.contains("mpc_transactions_total"));
        assert!(metrics_text.contains("pending"));
        assert!(metrics_text.contains("approved"));
    }

    #[tokio::test]
    async fn test_vote_processing_metrics() {
        metrics::initialize_metrics("test-node", 3, 5);

        // Simulate vote processing
        let start = std::time::Instant::now();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        let duration = start.elapsed().as_secs_f64();

        metrics::record_vote_processing("test-node", duration);

        let metrics_text = metrics::gather_metrics().unwrap();
        assert!(metrics_text.contains("mpc_vote_processing_duration_seconds"));
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_metrics_endpoint() {
    // Start test server
    let app = create_test_app();
    let server = spawn_test_server(app).await;

    // Make request to metrics endpoint
    let response = reqwest::get(&format!("{}/metrics", server.url()))
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = response.text().await.unwrap();
    assert!(body.contains("# HELP mpc_transactions_total"));
    assert!(body.contains("# TYPE mpc_transactions_total counter"));
}
```

### Manual Testing

```bash
# Test metrics endpoint directly
curl http://localhost:8080/metrics

# Should return Prometheus format:
# # HELP mpc_transactions_total Total number of transactions by state
# # TYPE mpc_transactions_total counter
# mpc_transactions_total{state="pending",node_id="1"} 42
# ...

# Test with Prometheus
curl -G http://localhost:9090/api/v1/query \
  --data-urlencode 'query=mpc_transactions_total'
```

## Best Practices

### 1. Label Cardinality

❌ **Bad**: High cardinality labels
```rust
// Don't use transaction IDs or user IDs as labels
metrics::record_transaction(&transaction_id, "pending"); // WRONG
```

✅ **Good**: Low cardinality labels
```rust
// Use state and node_id which have limited values
metrics::record_transaction(&node_id, "pending"); // CORRECT
```

### 2. Metric Naming

Follow Prometheus naming conventions:
- `mpc_` prefix for all MPC-related metrics
- `_total` suffix for counters
- `_seconds` suffix for time durations
- Use snake_case

### 3. Error Handling

Always handle metric collection errors gracefully:

```rust
pub async fn process_request() -> Result<Response, Error> {
    // Don't let metrics failures crash your application
    if let Err(e) = metrics::record_transaction("node-1", "pending") {
        tracing::warn!("Failed to record metric: {}", e);
    }

    // Continue with business logic
    process_business_logic().await
}
```

### 4. Performance

Metrics should be fast and non-blocking:

```rust
// ✅ Good: Quick metric update
metrics::record_vote(&node_id, "approve");

// ❌ Bad: Expensive operation in metric handler
metrics::record_custom_metric(expensive_calculation());
```

### 5. Documentation

Document your custom metrics:

```rust
lazy_static! {
    /// Number of active WebSocket connections
    /// Labels: node_id
    pub static ref WEBSOCKET_CONNECTIONS: GaugeVec = register_gauge_vec!(
        "mpc_websocket_connections",
        "Number of active WebSocket connections",
        &["node_id"]
    ).unwrap();
}
```

## Common Patterns

### Pattern 1: Timer Helper

```rust
use std::time::Instant;

pub struct MetricTimer {
    start: Instant,
    node_id: String,
    operation: String,
}

impl MetricTimer {
    pub fn new(node_id: String, operation: String) -> Self {
        Self {
            start: Instant::now(),
            node_id,
            operation,
        }
    }
}

impl Drop for MetricTimer {
    fn drop(&mut self) {
        let duration = self.start.elapsed().as_secs_f64();
        match self.operation.as_str() {
            "vote" => metrics::record_vote_processing(&self.node_id, duration),
            "consensus" => metrics::record_consensus_round(&self.node_id, duration),
            _ => {}
        }
    }
}

// Usage:
async fn process_vote(vote: Vote) -> Result<(), Error> {
    let _timer = MetricTimer::new("node-1".to_string(), "vote".to_string());
    // Vote processing happens here
    // Timer automatically records duration when dropped
    Ok(())
}
```

### Pattern 2: Metrics Middleware

```rust
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::http::Request;

pub async fn metrics_middleware<B>(
    request: Request<B>,
    next: Next<B>,
) -> Response {
    let start = Instant::now();
    let method = request.method().clone();
    let uri = request.uri().clone();

    let response = next.run(request).await;

    let duration = start.elapsed().as_secs_f64();
    let status = response.status().as_u16();

    // Record HTTP request metrics
    HTTP_REQUESTS_TOTAL
        .with_label_values(&[
            method.as_str(),
            uri.path(),
            &status.to_string(),
        ])
        .inc();

    HTTP_REQUEST_DURATION_SECONDS
        .with_label_values(&[method.as_str(), uri.path()])
        .observe(duration);

    response
}

// Add to your router:
let app = Router::new()
    .route("/api/v1/transactions", post(create_transaction))
    .layer(middleware::from_fn(metrics_middleware));
```

## Troubleshooting

### Metrics Not Updating

1. Check if metrics are initialized:
   ```rust
   metrics::initialize_metrics("node-1", 4, 5);
   ```

2. Verify metric recording is called:
   ```rust
   tracing::debug!("Recording transaction metric");
   metrics::record_transaction("node-1", "pending");
   ```

3. Test metrics endpoint:
   ```bash
   curl http://localhost:8080/metrics | grep mpc_transactions_total
   ```

### High Memory Usage

If metrics cause high memory usage:

1. Reduce label cardinality
2. Use sampling for high-frequency metrics
3. Adjust Prometheus retention period

### Metrics Out of Sync

Use background update tasks for eventually consistent metrics like queue sizes and pool counts.

## Next Steps

1. Review the main [README.md](README.md) for dashboard and alert documentation
2. Check [prometheus/alerts.yml](prometheus/alerts.yml) for alert configurations
3. Explore Grafana dashboards for visualization examples
4. Add custom metrics for your specific use cases
