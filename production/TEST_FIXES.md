# Test Fixes - Implementation Plan

**Generated:** 2026-01-26
**Test Results Reference:** [TEST_RESULTS.md](TEST_RESULTS.md)
**Total Tests Analyzed:** 86
**Tests Requiring Fixes:** 36 (PARTIAL status)

---

## Executive Summary

This document provides a comprehensive analysis of all failing/partial tests and actionable implementation plans to fix them. Tests fall into 5 major categories:

1. **DKG Implementation** (16 tests) - Core cryptographic protocol not fully implemented
2. **Authentication & Security** (2 tests) - Missing auth layer and rate limiting
3. **Node Health Tracking** (4 tests) - Hardcoded cluster status, no real-time monitoring
4. **Presignature & Signing** (10 tests) - Dependent on DKG completion
5. **Disaster Recovery** (4 tests) - Backup/restore mechanisms not tested

---

## Critical Priority Issues

### ⚠️ PRIORITY 1: DKG Ceremony Implementation

**Status:** Partially implemented, fails at database insertion

**Affected Tests:**
- Test #42: Full DKG Ceremony (CGGMP24)
- Test #43: DKG Round 1 (Commitment)
- Test #44: DKG Round 2 (Share Distribution)
- Test #45: DKG Round 3 (Verification)
- Test #46: DKG Key Share Persistence
- Test #47: DKG Failure - Insufficient Participants
- Test #48: DKG Failure - Node Timeout
- Test #49: Aux Info Ceremony
- Test #50: Aux Info Data Verification

**Root Cause Analysis:**

**File:** `crates/orchestrator/src/dkg_service.rs`
**Lines:** 244-323 (initiate_dkg function)

```rust
// Line 302-308: Database insertion fails
self.postgres
    .create_dkg_ceremony(&ceremony.to_storage())
    .await
    .map_err(|e| {
        OrchestrationError::StorageError(format!("Failed to create ceremony: {}", e))
    })?;
```

**File:** `crates/storage/src/postgres.rs`
**Lines:** 845-873 (create_dkg_ceremony function)

```rust
// Line 852-868: SQL INSERT with incorrect type conversion
client.execute(
    r#"
    INSERT INTO dkg_ceremonies (session_id, protocol, threshold, total_nodes, status, started_at)
    VALUES ($1, $2, $3, $4, $5, $6)
    "#,
    &[
        &ceremony.session_id.to_string(),  // ← Problem: UUID → String conversion
        &ceremony.protocol.to_string(),     // ← Redundant .to_string() (already String)
        &(ceremony.threshold as i32),
        &(ceremony.total_nodes as i32),
        &ceremony.status.to_string(),       // ← Redundant .to_string() (already String)
        &ceremony.started_at,
    ],
)
```

**Problem Details:**

1. **Type Mismatch:** Database schema expects `TEXT` for session_id, but the conversion might be failing
2. **Error Handling:** Generic "db error" doesn't reveal the actual SQL error
3. **Missing Protocol Implementation:** The DKG ceremony creates a record but doesn't actually run the cryptographic protocol

**Fix Implementation Plan:**

#### Step 1: Fix Database Type Handling

**File:** `crates/storage/src/postgres.rs`
**Action:** Modify `create_dkg_ceremony` method

```rust
pub async fn create_dkg_ceremony(&self, ceremony: &crate::DkgCeremony) -> Result<()> {
    let client = self
        .pool
        .get()
        .await
        .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

    // FIX: Proper type handling and better error messages
    let session_id_str = ceremony.session_id.to_string();

    let result = client
        .execute(
            r#"
            INSERT INTO dkg_ceremonies (session_id, protocol, threshold, total_nodes, status, started_at)
            VALUES ($1, $2::text, $3, $4, $5::text, $6)
            "#,
            &[
                &session_id_str,              // Already String
                &ceremony.protocol,           // Remove redundant .to_string()
                &(ceremony.threshold as i32),
                &(ceremony.total_nodes as i32),
                &ceremony.status,             // Remove redundant .to_string()
                &ceremony.started_at,
            ],
        )
        .await;

    // FIX: Better error logging
    match result {
        Ok(_) => {
            info!("Created DKG ceremony: session_id={} protocol={}", session_id_str, ceremony.protocol);
            Ok(())
        }
        Err(e) => {
            error!("Failed to insert DKG ceremony: SQL Error: {:?}", e);
            error!("Ceremony data: session_id={}, protocol={}, threshold={}, total_nodes={}",
                   session_id_str, ceremony.protocol, ceremony.threshold, ceremony.total_nodes);
            Err(Error::StorageError(format!("Failed to create DKG ceremony: {}", e)))
        }
    }
}
```

#### Step 2: Implement Actual DKG Protocol Execution

**File:** `crates/orchestrator/src/dkg_service.rs`
**Lines:** 417-570 (run_cggmp24_dkg and run_frost_dkg)

**Current State:** Functions exist but contain placeholder/incomplete implementations

```rust
async fn run_cggmp24_dkg(
    &self,
    session_id: Uuid,
    participants: Vec<NodeId>,
) -> Result<DkgResult> {
    // TODO: Implement actual CGGMP24 DKG protocol
    // ...
}
```

**Required Implementation:**

1. **Message Broadcasting:** Use `QuicEngine` to send DKG round messages to all participants
2. **Round Coordination:** Implement 3-round DKG protocol:
   - Round 1: Commitment phase
   - Round 2: Share distribution
   - Round 3: Share verification
3. **Key Share Storage:** Save encrypted key shares to PostgreSQL
4. **Public Key Derivation:** Calculate and store the threshold public key

**Detailed Implementation Steps:**

```rust
// File: crates/orchestrator/src/dkg_service.rs
// Replace run_cggmp24_dkg implementation

async fn run_cggmp24_dkg(
    &self,
    session_id: Uuid,
    participants: Vec<NodeId>,
) -> Result<DkgResult> {
    info!("Starting CGGMP24 DKG for session {}", session_id);

    let threshold = participants.len() - 1; // 4-of-5
    let party_index = self.node_id.0 as u16;

    // Step 1: Initialize DKG protocol from protocols crate
    use protocols::cggmp24::keygen;

    let dkg_result = keygen::run_keygen(
        party_index,
        threshold as u16,
        participants.len() as u16,
        session_id.to_string(),
        self.quic.clone(), // For P2P communication
        self.message_router.clone(),
    )
    .await
    .map_err(|e| {
        OrchestrationError::ProtocolError(format!("CGGMP24 keygen failed: {}", e))
    })?;

    // Step 2: Store key share in PostgreSQL (encrypted)
    let key_share_data = bincode::serialize(&dkg_result.key_share)
        .map_err(|e| OrchestrationError::SerializationError(format!("{}", e)))?;

    self.postgres
        .store_key_share(session_id, self.node_id, &key_share_data)
        .await
        .map_err(|e| {
            OrchestrationError::StorageError(format!("Failed to store key share: {}", e))
        })?;

    // Step 3: Update ceremony status to completed
    self.postgres
        .complete_dkg_ceremony(session_id, &dkg_result.public_key)
        .await?;

    // Step 4: Derive Bitcoin address from public key
    let address = threshold_bitcoin::derive_p2wpkh_address(&dkg_result.public_key, bitcoin::Network::Testnet)
        .map_err(|e| OrchestrationError::BitcoinError(format!("Failed to derive address: {}", e)))?;

    info!("CGGMP24 DKG completed: session={} address={}", session_id, address);

    Ok(DkgResult {
        session_id,
        public_key: dkg_result.public_key,
        bitcoin_address: address,
        participants,
    })
}
```

**New Dependencies Required:**

Add to `crates/orchestrator/Cargo.toml`:
```toml
bincode = "1.3"
```

#### Step 3: Implement DKG Round Message Broadcasting

**File:** `crates/orchestrator/src/dkg_service.rs`
**Lines:** 724-780 (broadcast_dkg_message and collect_dkg_round)

**Current Implementation:** Basic structure exists but needs enhancement

```rust
async fn broadcast_dkg_message(
    &self,
    session_id: Uuid,
    round: u32,
    message_payload: Vec<u8>,
    recipients: &[NodeId],
) -> Result<()> {
    info!("Broadcasting DKG message: session={} round={} size={} recipients={}",
          session_id, round, message_payload.len(), recipients.len());

    // Create message
    let network_message = NetworkMessage::DkgMessage {
        session_id,
        round,
        sender: self.node_id,
        payload: message_payload.clone(),
    };

    // Serialize
    let serialized = bincode::serialize(&network_message)
        .map_err(|e| OrchestrationError::SerializationError(format!("{}", e)))?;

    // Send to all recipients via QUIC
    for recipient in recipients {
        if *recipient == self.node_id {
            continue; // Skip self
        }

        match self.quic.send_message(*recipient, &serialized).await {
            Ok(_) => {
                info!("Sent DKG message to node {}", recipient);
            }
            Err(e) => {
                warn!("Failed to send DKG message to node {}: {}", recipient, e);
                // Continue with other nodes (Byzantine tolerance)
            }
        }
    }

    Ok(())
}

async fn collect_dkg_round(
    &self,
    session_id: Uuid,
    round: u32,
    expected_participants: &[NodeId],
    timeout: Duration,
) -> Result<Vec<(NodeId, Vec<u8>)>> {
    info!("Collecting DKG round {} messages from {} participants (timeout: {:?})",
          round, expected_participants.len(), timeout);

    let deadline = tokio::time::Instant::now() + timeout;
    let mut received_messages = Vec::new();

    loop {
        // Check timeout
        if tokio::time::Instant::now() > deadline {
            warn!("DKG round {} collection timed out. Received {}/{} messages",
                  round, received_messages.len(), expected_participants.len());
            break;
        }

        // Try to receive message from buffer
        let mut buffer = self.message_buffer.lock().await;
        if let Some(round_messages) = buffer.get_mut(&session_id) {
            if let Some(node_messages) = round_messages.get_mut(&round) {
                // Collect all available messages
                for (node_id, payload) in node_messages.drain() {
                    received_messages.push((node_id, payload));
                }
            }
        }
        drop(buffer);

        // Check if we have threshold messages
        if received_messages.len() >= expected_participants.len() {
            info!("Collected all {} DKG round {} messages", received_messages.len(), round);
            break;
        }

        // Wait a bit before checking again
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Verify we have threshold (allow Byzantine tolerance)
    let threshold = (expected_participants.len() * 2) / 3; // 2/3 threshold
    if received_messages.len() < threshold {
        return Err(OrchestrationError::InsufficientParticipants(format!(
            "Only received {}/{} DKG round {} messages (threshold: {})",
            received_messages.len(), expected_participants.len(), round, threshold
        )));
    }

    Ok(received_messages)
}
```

#### Step 4: Test DKG Implementation

**New Test File:** `crates/orchestrator/tests/dkg_integration_test.rs`

```rust
#[tokio::test]
async fn test_dkg_ceremony_complete_flow() {
    // Setup 5-node cluster
    let nodes = setup_test_cluster(5).await;

    // Initiate DKG on node 1
    let dkg_result = nodes[0]
        .dkg_service
        .initiate_dkg(ProtocolType::CGGMP24, 4, 5)
        .await
        .expect("DKG should complete successfully");

    // Verify all nodes have key shares
    for node in &nodes {
        let key_share = node
            .postgres
            .get_key_share(dkg_result.session_id, node.node_id)
            .await
            .expect("Key share should exist")
            .expect("Key share should not be None");

        assert!(!key_share.is_empty(), "Key share should not be empty");
    }

    // Verify public key is stored
    let ceremony = nodes[0]
        .postgres
        .get_dkg_ceremony(dkg_result.session_id)
        .await
        .expect("Ceremony should exist");

    assert_eq!(ceremony.status, "completed");
    assert!(ceremony.public_key.is_some());
}
```

**Estimated Implementation Time:** 3-5 days
**Complexity:** High
**Blockers:** Requires full understanding of CGGMP24 protocol

---

### ⚠️ PRIORITY 2: Node Health Tracking

**Status:** Hardcoded values returned

**Affected Tests:**
- Test #79: Single Node Restart
- Test #80: 1 Node Failure (Threshold Safe)
- Test #81: 2 Nodes Failure (Threshold UNSAFE)
- Test #83: etcd Cluster Quorum Loss

**Root Cause Analysis:**

**File:** `crates/api/src/handlers/cluster.rs`
**Lines:** 18-38 (get_cluster_status function)

```rust
pub async fn get_cluster_status(
    _postgres: &PostgresStorage,  // ← Parameters not used!
    _etcd: &Mutex<EtcdStorage>,   // ← Parameters not used!
) -> Result<ClusterStatus, ApiError> {
    info!("Fetching cluster status");

    // For now, return placeholder values
    // You would implement: etcd.get_cluster_config() and check node health

    Ok(ClusterStatus {
        total_nodes: 5,         // ← Hardcoded!
        healthy_nodes: 5,       // ← Always returns 5, never changes!
        threshold: 3,           // ← Hardcoded!
        status: "healthy".to_string(),
    })
}
```

**Fix Implementation Plan:**

#### Step 1: Implement Real Node Health Tracking

**File:** `crates/api/src/handlers/cluster.rs`
**Replace:** `get_cluster_status` function

```rust
pub async fn get_cluster_status(
    postgres: &PostgresStorage,
    etcd: &Mutex<EtcdStorage>,
) -> Result<ClusterStatus, ApiError> {
    info!("Fetching cluster status");

    // Step 1: Get cluster configuration from etcd
    let mut etcd_client = etcd.lock().await;
    let cluster_config = etcd_client
        .get("/cluster/config")
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to get cluster config: {}", e)))?;

    // Parse cluster config (JSON format expected)
    let config: serde_json::Value = serde_json::from_str(&cluster_config)
        .unwrap_or_else(|_| {
            // Default configuration if not set in etcd
            serde_json::json!({
                "total_nodes": 5,
                "threshold": 4
            })
        });

    let total_nodes = config["total_nodes"].as_u64().unwrap_or(5) as u32;
    let threshold = config["threshold"].as_u64().unwrap_or(3) as u32;

    drop(etcd_client); // Release lock early

    // Step 2: Query actual node health from PostgreSQL
    let mut healthy_count = 0;
    let health_threshold_seconds = 60; // Consider node healthy if seen in last 60 seconds

    for node_id in 1..=total_nodes {
        match postgres.get_node_health(NodeId(node_id as u64)).await {
            Ok(Some(health_data)) => {
                let seconds_since_heartbeat = health_data["seconds_since_heartbeat"]
                    .as_f64()
                    .unwrap_or(9999.0);

                if seconds_since_heartbeat < health_threshold_seconds as f64 {
                    healthy_count += 1;
                    info!("Node {} is healthy (last seen {} seconds ago)",
                          node_id, seconds_since_heartbeat);
                } else {
                    warn!("Node {} is unhealthy (last seen {} seconds ago)",
                          node_id, seconds_since_heartbeat);
                }
            }
            Ok(None) => {
                warn!("Node {} has no health data", node_id);
            }
            Err(e) => {
                error!("Error checking health for node {}: {}", node_id, e);
            }
        }
    }

    // Step 3: Determine overall cluster health
    let status = if healthy_count >= threshold {
        "healthy"
    } else if healthy_count > 0 {
        "degraded"
    } else {
        "critical"
    };

    info!("Cluster status: {}/{} healthy nodes (threshold: {})",
          healthy_count, total_nodes, threshold);

    Ok(ClusterStatus {
        total_nodes,
        healthy_nodes: healthy_count,
        threshold,
        status: status.to_string(),
    })
}
```

#### Step 2: Implement Node Heartbeat Mechanism

**New File:** `crates/orchestrator/src/heartbeat_service.rs`

```rust
//! Node Heartbeat Service
//!
//! Periodically sends heartbeat signals to update node health status

use std::sync::Arc;
use std::time::Duration;
use threshold_storage::PostgresStorage;
use threshold_types::NodeId;
use tokio::time;
use tracing::{error, info};

pub struct HeartbeatService {
    postgres: Arc<PostgresStorage>,
    node_id: NodeId,
    interval: Duration,
}

impl HeartbeatService {
    pub fn new(postgres: Arc<PostgresStorage>, node_id: NodeId) -> Self {
        Self {
            postgres,
            node_id,
            interval: Duration::from_secs(10), // Send heartbeat every 10 seconds
        }
    }

    /// Start the heartbeat loop (runs in background)
    pub async fn start(&self) {
        let mut interval = time::interval(self.interval);

        info!("Starting heartbeat service for node {}", self.node_id);

        loop {
            interval.tick().await;

            // Update node heartbeat
            if let Err(e) = self.postgres.update_node_last_seen(&format!("node-{}", self.node_id.0)).await {
                error!("Failed to send heartbeat for node {}: {}", self.node_id, e);
            } else {
                info!("Heartbeat sent for node {}", self.node_id);
            }
        }
    }

    /// Spawn heartbeat service in background task
    pub fn spawn(self: Arc<Self>) {
        tokio::spawn(async move {
            self.start().await;
        });
    }
}
```

#### Step 3: Initialize Heartbeat Service in Main

**File:** `crates/server/src/main.rs`
**Add after orchestrator initialization:**

```rust
// Initialize heartbeat service
let heartbeat_service = Arc::new(HeartbeatService::new(
    postgres.clone(),
    node_id,
));
heartbeat_service.clone().spawn();

info!("Heartbeat service started for node {}", node_id);
```

**Estimated Implementation Time:** 1 day
**Complexity:** Medium
**Blockers:** None

---

### ⚠️ PRIORITY 3: Authentication Layer

**Status:** Not implemented

**Affected Tests:**
- Test #58: Unauthenticated Access
- Test #65: API Rate Limiting

**Root Cause Analysis:**

**File:** `crates/api/src/lib.rs`
**Lines:** 59-68 (Router middleware configuration)

```rust
// Build the complete router with middleware
Router::new()
    .route("/health", get(routes::health::health_check))
    .nest("/api/v1", api_v1)
    .layer(
        ServiceBuilder::new()
            .layer(TraceLayer::new_for_http())  // Only logging
            .layer(cors),                        // Only CORS
    )
    .with_state(state)
```

**Missing:** Authentication middleware and rate limiting middleware

**Fix Implementation Plan:**

#### Step 1: Implement JWT Authentication Middleware

**New File:** `crates/api/src/middleware/auth.rs`

```rust
//! JWT Authentication Middleware

use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use tracing::warn;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,     // Subject (user ID)
    pub role: String,    // Role (admin, user)
    pub exp: usize,      // Expiration time
}

/// JWT authentication middleware
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
    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "dev-secret-key".to_string());
    let decoding_key = DecodingKey::from_secret(secret.as_bytes());

    match decode::<Claims>(token, &decoding_key, &Validation::new(Algorithm::HS256)) {
        Ok(_claims) => {
            // Authentication successful
            Ok(next.run(req).await)
        }
        Err(e) => {
            warn!("JWT validation failed: {}", e);
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

/// Public endpoints that don't require authentication
pub fn is_public_endpoint(path: &str) -> bool {
    matches!(
        path,
        "/health" | "/api/v1/cluster/status"
    )
}
```

#### Step 2: Implement Rate Limiting Middleware

**New File:** `crates/api/src/middleware/rate_limit.rs`

```rust
//! Rate Limiting Middleware using Token Bucket algorithm

use axum::{
    body::Body,
    extract::{Request, ConnectInfo},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::warn;

/// Rate limiter configuration
#[derive(Clone)]
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
struct TokenBucket {
    tokens: usize,
    last_refill: Instant,
}

/// Rate limiter state
pub struct RateLimiter {
    buckets: Arc<Mutex<HashMap<String, TokenBucket>>>,
    config: RateLimitConfig,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            buckets: Arc::new(Mutex::new(HashMap::new())),
            config,
        }
    }

    /// Check if request is allowed
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
}

/// Rate limiting middleware
pub async fn rate_limit(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(limiter): axum::extract::State<Arc<RateLimiter>>,
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
```

#### Step 3: Update API Router with Middleware

**File:** `crates/api/src/lib.rs`
**Update `create_router` function:**

```rust
use crate::middleware::{auth, rate_limit};

pub fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Initialize rate limiter
    let rate_limiter = Arc::new(RateLimiter::new(RateLimitConfig::default()));

    // Public endpoints (no auth required)
    let public_routes = Router::new()
        .route("/health", get(routes::health::health_check))
        .route("/api/v1/cluster/status", get(routes::cluster::get_cluster_status));

    // Protected endpoints (auth required)
    let protected_routes = Router::new()
        .route("/api/v1/transactions", post(routes::transactions::create_transaction))
        .route("/api/v1/transactions", get(routes::transactions::list_transactions))
        .route("/api/v1/transactions/:txid", get(routes::transactions::get_transaction))
        .route("/api/v1/wallet/balance", get(routes::wallet::get_balance))
        .route("/api/v1/wallet/address", get(routes::wallet::get_address))
        .route("/api/v1/cluster/nodes", get(routes::cluster::list_nodes))
        .nest("/api/v1/dkg", routes::dkg::routes())
        .nest("/api/v1/aux-info", routes::aux_info::routes())
        .nest("/api/v1/presignatures", routes::presig::routes())
        .layer(axum::middleware::from_fn(auth::jwt_auth));  // Add auth middleware

    // Combine routes
    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(cors)
        )
        .layer(axum::middleware::from_fn_with_state(
            rate_limiter,
            rate_limit::rate_limit,
        ))
        .with_state(state)
}
```

**New Dependencies:**

Add to `crates/api/Cargo.toml`:
```toml
jsonwebtoken = "9.2"
```

**Estimated Implementation Time:** 2 days
**Complexity:** Medium
**Blockers:** None

---

### ⚠️ PRIORITY 4: Presignature & Signing Tests

**Status:** Dependent on DKG completion

**Affected Tests:**
- Test #51: Presignature Pool Management
- Test #52: Presignature Pool Status
- Test #53: CGGMP24 Signing (P2WPKH)
- Test #54: FROST Signing (P2TR)
- Test #55: Signature Verification
- Test #56: Signing Timeout
- Test #57: Invalid Share Detection
- Test #72: Concurrent Signing Load
- Test #73: DKG Latency
- Test #74: Signing Latency

**Root Cause:**

These tests depend on a completed DKG ceremony. The presignature service code exists and is well-implemented, but requires:

1. A valid DKG ceremony in the database
2. Key shares stored for the local node
3. Aux info generated and stored

**File:** `crates/orchestrator/src/presig_service.rs`
**Lines:** 261-269

```rust
let (aux_info_session_id, aux_info_data) = self
    .aux_info_service
    .get_latest_aux_info()
    .await
    .ok_or_else(|| {
        OrchestrationError::Internal(
            "No aux_info available. Run aux_info generation first.".to_string(),
        )
    })?;
```

**Fix Implementation Plan:**

These will be automatically fixed once **PRIORITY 1 (DKG Implementation)** is completed.

**Additional Steps After DKG Fix:**

1. Add integration test that runs full flow:
   - DKG ceremony
   - Aux info generation
   - Presignature generation
   - Transaction signing

**Test File:** `crates/orchestrator/tests/full_signing_flow_test.rs`

```rust
#[tokio::test]
async fn test_complete_signing_flow() {
    // Step 1: Setup cluster
    let nodes = setup_test_cluster(5).await;

    // Step 2: Run DKG
    let dkg_result = nodes[0].dkg_service.initiate_dkg(...).await.unwrap();

    // Step 3: Generate aux info
    let aux_result = nodes[0].aux_info_service.generate(...).await.unwrap();

    // Step 4: Generate presignatures
    let presig_count = nodes[0].presig_service.generate_batch(10).await.unwrap();
    assert_eq!(presig_count, 10);

    // Step 5: Sign transaction
    let tx = create_test_transaction();
    let signature = nodes[0].signing_coordinator.sign(tx).await.unwrap();

    // Step 6: Verify signature
    assert!(verify_signature(&signature, &dkg_result.public_key));
}
```

**Estimated Implementation Time:** Automatically resolved by DKG fix + 1 day for tests
**Complexity:** Low (dependent on DKG)
**Blockers:** DKG implementation must be completed first

---

## Medium Priority Issues

### 🔶 Byzantine Fault Tolerance Tests

**Affected Tests:**
- Test #66: Malicious Node - Invalid Signature
- Test #68: Byzantine Node - Conflicting Messages
- Test #69: Signature Below Threshold
- Test #70: t-out-of-n Security

**Status:** Detection mechanisms exist (byzantine_violations table) but not actively tested

**Fix:** These require simulating Byzantine behavior, which is complex. Recommend implementing after core functionality (DKG) is working.

---

### 🔶 Disaster Recovery Tests

**Affected Tests:**
- Test #78: Network Throughput (QUIC)
- Test #84: PostgreSQL Backup
- Test #85: PostgreSQL Restore
- Test #86: etcd Snapshot & Restore

**Status:** Backup/restore mechanisms available but not tested in integration tests

**Fix:** Add integration tests that:
1. Create database snapshot
2. Simulate failure
3. Restore from snapshot
4. Verify data integrity

**Estimated Implementation Time:** 2-3 days
**Complexity:** Medium

---

## Implementation Priority Roadmap

### Phase 1: Critical Fixes (1-2 weeks)
1. ✅ Fix DKG database insertion (2 days)
2. ✅ Implement actual CGGMP24 DKG protocol (3-5 days)
3. ✅ Implement node health tracking (1 day)
4. ✅ Add authentication & rate limiting (2 days)

### Phase 2: Testing & Validation (1 week)
5. ✅ Test DKG end-to-end (2 days)
6. ✅ Test presignature generation (1 day)
7. ✅ Test signing flow (2 days)
8. ✅ Integration tests for all flows (2 days)

### Phase 3: Advanced Features (1-2 weeks)
9. ⏳ Byzantine behavior simulation (3 days)
10. ⏳ Disaster recovery tests (3 days)
11. ⏳ Performance optimization (3-5 days)

---

## Testing Strategy

After implementing each fix:

1. **Unit Tests:** Test individual functions
2. **Integration Tests:** Test complete flows
3. **Manual Testing:** Run actual test commands from COMPREHENSIVE_TESTING_GUIDE.md
4. **Update TEST_RESULTS.md:** Change PARTIAL → PASSED

---

## Success Criteria

**Before declaring system production-ready:**

- [ ] All 46 currently PASSED tests remain PASSED
- [ ] At least 30/36 PARTIAL tests converted to PASSED (83%)
- [ ] Zero FAILED tests
- [ ] DKG ceremony completes successfully in <2 minutes
- [ ] Transaction signing works with 4-of-5 threshold
- [ ] API authentication protects sensitive endpoints
- [ ] Rate limiting prevents DoS attacks
- [ ] Node failures properly detected within 60 seconds

---

## Maintenance Notes

**After Fixes Are Implemented:**

1. Update FUTURE_IMPROVEMENTS.md to remove completed items
2. Add new integration tests to CI/CD pipeline
3. Document all API authentication requirements
4. Create deployment guide with security checklist

---

**Document Status:** Ready for Implementation
**Next Step:** Begin Phase 1 - Critical Fixes
**Estimated Total Implementation Time:** 3-4 weeks for full completion
