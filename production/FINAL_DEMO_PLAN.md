# 🎯 FINAL DEMO PLAN - Transaction Signing Flow

**Date:** 2026-01-27
**Goal:** Complete end-to-end transaction flow - from creation to Bitcoin network broadcast
**Status:** 🟡 IN PROGRESS - Voting mechanism missing

---

## 📊 CURRENT STATE ANALYSIS

### ✅ WHAT'S WORKING

1. **DKG (Distributed Key Generation)** - FULLY OPERATIONAL ✓
   - 3 completed ceremonies (CGGMP24 + FROST)
   - 15 key shares stored in database (5 nodes × 3 ceremonies)
   - All nodes have matching public keys
   - Key derivation working

2. **Transaction Creation** - FULLY OPERATIONAL ✓
   - API endpoint: `POST /api/v1/transactions`
   - Creates valid unsigned Bitcoin transactions
   - Stores in PostgreSQL with state "pending"
   - Transaction builder creates proper P2WPKH/SegWit transactions

3. **OrchestrationService** - PARTIALLY WORKING ⚠️
   - Background service polling for pending transactions
   - Creates voting rounds in database
   - Updates transaction states
   - **BUT:** Doesn't actually broadcast vote requests to nodes!

4. **Vote Processor & FSM** - CODE EXISTS ✓
   - `VoteProcessor` can accept and count votes
   - `VoteFSM` manages transaction state transitions
   - Byzantine detection implemented
   - **BUT:** Never receives votes because nodes don't cast them!

5. **Infrastructure**
   - PostgreSQL: Shared database for all nodes
   - etcd: Distributed coordination
   - QUIC/mTLS: P2P communication layer
   - All 5 nodes healthy and running

### ❌ WHAT'S MISSING - THE 3 CRITICAL GAPS

---

## 🔴 GAP #1: Vote Request Broadcasting

### Problem
**File:** `crates/orchestrator/src/service.rs:234`

```rust
// 4. Broadcast vote request to all nodes
// NOTE: In production, this would send P2P messages to all nodes
// For now, nodes will poll their local databases or receive via P2P receiver
```

**Current behavior:**
- OrchestrationService creates a voting round in PostgreSQL
- Updates transaction state to "voting"
- **BUT STOPS THERE** - doesn't tell any node to vote!

**Why it fails:**
- No P2P message sent
- No HTTP request to node APIs
- Nodes have no idea a transaction needs voting

### Solution Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  OrchestrationService (runs on Node-1)                      │
│  ┌──────────────────────────────────────────────────────┐   │
│  │ initiate_voting(&tx)                                  │   │
│  │  1. Create voting_round in DB                         │   │
│  │  2. Update tx state to "voting"                       │   │
│  │  3. [NEW] Broadcast VoteRequest to all nodes         │   │
│  └──────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
                         │
                         ├─── HTTP POST /internal/vote-request
                         │
        ┌────────────────┼────────────────┬──────────────────┐
        │                │                │                  │
        ▼                ▼                ▼                  ▼
   [Node-1]         [Node-2]         [Node-3]         [Node-4]  [Node-5]
   localhost:8081   localhost:8082   localhost:8083   localhost:8084  localhost:8085
```

### Implementation Steps

#### Step 1: Define VoteRequest message type

**File:** `crates/types/src/lib.rs`

```rust
/// Request for nodes to vote on a transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteRequest {
    pub tx_id: TxId,
    pub round_id: i64,
    pub round_number: u32,
    pub threshold: u32,
    pub timeout_at: chrono::DateTime<chrono::Utc>,
}
```

#### Step 2: Add internal API endpoint

**File:** `crates/api/src/routes/internal.rs` (NEW FILE)

```rust
//! Internal API routes for node-to-node communication

use axum::{extract::State, Json, routing::post, Router};
use crate::state::AppState;
use threshold_types::VoteRequest;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/vote-request", post(receive_vote_request))
}

/// Receive a vote request from orchestrator
async fn receive_vote_request(
    State(state): State<AppState>,
    Json(req): Json<VoteRequest>,
) -> Result<Json<&'static str>, ApiError> {
    info!("Received vote request for tx_id={}", req.tx_id);

    // Trigger automatic voting mechanism
    state.vote_trigger.send(req).await
        .map_err(|_| ApiError::InternalError("Vote trigger channel closed".into()))?;

    Ok(Json("Vote request received"))
}
```

#### Step 3: Implement HTTP broadcasting

**File:** `crates/orchestrator/src/service.rs`

Add field to `OrchestrationService`:
```rust
pub struct OrchestrationService {
    // ... existing fields ...

    /// HTTP client for broadcasting to nodes
    http_client: reqwest::Client,

    /// Node endpoints (from NODE_ENDPOINTS env var)
    node_endpoints: HashMap<u64, String>,
}
```

Replace the comment at line 234 with actual implementation:
```rust
// 4. Broadcast vote request to all nodes
let vote_request = VoteRequest {
    tx_id: tx.txid.clone(),
    round_id,
    round_number: 1,
    threshold: 4,
    timeout_at: now + chrono::Duration::seconds(self.config.voting_timeout.as_secs() as i64),
};

// Broadcast to all nodes in parallel
let broadcast_futures: Vec<_> = self.node_endpoints
    .iter()
    .map(|(node_id, endpoint)| {
        let client = self.http_client.clone();
        let url = format!("{}/internal/vote-request", endpoint);
        let req = vote_request.clone();

        async move {
            match client.post(&url)
                .json(&req)
                .timeout(Duration::from_secs(5))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    info!("Vote request sent to node {}", node_id);
                    Ok(())
                }
                Ok(resp) => {
                    warn!("Vote request failed for node {}: status={}", node_id, resp.status());
                    Err(())
                }
                Err(e) => {
                    error!("Failed to send vote request to node {}: {}", node_id, e);
                    Err(())
                }
            }
        }
    })
    .collect();

// Wait for all broadcasts (don't fail if some nodes are unreachable)
let results = futures::future::join_all(broadcast_futures).await;
let success_count = results.iter().filter(|r| r.is_ok()).count();

info!("Vote request broadcast: {}/{} nodes reached", success_count, self.node_endpoints.len());
```

---

## 🔴 GAP #2: Automatic Voting by Nodes

### Problem

**Current behavior:**
- Nodes receive VoteRequest (after Gap #1 is fixed)
- **DO NOTHING** - no automatic voting logic exists
- Votes never cast → `votes_received=0` → timeout

**Why it fails:**
- No background task listening for vote requests
- No logic to automatically approve/reject transactions
- No code to sign and submit votes

### Solution Architecture

```
┌────────────────────────────────────────────────────────────────┐
│  Node-2 (Example - all nodes have same logic)                  │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ VoteRequest received via /internal/vote-request         │   │
│  └─────────────────────┬───────────────────────────────────┘   │
│                        │                                        │
│                        ▼                                        │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ Automatic Voting Logic                                   │   │
│  │  1. Load transaction from PostgreSQL                     │   │
│  │  2. Validate transaction (amount, recipient, etc.)       │   │
│  │  3. Make voting decision (approve/reject)                │   │
│  │  4. Create Vote with signature                           │   │
│  │  5. Submit vote to VoteProcessor                         │   │
│  └─────────────────────────────────────────────────────────┘   │
│                        │                                        │
│                        ▼                                        │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ VoteProcessor::process_vote()                            │   │
│  │  - Validate signature                                    │   │
│  │  - Record in PostgreSQL                                  │   │
│  │  - Increment vote count in etcd                          │   │
│  │  - Check if threshold reached (4/5)                      │   │
│  └─────────────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────────┘
```

### Implementation Steps

#### Step 1: Create AutoVoter component

**File:** `crates/orchestrator/src/auto_voter.rs` (NEW FILE)

```rust
//! Automatic voting logic for transaction approval

use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn, error};
use threshold_types::{VoteRequest, Vote, NodeId, TransactionId};
use threshold_storage::PostgresStorage;
use threshold_consensus::VoteProcessor;
use threshold_crypto::KeyPair;

/// Automatic voter that processes vote requests
pub struct AutoVoter {
    node_id: NodeId,
    postgres: Arc<PostgresStorage>,
    vote_processor: Arc<VoteProcessor>,
    keypair: Arc<KeyPair>,
    receiver: mpsc::Receiver<VoteRequest>,
}

impl AutoVoter {
    pub fn new(
        node_id: NodeId,
        postgres: Arc<PostgresStorage>,
        vote_processor: Arc<VoteProcessor>,
        keypair: Arc<KeyPair>,
        receiver: mpsc::Receiver<VoteRequest>,
    ) -> Self {
        Self {
            node_id,
            postgres,
            vote_processor,
            keypair,
            receiver,
        }
    }

    /// Start the auto voter background task
    pub fn start(mut self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            info!("AutoVoter started for node {}", self.node_id);

            while let Some(vote_request) = self.receiver.recv().await {
                if let Err(e) = self.process_vote_request(vote_request).await {
                    error!("Failed to process vote request: {}", e);
                }
            }

            warn!("AutoVoter stopped for node {}", self.node_id);
        })
    }

    async fn process_vote_request(&self, req: VoteRequest) -> Result<(), Box<dyn std::error::Error>> {
        info!("Processing vote request: tx_id={} round={}", req.tx_id, req.round_number);

        // 1. Load transaction from database
        let tx = self.postgres
            .get_transaction(&req.tx_id)
            .await?
            .ok_or_else(|| format!("Transaction not found: {}", req.tx_id))?;

        info!("Loaded transaction: recipient={} amount={} sats", tx.recipient, tx.amount_sats);

        // 2. Make voting decision (for demo: auto-approve all valid transactions)
        let should_approve = self.evaluate_transaction(&tx).await;

        if !should_approve {
            warn!("Transaction rejected by policy: {}", req.tx_id);
            // Could send reject vote here
            return Ok(());
        }

        // 3. Create and sign vote
        let vote_value = tx.amount_sats; // Include amount in vote
        let message = format!("{}||{}", req.tx_id, vote_value);
        let signature = self.keypair.sign(message.as_bytes());

        let vote = Vote {
            node_id: self.node_id,
            tx_id: req.tx_id.clone(),
            round_number: req.round_number,
            approve: true,
            value: Some(vote_value),
            signature: signature.to_bytes().to_vec(),
            public_key: Some(self.keypair.public_key().to_bytes().to_vec()),
            peer_id: threshold_types::PeerId(format!("node-{}", self.node_id.0)),
            timestamp: chrono::Utc::now(),
        };

        info!("Casting vote: node_id={} tx_id={} approve=true", self.node_id, req.tx_id);

        // 4. Submit vote to vote processor
        match self.vote_processor.process_vote(vote).await {
            Ok(result) => {
                info!("Vote processed: {:?}", result);
            }
            Err(e) => {
                error!("Failed to process vote: {}", e);
            }
        }

        Ok(())
    }

    /// Evaluate whether to approve a transaction
    async fn evaluate_transaction(&self, tx: &Transaction) -> bool {
        // FOR DEMO: Simple validation rules
        // In production: check whitelist, limits, compliance, etc.

        // Rule 1: Amount must be reasonable (< 1 BTC for testnet)
        if tx.amount_sats > 100_000_000 {
            warn!("Transaction amount too high: {} sats", tx.amount_sats);
            return false;
        }

        // Rule 2: Recipient must not be empty
        if tx.recipient.is_empty() {
            warn!("Empty recipient address");
            return false;
        }

        // Rule 3: Transaction must be in valid state
        if tx.state != TransactionState::Voting {
            warn!("Transaction not in voting state: {:?}", tx.state);
            return false;
        }

        info!("Transaction approved by policy");
        true
    }
}
```

#### Step 2: Integrate AutoVoter into node startup

**File:** `crates/api/src/bin/server.rs`

In the main startup logic, add:

```rust
// Create vote trigger channel
let (vote_tx, vote_rx) = tokio::sync::mpsc::channel(100);

// Create AutoVoter
let auto_voter = AutoVoter::new(
    node_id,
    Arc::clone(&postgres),
    Arc::clone(&vote_processor),
    Arc::clone(&keypair), // Node's signing keypair
    vote_rx,
);

// Start AutoVoter background task
let auto_voter_handle = auto_voter.start();

// Store vote_tx in AppState so API can trigger votes
let app_state = AppState {
    // ... existing fields ...
    vote_trigger: vote_tx,
};
```

#### Step 3: Add vote_trigger to AppState

**File:** `crates/api/src/state.rs`

```rust
pub struct AppState {
    // ... existing fields ...

    /// Channel to trigger automatic voting
    pub vote_trigger: tokio::sync::mpsc::Sender<VoteRequest>,
}
```

---

## 🔴 GAP #3: Signed Transaction Broadcasting

### Problem

After signing completes:
- Signed transaction exists in database
- Transaction state is "signed"
- **BUT:** Nobody broadcasts it to Bitcoin network!

### Solution Architecture

```
┌────────────────────────────────────────────────────────────┐
│  OrchestrationService                                       │
│                                                             │
│  ┌───────────────────────────────────────────────────────┐ │
│  │ process_signed_transactions()                          │ │
│  │  - Query transactions in "signed" state                │ │
│  │  - For each signed tx:                                 │ │
│  │    1. Extract signed_tx bytes                          │ │
│  │    2. Convert to hex                                   │ │
│  │    3. Broadcast via BitcoinClient                      │ │
│  │    4. Update state to "broadcasting"/"confirmed"       │ │
│  └───────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────┘
                         │
                         ▼
              ┌──────────────────────┐
              │  BitcoinClient       │
              │  broadcast_tx()      │
              └──────────────────────┘
                         │
                         ▼
              ┌──────────────────────┐
              │  Bitcoin Network     │
              │  (Testnet/Mempool)   │
              └──────────────────────┘
```

### Implementation Steps

#### Step 1: Add broadcast method to BitcoinClient

**File:** `crates/bitcoin/src/client.rs`

```rust
/// Broadcast a signed transaction to the Bitcoin network
pub async fn broadcast_transaction(&self, signed_tx_hex: &str) -> Result<String, BitcoinError> {
    let url = format!("{}/tx", self.esplora_url);

    info!("Broadcasting transaction to Bitcoin network: {}", &signed_tx_hex[..20]);

    let response = self.client
        .post(&url)
        .body(signed_tx_hex.to_string())
        .header("Content-Type", "text/plain")
        .send()
        .await
        .map_err(|e| BitcoinError::NetworkError(e.to_string()))?;

    if response.status().is_success() {
        let txid = response.text().await
            .map_err(|e| BitcoinError::ParseError(e.to_string()))?;

        info!("Transaction broadcast successful: txid={}", txid);
        Ok(txid)
    } else {
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        error!("Transaction broadcast failed: {}", error_text);
        Err(BitcoinError::BroadcastFailed(error_text))
    }
}
```

#### Step 2: Add broadcasting loop to OrchestrationService

**File:** `crates/orchestrator/src/service.rs`

In the `run()` method, add:

```rust
// Process signed transactions (NEW: broadcast to Bitcoin network)
if let Err(e) = self.process_signed_transactions().await {
    error!("Error processing signed transactions: {}", e);
}
```

Implement the method:

```rust
/// Process signed transactions and broadcast to Bitcoin network
async fn process_signed_transactions(&self) -> Result<()> {
    let signed_txs = self.postgres
        .get_transactions_by_state("signed")
        .await
        .map_err(|e| OrchestrationError::Storage(e.into()))?;

    if signed_txs.is_empty() {
        return Ok(());
    }

    debug!("Processing {} signed transactions for broadcast", signed_txs.len());

    for tx in signed_txs {
        // Check if already broadcasting
        if tx.bitcoin_txid.is_some() {
            debug!("Transaction {} already broadcast", tx.txid);
            continue;
        }

        match self.broadcast_transaction(&tx).await {
            Ok(bitcoin_txid) => {
                info!("Transaction broadcast successful: {} → {}", tx.txid, bitcoin_txid);

                // Update state and record Bitcoin txid
                if let Err(e) = self.postgres
                    .update_transaction_bitcoin_txid(&tx.txid, &bitcoin_txid)
                    .await
                {
                    error!("Failed to record Bitcoin txid: {}", e);
                }

                if let Err(e) = self.postgres
                    .update_transaction_state(&tx.txid, TransactionState::Broadcasting)
                    .await
                {
                    error!("Failed to update state to broadcasting: {}", e);
                }
            }
            Err(e) => {
                error!("Failed to broadcast transaction {}: {}", tx.txid, e);
                // Could retry or mark as failed
            }
        }
    }

    Ok(())
}

async fn broadcast_transaction(&self, tx: &Transaction) -> Result<String> {
    let signed_tx = tx.signed_tx
        .as_ref()
        .ok_or_else(|| OrchestrationError::InvalidState("No signed transaction".into()))?;

    let signed_tx_hex = hex::encode(signed_tx);

    self.bitcoin
        .broadcast_transaction(&signed_tx_hex)
        .await
        .map_err(|e| OrchestrationError::BitcoinError(e.to_string()))
}
```

---

## 📋 COMPLETE IMPLEMENTATION CHECKLIST

### Phase 1: Vote Request Broadcasting
- [ ] Add `VoteRequest` type to `types/src/lib.rs`
- [ ] Create `api/src/routes/internal.rs` with `/internal/vote-request` endpoint
- [ ] Add `vote_trigger` channel to `AppState`
- [ ] Add `http_client` and `node_endpoints` to `OrchestrationService`
- [ ] Implement HTTP broadcasting in `initiate_voting()`
- [ ] Parse `NODE_ENDPOINTS` env var on startup
- [ ] Test: Verify vote requests reach all nodes

### Phase 2: Automatic Voting
- [ ] Create `orchestrator/src/auto_voter.rs` module
- [ ] Implement `AutoVoter` struct with vote processing logic
- [ ] Add `evaluate_transaction()` policy rules
- [ ] Integrate AutoVoter into node startup in `bin/server.rs`
- [ ] Create node signing keypair for vote signatures
- [ ] Test: Verify votes are cast automatically
- [ ] Test: Verify votes reach VoteProcessor
- [ ] Test: Verify vote threshold detection (4/5)

### Phase 3: Transaction Broadcasting
- [ ] Add `broadcast_transaction()` to `BitcoinClient`
- [ ] Add `process_signed_transactions()` to `OrchestrationService`
- [ ] Add `update_transaction_bitcoin_txid()` to PostgreSQL storage
- [ ] Add broadcasting to main orchestration loop
- [ ] Test: Verify transactions broadcast to Bitcoin testnet
- [ ] Test: Verify Bitcoin txid recorded in database

### Phase 4: End-to-End Integration
- [ ] Test complete flow: create → vote → sign → broadcast
- [ ] Verify all state transitions
- [ ] Test with multiple concurrent transactions
- [ ] Test failure scenarios (node offline, timeout, etc.)

---

## 🧪 DETAILED TEST PLAN

### Test 1: Vote Request Broadcasting

**Goal:** Verify vote requests reach all 5 nodes

**Steps:**
```bash
# 1. Check all nodes healthy
curl http://localhost:8081/api/v1/cluster/status
# Expect: 5/5 nodes online

# 2. Create a transaction
curl -X POST http://localhost:8081/api/v1/transactions \
  -H "Content-Type: application/json" \
  -d '{
    "recipient": "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx",
    "amount_sats": 50000
  }'

# Expected response:
{
  "txid": "abc123...",
  "state": "pending",
  "amount_sats": 50000
}

# 3. Wait 5 seconds for OrchestrationService to pick it up

# 4. Check transaction state changed to "voting"
curl http://localhost:8081/api/v1/transactions/abc123...

# Expected response:
{
  "txid": "abc123...",
  "state": "voting",  # ← Changed!
  ...
}

# 5. Check voting round created
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c \
  "SELECT * FROM voting_rounds WHERE tx_id='abc123...';"

# Expected: 1 row with votes_received=0, threshold=4

# 6. Check logs on each node for "Received vote request"
docker logs mpc-node-1 2>&1 | grep "Received vote request"
docker logs mpc-node-2 2>&1 | grep "Received vote request"
docker logs mpc-node-3 2>&1 | grep "Received vote request"
docker logs mpc-node-4 2>&1 | grep "Received vote request"
docker logs mpc-node-5 2>&1 | grep "Received vote request"

# Expected: Each log shows "Received vote request for tx_id=abc123..."
```

**Success Criteria:**
- ✅ All 5 nodes log "Received vote request"
- ✅ Transaction state is "voting"
- ✅ Voting round exists in database

---

### Test 2: Automatic Voting

**Goal:** Verify nodes automatically cast votes

**Prerequisites:** Test 1 completed successfully

**Steps:**
```bash
# 1. Check AutoVoter logs
docker logs mpc-node-1 2>&1 | grep "Casting vote"

# Expected output (for each node):
# "Casting vote: node_id=1 tx_id=abc123... approve=true"

# 2. Wait 2 seconds for all votes to be processed

# 3. Check votes in database
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c \
  "SELECT node_id, approve FROM votes WHERE tx_id='abc123...' ORDER BY node_id;"

# Expected output:
 node_id | approve
---------+---------
       1 | t
       2 | t
       3 | t
       4 | t
       5 | t
(5 rows)

# 4. Check voting round updated
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c \
  "SELECT votes_received, approved, completed FROM voting_rounds WHERE tx_id='abc123...';"

# Expected output:
 votes_received | approved | completed
----------------+----------+-----------
              5 |        t |         t

# 5. Check transaction state transitioned to "approved"
curl http://localhost:8081/api/v1/transactions/abc123...

# Expected:
{
  "txid": "abc123...",
  "state": "approved",  # ← Changed from "voting"!
  ...
}
```

**Success Criteria:**
- ✅ 5 votes recorded in database
- ✅ votes_received = 5
- ✅ approved = true
- ✅ Transaction state is "approved"

---

### Test 3: MPC Signing

**Goal:** Verify transaction gets signed by MPC protocol

**Prerequisites:** Test 2 completed successfully (transaction in "approved" state)

**Steps:**
```bash
# 1. Wait for OrchestrationService to process approved transaction
# (Should happen automatically within 5 seconds)

# 2. Check transaction state
curl http://localhost:8081/api/v1/transactions/abc123...

# Expected progression:
# approved → signing → signed

# 3. Check for signed_tx in database
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c \
  "SELECT txid, state, LENGTH(signed_tx) as signed_tx_size FROM transactions WHERE txid='abc123...';"

# Expected:
     txid     | state  | signed_tx_size
--------------+--------+----------------
 abc123...    | signed |            250

# 4. Verify signature is valid Bitcoin transaction
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c \
  "SELECT encode(signed_tx, 'hex') FROM transactions WHERE txid='abc123...' LIMIT 1;" \
  > signed_tx.hex

# Use Bitcoin Core or online tool to decode:
bitcoin-cli -testnet decoderawtransaction $(cat signed_tx.hex)

# Expected: Valid transaction with signatures
```

**Success Criteria:**
- ✅ Transaction state is "signed"
- ✅ signed_tx is not NULL
- ✅ Signature is valid Bitcoin transaction format

---

### Test 4: Bitcoin Network Broadcasting

**Goal:** Verify signed transaction is broadcast to Bitcoin testnet

**Prerequisites:** Test 3 completed successfully (transaction signed)

**Steps:**
```bash
# 1. Wait for OrchestrationService broadcasting loop
# (Polls every 5 seconds)

# 2. Check transaction state
curl http://localhost:8081/api/v1/transactions/abc123...

# Expected:
{
  "txid": "abc123...",
  "state": "broadcasting",  # ← Changed!
  "bitcoin_txid": "def456..."  # ← New field!
}

# 3. Verify on blockchain explorer
# Go to: https://blockstream.info/testnet/tx/def456...
# Or use API:
curl https://blockstream.info/testnet/api/tx/def456...

# Expected: Transaction visible in mempool

# 4. Check audit log
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c \
  "SELECT event_type, details FROM audit_log WHERE tx_id='abc123...' ORDER BY timestamp DESC LIMIT 5;"

# Expected events:
# - transaction_broadcast
# - transaction_signed
# - voting_threshold_reached
# - voting_round_created
# - transaction_created
```

**Success Criteria:**
- ✅ Transaction state is "broadcasting" or "confirmed"
- ✅ bitcoin_txid is not NULL
- ✅ Transaction visible on blockchain explorer
- ✅ Audit trail complete

---

### Test 5: End-to-End Flow (Final Demo Test)

**Goal:** Verify complete transaction lifecycle from creation to blockchain

**Full Flow:**
```
User Request
     ↓
Create Transaction (pending)
     ↓
OrchestrationService detects → initiate_voting()
     ↓
Broadcast VoteRequest to all 5 nodes
     ↓
Each node's AutoVoter receives request
     ↓
Each node evaluates & casts vote
     ↓
VoteProcessor counts votes (5/5)
     ↓
Threshold reached (4/5 required)
     ↓
State: voting → approved
     ↓
OrchestrationService detects approved tx
     ↓
Initiate MPC signing protocol
     ↓
CGGMP24/FROST signing rounds via QUIC
     ↓
Signature generated
     ↓
State: approved → signing → signed
     ↓
OrchestrationService detects signed tx
     ↓
Broadcast to Bitcoin testnet
     ↓
State: signed → broadcasting
     ↓
Wait for confirmations
     ↓
State: broadcasting → confirmed
```

**Test Script:**

```bash
#!/bin/bash
# final_demo_test.sh

set -e

TXID=""
BITCOIN_TXID=""

echo "========================================="
echo "🎯 FINAL DEMO TEST - Complete Flow"
echo "========================================="
echo ""

# Step 1: Create transaction
echo "📝 Step 1: Creating transaction..."
RESPONSE=$(curl -s -X POST http://localhost:8081/api/v1/transactions \
  -H "Content-Type: application/json" \
  -d '{
    "recipient": "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx",
    "amount_sats": 50000,
    "metadata": "Demo transaction"
  }')

TXID=$(echo $RESPONSE | jq -r '.txid')
echo "✅ Transaction created: $TXID"
echo "   State: pending"
echo ""

# Step 2: Wait for voting initiation
echo "⏳ Step 2: Waiting for voting initiation (5s)..."
sleep 5

STATE=$(curl -s http://localhost:8081/api/v1/transactions/$TXID | jq -r '.state')
echo "✅ State changed to: $STATE"
echo ""

# Step 3: Wait for votes to be cast
echo "⏳ Step 3: Waiting for automatic votes (3s)..."
sleep 3

VOTES=$(docker exec mpc-postgres psql -U mpc -d mpc_wallet -t -c \
  "SELECT COUNT(*) FROM votes WHERE tx_id='$TXID';")
echo "✅ Votes received: $VOTES/5"
echo ""

# Step 4: Check threshold reached
APPROVED=$(docker exec mpc-postgres psql -U mpc -d mpc_wallet -t -c \
  "SELECT approved FROM voting_rounds WHERE tx_id='$TXID';")
echo "✅ Voting approved: $APPROVED"

STATE=$(curl -s http://localhost:8081/api/v1/transactions/$TXID | jq -r '.state')
echo "✅ State changed to: $STATE"
echo ""

# Step 5: Wait for MPC signing
echo "⏳ Step 5: Waiting for MPC signing (10s)..."
sleep 10

STATE=$(curl -s http://localhost:8081/api/v1/transactions/$TXID | jq -r '.state')
echo "✅ State changed to: $STATE"
echo ""

# Step 6: Check signed transaction
HAS_SIGNATURE=$(docker exec mpc-postgres psql -U mpc -d mpc_wallet -t -c \
  "SELECT signed_tx IS NOT NULL FROM transactions WHERE txid='$TXID';")
echo "✅ Transaction signed: $HAS_SIGNATURE"
echo ""

# Step 7: Wait for broadcasting
echo "⏳ Step 7: Waiting for Bitcoin broadcast (5s)..."
sleep 5

RESPONSE=$(curl -s http://localhost:8081/api/v1/transactions/$TXID)
STATE=$(echo $RESPONSE | jq -r '.state')
BITCOIN_TXID=$(echo $RESPONSE | jq -r '.bitcoin_txid // empty')

echo "✅ State changed to: $STATE"
if [ -n "$BITCOIN_TXID" ]; then
    echo "✅ Bitcoin TXID: $BITCOIN_TXID"
    echo ""
    echo "🌐 View on blockchain explorer:"
    echo "   https://blockstream.info/testnet/tx/$BITCOIN_TXID"
else
    echo "⚠️  Bitcoin TXID not yet available"
fi
echo ""

# Step 8: Final summary
echo "========================================="
echo "📊 FINAL SUMMARY"
echo "========================================="
echo "Internal TXID: $TXID"
echo "Bitcoin TXID:  $BITCOIN_TXID"
echo "Final State:   $STATE"
echo ""
echo "Timeline:"
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c \
  "SELECT event_type, timestamp FROM audit_log WHERE tx_id='$TXID' ORDER BY timestamp;"
echo ""
echo "✅ END-TO-END TEST COMPLETE!"
```

**Success Criteria:**
- ✅ Transaction progresses through all states: pending → voting → approved → signing → signed → broadcasting
- ✅ 5/5 votes cast
- ✅ Threshold reached (4/5)
- ✅ Transaction signed
- ✅ Broadcast to Bitcoin testnet
- ✅ Bitcoin txid obtained
- ✅ Visible on blockchain explorer
- ✅ Total time < 30 seconds

---

## 🚀 IMPLEMENTATION ORDER

### Day 1: Vote Request Broadcasting
1. Morning: Implement VoteRequest type and internal API endpoint
2. Afternoon: Add HTTP broadcasting to OrchestrationService
3. Evening: Test and verify vote requests reach all nodes

### Day 2: Automatic Voting
1. Morning: Implement AutoVoter module
2. Afternoon: Integrate into node startup and test
3. Evening: Verify votes are cast and threshold reached

### Day 3: Transaction Broadcasting & Integration
1. Morning: Implement Bitcoin broadcasting
2. Afternoon: End-to-end testing
3. Evening: Bug fixes and polish

---

## 📝 NOTES & CONSIDERATIONS

### Current Architecture Decisions

1. **Centralized PostgreSQL**: All nodes share one database
   - ✅ Pro: Simple state synchronization
   - ⚠️ Con: Single point of failure (acceptable for demo)
   - ⚠️ Con: Not true MPC security (acceptable for demo)

2. **HTTP for Vote Requests**: Using HTTP instead of QUIC P2P
   - ✅ Pro: Easier to implement and debug
   - ✅ Pro: HTTP infrastructure already exists
   - ⚠️ Con: Less secure than P2P (acceptable for demo)

3. **Auto-Approval Policy**: All valid transactions auto-approved
   - ✅ Pro: Demo works without manual intervention
   - ⚠️ Con: No real policy enforcement (add later)

### Security Issues (Deferred for Demo)

1. ❌ Key shares stored as plaintext JSON → Encrypt later
2. ❌ No authentication on APIs → Add auth later
3. ❌ No rate limiting → Add later
4. ❌ All nodes see all key shares → Separate DBs later

### Known Limitations

1. **No presignatures**: Signing might be slow (5-10s)
2. **No aux_info coordination**: CGGMP24 might fail (use FROST as fallback)
3. **Testnet only**: Real Bitcoin not supported yet
4. **No fee bumping**: Stuck transactions won't be replaced

---

## 🎬 FINAL DEMO SCRIPT

### Prerequisites
```bash
# Ensure all services running
docker-compose -f docker/docker-compose.yml up -d

# Check health
curl http://localhost:8081/api/v1/cluster/status
```

### Demo Flow

**Narrator:** "We'll now demonstrate a complete MPC transaction from creation to blockchain."

**Step 1:** Create transaction via API
```bash
curl -X POST http://localhost:8081/api/v1/transactions \
  -H "Content-Type: application/json" \
  -d '{
    "recipient": "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx",
    "amount_sats": 50000
  }' | jq
```

**Narrator:** "Transaction created with ID abc123. State is 'pending'."

**Step 2:** Show voting in progress
```bash
# Wait 5 seconds
sleep 5

# Show transaction state changed
curl http://localhost:8081/api/v1/transactions/abc123... | jq

# Show votes being cast (live logs)
docker logs -f mpc-node-1 | grep "Casting vote"
```

**Narrator:** "All 5 nodes automatically reviewed and approved the transaction."

**Step 3:** Show MPC signing
```bash
# Show state progression
watch -n 1 'curl -s http://localhost:8081/api/v1/transactions/abc123... | jq .state'
```

**Narrator:** "MPC signing protocol running across 5 nodes using CGGMP24..."

**Step 4:** Show broadcast
```bash
# Show Bitcoin txid
curl http://localhost:8081/api/v1/transactions/abc123... | jq '.bitcoin_txid'

# Open blockchain explorer
xdg-open https://blockstream.info/testnet/tx/def456...
```

**Narrator:** "Transaction successfully broadcast to Bitcoin testnet!"

---

## ✅ DEFINITION OF DONE

The demo is complete when:

1. ✅ A transaction can be created via API
2. ✅ Vote requests are automatically broadcast to all nodes
3. ✅ All 5 nodes automatically cast votes
4. ✅ Vote threshold (4/5) is detected
5. ✅ MPC signing protocol executes successfully
6. ✅ Signed transaction is generated
7. ✅ Transaction is broadcast to Bitcoin testnet
8. ✅ Transaction appears on blockchain explorer
9. ✅ Complete flow takes < 30 seconds
10. ✅ All steps logged in audit trail

---

**Last Updated:** 2026-01-27
**Status:** 🟡 READY TO IMPLEMENT
**Estimated Time:** 2-3 days
**Priority:** 🔴 CRITICAL - Required for demo
