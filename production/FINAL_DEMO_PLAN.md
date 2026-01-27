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

---

# 🎯 ADDITIONAL DEMO REQUIREMENTS - UI & Production Features

**Date Added:** 2026-01-27
**Timeline:** 1 Week
**Goal:** Complete production-ready demo with UI, user management, policy engine, and monitoring

---

## 📋 NEW REQUIREMENTS OVERVIEW

The following features must be added to complete the demo within 1 week:

1. **Wallet Creation System** - User wallet management and address derivation
2. **BackupNet Integration** - Integrate BackupNet component (from Cemil)
3. **API Gateway** - Request signing and routing layer
4. **TX Observer Component** - Real-time transaction state monitoring UI
5. **Log Screen** - Flow visualization and approval tracking
6. **Multi-User Demo** - 3-4 mock users with P2P transactions + Frontend + Login
7. **Admin Panel** - System monitoring + Policy engine configuration
8. **Policy Engine** - Per-user policies, whitelists, amount limits, timeouts
9. **ChainMonitor Component** - Blockchain state synchronization and balance tracking

---

## 🔴 REQUIREMENT #1: Wallet Creation System

### Problem & Requirements

**Current State:**
- Only L1 root key exists (from DKG)
- No user wallet management
- Cannot create individual user wallets
- No address derivation for users

**Required Functionality:**
1. Create user wallets from L1 root key using BIP32-style derivation
2. Each user wallet has threshold-distributed child shares across nodes
3. Generate Bitcoin addresses (P2WPKH for ECDSA, P2TR for Schnorr)
4. Store user wallet metadata (NOT the shares themselves)
5. API endpoints for wallet creation and management

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  Wallet Creation Flow                                            │
│                                                                   │
│  1. User Registration                                            │
│     │                                                             │
│     ├─► POST /api/v1/wallets                                     │
│     │    Body: { user_id: "alice", email: "alice@example.com" }  │
│     │                                                             │
│     ▼                                                             │
│  2. Generate Derivation Path                                     │
│     │   path = m/44'/0'/0'/0/user_index                          │
│     │   ρ = Hash(pk^root, user_id, chain_id, counter)           │
│     │                                                             │
│     ▼                                                             │
│  3. Each Node Derives Child Share                                │
│     │   ρ_j = F_ρ(j) mod q                                       │
│     │   sk_j^user = sk_j^root + ρ_j mod q                        │
│     │                                                             │
│     ▼                                                             │
│  4. Compute Public Key                                           │
│     │   pk^user = [sk^user]G                                     │
│     │                                                             │
│     ▼                                                             │
│  5. Derive Bitcoin Address                                       │
│     │   P2WPKH: bc1q... (ECDSA)                                  │
│     │   P2TR: bc1p... (Schnorr)                                  │
│     │                                                             │
│     ▼                                                             │
│  6. Store Metadata (NOT shares!)                                 │
│     │   PostgreSQL: wallets table                                │
│     │   - user_id, derivation_path, public_key, address         │
│     │   - L2 shares stored in RAM only!                          │
│     │                                                             │
│     ▼                                                             │
│  7. Return Wallet Info to User                                   │
│     {                                                             │
│       "wallet_id": "wallet_abc123",                              │
│       "address": "bc1q...",                                      │
│       "public_key": "02...",                                     │
│       "created_at": "2026-01-27T..."                             │
│     }                                                             │
└─────────────────────────────────────────────────────────────────┘
```

### Database Schema

**New Table: `wallets`**
```sql
CREATE TABLE wallets (
    wallet_id VARCHAR(64) PRIMARY KEY,
    user_id VARCHAR(64) NOT NULL UNIQUE,
    email VARCHAR(255),
    derivation_path VARCHAR(255) NOT NULL,
    public_key_hex TEXT NOT NULL,
    address_p2wpkh VARCHAR(100),  -- Native SegWit
    address_p2tr VARCHAR(100),     -- Taproot
    protocol VARCHAR(20) NOT NULL, -- 'cggmp24' or 'frost'
    created_at TIMESTAMP DEFAULT NOW(),
    balance_sats BIGINT DEFAULT 0,
    last_sync_at TIMESTAMP,

    INDEX idx_user_id (user_id),
    INDEX idx_address_p2wpkh (address_p2wpkh),
    INDEX idx_address_p2tr (address_p2tr)
);
```

### Implementation Steps

#### Step 1: Child Share Derivation Module

**File:** `crates/crypto/src/child_derivation.rs` (NEW FILE)

```rust
//! L2 Child Share Derivation for User Wallets
//!
//! Implements deterministic child share derivation from L1 root shares
//! according to the formula: sk_j^user = sk_j^root + ρ_j mod q

use anyhow::{Result, bail};
use sha2::{Sha256, Digest};
use generic_ec::{Scalar, Point, curves::Secp256k1};

/// Derive child share for a specific node
///
/// # Arguments
/// * `root_share` - Node's L1 root key share (sk_j^root)
/// * `user_id` - Unique user identifier
/// * `chain_id` - Blockchain identifier (e.g., "bitcoin-mainnet")
/// * `counter` - Derivation counter (for multiple wallets per user)
///
/// # Returns
/// Child share: sk_j^user = sk_j^root + ρ_j mod q
pub fn derive_child_share(
    root_share: &Scalar<Secp256k1>,
    user_id: &str,
    chain_id: &str,
    counter: u32,
) -> Result<Scalar<Secp256k1>> {
    // 1. Compute base derivation value ρ
    let rho = compute_rho(user_id, chain_id, counter)?;

    // 2. Derive node-specific offset ρ_j = F_ρ(j)
    // For demo: use rho directly (in production: use PRF)
    let rho_scalar = Scalar::<Secp256k1>::from_be_bytes(&rho)
        .map_err(|e| anyhow::anyhow!("Failed to convert rho to scalar: {}", e))?;

    // 3. Compute child share: sk_j^user = sk_j^root + ρ_j mod q
    let child_share = root_share + &rho_scalar;

    Ok(child_share)
}

/// Compute base derivation value ρ = Hash(user_id || chain_id || counter)
fn compute_rho(user_id: &str, chain_id: &str, counter: u32) -> Result<[u8; 32]> {
    let mut hasher = Sha256::new();
    hasher.update(b"MPC_WALLET_L2_DERIVATION");
    hasher.update(user_id.as_bytes());
    hasher.update(chain_id.as_bytes());
    hasher.update(counter.to_le_bytes());

    let hash = hasher.finalize();
    let mut result = [0u8; 32];
    result.copy_from_slice(&hash);

    Ok(result)
}

/// Derive public key from child shares (aggregation)
///
/// pk^user = [sk_1^user + sk_2^user + ... + sk_n^user]G
///          = [sk^user]G
pub fn aggregate_child_public_key(
    child_shares: &[Scalar<Secp256k1>],
    threshold: usize,
) -> Result<Point<Secp256k1>> {
    if child_shares.len() < threshold {
        bail!("Not enough child shares: {} < {}", child_shares.len(), threshold);
    }

    // Sum the first `threshold` shares
    let mut total = Scalar::<Secp256k1>::zero();
    for share in &child_shares[..threshold] {
        total = total + share;
    }

    // Multiply by generator to get public key
    let public_key = Point::<Secp256k1>::generator() * &total;

    Ok(public_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_child_derivation_deterministic() {
        let root_share = Scalar::<Secp256k1>::random(&mut rand::thread_rng());

        let child1 = derive_child_share(&root_share, "alice", "bitcoin", 0).unwrap();
        let child2 = derive_child_share(&root_share, "alice", "bitcoin", 0).unwrap();

        // Same inputs should produce same output
        assert_eq!(child1, child2);
    }

    #[test]
    fn test_different_users_different_shares() {
        let root_share = Scalar::<Secp256k1>::random(&mut rand::thread_rng());

        let alice_share = derive_child_share(&root_share, "alice", "bitcoin", 0).unwrap();
        let bob_share = derive_child_share(&root_share, "bob", "bitcoin", 0).unwrap();

        assert_ne!(alice_share, bob_share);
    }
}
```

#### Step 2: Wallet Service

**File:** `crates/wallet/src/service.rs` (NEW FILE)

```rust
//! User Wallet Management Service

use std::sync::Arc;
use anyhow::Result;
use tracing::{info, error};
use threshold_storage::PostgresStorage;
use threshold_crypto::child_derivation;
use common::bitcoin_address::{derive_p2wpkh_address, BitcoinNetwork};

pub struct WalletService {
    postgres: Arc<PostgresStorage>,
    node_id: u64,
    network: BitcoinNetwork,
}

impl WalletService {
    pub fn new(
        postgres: Arc<PostgresStorage>,
        node_id: u64,
        network: BitcoinNetwork,
    ) -> Self {
        Self { postgres, node_id, network }
    }

    /// Create a new user wallet
    pub async fn create_wallet(
        &self,
        user_id: &str,
        email: Option<&str>,
        protocol: &str,
    ) -> Result<WalletInfo> {
        info!("Creating wallet for user: {}", user_id);

        // 1. Check if user already has a wallet
        if self.postgres.get_wallet_by_user_id(user_id).await?.is_some() {
            return Err(anyhow::anyhow!("User already has a wallet"));
        }

        // 2. Load L1 root key share for this node
        let root_share = self.postgres.get_root_key_share(self.node_id, protocol).await?
            .ok_or_else(|| anyhow::anyhow!("Root key share not found for node {}", self.node_id))?;

        // 3. Derive L2 child share
        let child_share = child_derivation::derive_child_share(
            &root_share.scalar,
            user_id,
            "bitcoin-testnet",
            0, // counter = 0 for first wallet
        )?;

        // 4. Store child share IN RAM ONLY (for demo: use in-memory cache)
        // In production: use secure volatile memory
        self.cache_child_share(user_id, child_share.clone()).await?;

        // 5. Broadcast child share derivation to all nodes via P2P
        // (All nodes must derive same child share from their root shares)
        self.broadcast_child_derivation(user_id, protocol).await?;

        // 6. Aggregate public key from all nodes' child shares
        // (Requires coordination - for demo: use local computation)
        let child_public_key = self.compute_child_public_key(user_id).await?;

        // 7. Derive Bitcoin addresses
        let pubkey_bytes = child_public_key.to_bytes();
        let address_p2wpkh = derive_p2wpkh_address(&pubkey_bytes, self.network)?;

        // 8. Store wallet metadata in PostgreSQL
        let wallet_id = uuid::Uuid::new_v4().to_string();
        self.postgres.insert_wallet(&Wallet {
            wallet_id: wallet_id.clone(),
            user_id: user_id.to_string(),
            email: email.map(String::from),
            derivation_path: format!("m/44'/0'/0'/0/{}", 0),
            public_key_hex: hex::encode(&pubkey_bytes),
            address_p2wpkh: Some(address_p2wpkh.clone()),
            address_p2tr: None, // TODO: Schnorr support
            protocol: protocol.to_string(),
            created_at: chrono::Utc::now(),
            balance_sats: 0,
            last_sync_at: None,
        }).await?;

        info!("Wallet created successfully: address={}", address_p2wpkh);

        Ok(WalletInfo {
            wallet_id,
            user_id: user_id.to_string(),
            address: address_p2wpkh,
            public_key: hex::encode(&pubkey_bytes),
            created_at: chrono::Utc::now(),
        })
    }

    /// Get wallet info by user ID
    pub async fn get_wallet(&self, user_id: &str) -> Result<Option<WalletInfo>> {
        let wallet = self.postgres.get_wallet_by_user_id(user_id).await?;
        Ok(wallet.map(|w| WalletInfo {
            wallet_id: w.wallet_id,
            user_id: w.user_id,
            address: w.address_p2wpkh.unwrap_or_default(),
            public_key: w.public_key_hex,
            created_at: w.created_at,
        }))
    }

    /// List all wallets
    pub async fn list_wallets(&self) -> Result<Vec<WalletInfo>> {
        let wallets = self.postgres.get_all_wallets().await?;
        Ok(wallets.into_iter().map(|w| WalletInfo {
            wallet_id: w.wallet_id,
            user_id: w.user_id,
            address: w.address_p2wpkh.unwrap_or_default(),
            public_key: w.public_key_hex,
            created_at: w.created_at,
        }).collect())
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WalletInfo {
    pub wallet_id: String,
    pub user_id: String,
    pub address: String,
    pub public_key: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
```

#### Step 3: Wallet API Endpoints

**File:** `crates/api/src/handlers/wallet.rs` (NEW FILE)

```rust
//! Wallet Management API Handlers

use axum::{extract::State, Json, extract::Path};
use crate::error::ApiError;
use crate::state::AppState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct CreateWalletRequest {
    pub user_id: String,
    pub email: Option<String>,
    #[serde(default = "default_protocol")]
    pub protocol: String,
}

fn default_protocol() -> String {
    "cggmp24".to_string()
}

#[derive(Debug, Serialize)]
pub struct CreateWalletResponse {
    pub wallet_id: String,
    pub user_id: String,
    pub address: String,
    pub public_key: String,
    pub created_at: String,
}

/// Create a new user wallet
///
/// POST /api/v1/wallets
pub async fn create_wallet(
    State(state): State<AppState>,
    Json(req): Json<CreateWalletRequest>,
) -> Result<Json<CreateWalletResponse>, ApiError> {
    // Validate user_id
    if req.user_id.is_empty() {
        return Err(ApiError::BadRequest("user_id cannot be empty".to_string()));
    }

    // Create wallet
    let wallet_info = state.wallet_service
        .create_wallet(&req.user_id, req.email.as_deref(), &req.protocol)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))?;

    Ok(Json(CreateWalletResponse {
        wallet_id: wallet_info.wallet_id,
        user_id: wallet_info.user_id,
        address: wallet_info.address,
        public_key: wallet_info.public_key,
        created_at: wallet_info.created_at.to_rfc3339(),
    }))
}

/// Get wallet by user ID
///
/// GET /api/v1/wallets/:user_id
pub async fn get_wallet(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<CreateWalletResponse>, ApiError> {
    let wallet = state.wallet_service
        .get_wallet(&user_id)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Wallet not found for user: {}", user_id)))?;

    Ok(Json(CreateWalletResponse {
        wallet_id: wallet.wallet_id,
        user_id: wallet.user_id,
        address: wallet.address,
        public_key: wallet.public_key,
        created_at: wallet.created_at.to_rfc3339(),
    }))
}

/// List all wallets
///
/// GET /api/v1/wallets
pub async fn list_wallets(
    State(state): State<AppState>,
) -> Result<Json<Vec<CreateWalletResponse>>, ApiError> {
    let wallets = state.wallet_service
        .list_wallets()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))?;

    let response = wallets.into_iter().map(|w| CreateWalletResponse {
        wallet_id: w.wallet_id,
        user_id: w.user_id,
        address: w.address,
        public_key: w.public_key,
        created_at: w.created_at.to_rfc3339(),
    }).collect();

    Ok(Json(response))
}
```

**File:** `crates/api/src/routes/wallet.rs` (NEW FILE)

```rust
//! Wallet API Routes

use crate::handlers::wallet;
use crate::state::AppState;
use axum::{
    routing::{get, post},
    Router,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", post(wallet::create_wallet))
        .route("/", get(wallet::list_wallets))
        .route("/:user_id", get(wallet::get_wallet))
}
```

### Test Plan

```bash
# Test 1: Create wallet for Alice
curl -X POST http://localhost:8081/api/v1/wallets \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "alice",
    "email": "alice@example.com",
    "protocol": "cggmp24"
  }'

# Expected:
{
  "wallet_id": "wallet_abc123",
  "user_id": "alice",
  "address": "tb1q...",
  "public_key": "02...",
  "created_at": "2026-01-27T..."
}

# Test 2: Get wallet
curl http://localhost:8081/api/v1/wallets/alice

# Test 3: List all wallets
curl http://localhost:8081/api/v1/wallets

# Test 4: Verify in database
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c \
  "SELECT user_id, address_p2wpkh, public_key_hex FROM wallets;"
```

---

## 🔴 REQUIREMENT #2: BackupNet Integration

### Problem & Requirements

**Current State:**
- No backup mechanism for key shares or metadata
- System cannot recover from node failures
- Data loss risk

**Required Functionality:**
1. Integrate BackupNet component from Cemil
2. Backup L1 root key share metadata (encrypted)
3. Backup L2 user wallet metadata (derivation info only, NOT shares)
4. Restore capability after node restart
5. Encrypted backup storage

### Architecture

```
┌───────────────────────────────────────────────────────────────┐
│  BackupNet Architecture                                        │
│                                                                 │
│  Node-1                          BackupNet Service             │
│  ┌─────────────────┐            ┌────────────────────────┐    │
│  │ Key Share       │──backup──►│  Encrypted Storage     │    │
│  │ Metadata        │            │  - S3/MinIO/IPFS       │    │
│  └─────────────────┘            │  - Version Control     │    │
│                                  │  - Redundancy          │    │
│  ┌─────────────────┐  restore   └────────────────────────┘    │
│  │ Wallet Metadata │◄───────────────────┘                      │
│  │ (Derivation)    │                                           │
│  └─────────────────┘                                           │
│                                                                 │
│  CRITICAL: L2 child shares NEVER backed up!                    │
│  Only metadata needed for re-derivation.                       │
└───────────────────────────────────────────────────────────────┘
```

### Implementation Steps

#### Step 1: BackupNet Client Module

**File:** `crates/backupnet/src/client.rs` (NEW FILE - integrate from Cemil)

```rust
//! BackupNet Client for Encrypted Backup Storage

use anyhow::Result;
use serde::{Serialize, Deserialize};
use tracing::{info, error};

/// BackupNet client configuration
pub struct BackupNetClient {
    endpoint: String,
    encryption_key: Vec<u8>,
    node_id: u64,
}

impl BackupNetClient {
    pub fn new(endpoint: String, encryption_key: Vec<u8>, node_id: u64) -> Self {
        Self { endpoint, encryption_key, node_id }
    }

    /// Backup key share metadata (encrypted)
    pub async fn backup_key_share_metadata(
        &self,
        ceremony_id: &str,
        metadata: &KeyShareMetadata,
    ) -> Result<String> {
        info!("Backing up key share metadata for ceremony: {}", ceremony_id);

        // 1. Serialize metadata
        let json = serde_json::to_vec(metadata)?;

        // 2. Encrypt using AES-256-GCM
        let encrypted = self.encrypt(&json)?;

        // 3. Upload to BackupNet
        let backup_id = self.upload(&encrypted, &format!("keyshare/{}/{}", ceremony_id, self.node_id)).await?;

        info!("Key share metadata backed up: backup_id={}", backup_id);
        Ok(backup_id)
    }

    /// Backup wallet metadata (derivation info only)
    pub async fn backup_wallet_metadata(
        &self,
        user_id: &str,
        wallet_metadata: &WalletMetadata,
    ) -> Result<String> {
        info!("Backing up wallet metadata for user: {}", user_id);

        let json = serde_json::to_vec(wallet_metadata)?;
        let encrypted = self.encrypt(&json)?;
        let backup_id = self.upload(&encrypted, &format!("wallet/{}", user_id)).await?;

        info!("Wallet metadata backed up: backup_id={}", backup_id);
        Ok(backup_id)
    }

    /// Restore key share metadata
    pub async fn restore_key_share_metadata(
        &self,
        ceremony_id: &str,
    ) -> Result<KeyShareMetadata> {
        info!("Restoring key share metadata for ceremony: {}", ceremony_id);

        let path = format!("keyshare/{}/{}", ceremony_id, self.node_id);
        let encrypted = self.download(&path).await?;
        let decrypted = self.decrypt(&encrypted)?;
        let metadata: KeyShareMetadata = serde_json::from_slice(&decrypted)?;

        Ok(metadata)
    }

    /// Encrypt data using AES-256-GCM
    fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        // TODO: Implement AES-256-GCM encryption
        // For now: placeholder
        Ok(data.to_vec())
    }

    /// Decrypt data
    fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        // TODO: Implement AES-256-GCM decryption
        Ok(data.to_vec())
    }

    /// Upload to BackupNet storage
    async fn upload(&self, data: &[u8], path: &str) -> Result<String> {
        // TODO: Integrate Cemil's BackupNet upload API
        let backup_id = uuid::Uuid::new_v4().to_string();
        Ok(backup_id)
    }

    /// Download from BackupNet storage
    async fn download(&self, path: &str) -> Result<Vec<u8>> {
        // TODO: Integrate Cemil's BackupNet download API
        Ok(vec![])
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KeyShareMetadata {
    pub ceremony_id: String,
    pub node_id: u64,
    pub threshold: u32,
    pub public_key_hex: String,
    pub protocol: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WalletMetadata {
    pub user_id: String,
    pub derivation_path: String,
    pub public_key_hex: String,
    pub address: String,
    pub protocol: String,
}
```

#### Step 2: Integrate BackupNet into Services

**File:** `crates/dkg/src/service.rs`

Add backup call after key generation:

```rust
// After successful DKG
let metadata = KeyShareMetadata {
    ceremony_id: ceremony.ceremony_id.clone(),
    node_id: self.node_id,
    threshold: ceremony.threshold,
    public_key_hex: hex::encode(&public_key),
    protocol: ceremony.protocol.clone(),
    created_at: chrono::Utc::now(),
};

// Backup to BackupNet
if let Err(e) = self.backupnet_client.backup_key_share_metadata(&ceremony.ceremony_id, &metadata).await {
    error!("Failed to backup key share metadata: {}", e);
}
```

### Test Plan

```bash
# Test 1: Verify backup after DKG
curl -X POST http://localhost:8081/api/v1/dkg/initiate \
  -H "Content-Type: application/json" \
  -d '{
    "protocol": "cggmp24",
    "threshold": 4,
    "total_parties": 5
  }'

# Check logs for backup confirmation
docker logs mpc-node-1 2>&1 | grep "backed up"

# Test 2: Node restart and restore
docker restart mpc-node-1
docker logs mpc-node-1 2>&1 | grep "Restored.*metadata"
```

---

## 🔴 REQUIREMENT #3: API Gateway

### Problem & Requirements

**Current State:**
- Direct API access to nodes
- No request signing or authentication
- No rate limiting
- No request routing

**Required Functionality:**
1. Single entry point for all API requests
2. Request signing and verification
3. JWT authentication
4. Rate limiting per user
5. Request routing to appropriate nodes
6. Load balancing

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  API Gateway Architecture                                        │
│                                                                   │
│  Client (UI/Mobile)                                              │
│       │                                                           │
│       ├─► POST /api/v1/transactions                              │
│       │   Header: Authorization: Bearer <JWT>                    │
│       │   Header: X-Request-Signature: <HMAC-SHA256>            │
│       │                                                           │
│       ▼                                                           │
│  ┌────────────────────────────────────────────────────┐          │
│  │  API Gateway (Port 8080)                           │          │
│  │  ┌──────────────────────────────────────────────┐ │          │
│  │  │  1. JWT Validation                           │ │          │
│  │  │  2. Request Signature Verification           │ │          │
│  │  │  3. Rate Limiting Check                      │ │          │
│  │  │  4. Route to Appropriate Node                │ │          │
│  │  └──────────────────────────────────────────────┘ │          │
│  └────────────────────────────────────────────────────┘          │
│       │                                                           │
│       ├────────────┬────────────┬────────────┐                   │
│       ▼            ▼            ▼            ▼                   │
│  [Node-1:8081] [Node-2:8082] [Node-3:8083] [Node-4:8084]        │
└─────────────────────────────────────────────────────────────────┘
```

### Implementation Steps

#### Step 1: API Gateway Service

**File:** `crates/gateway/src/main.rs` (NEW CRATE)

```rust
//! API Gateway - Request Signing, Authentication, and Routing

use axum::{
    Router,
    routing::{get, post},
    middleware,
    extract::{State, Request},
    response::{Response, IntoResponse},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let config = GatewayConfig::from_env()?;
    let state = Arc::new(GatewayState::new(config));

    let app = Router::new()
        // Public endpoints (no auth)
        .route("/health", get(health_check))
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/auth/register", post(auth::register))

        // Protected endpoints (require JWT + signature)
        .route("/api/v1/wallets", post(proxy_create_wallet))
        .route("/api/v1/wallets/:user_id", get(proxy_get_wallet))
        .route("/api/v1/transactions", post(proxy_create_transaction))
        .route("/api/v1/transactions/:txid", get(proxy_get_transaction))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit_middleware,
        ))
        .layer(CorsLayer::permissive())
        .with_state(state);

    info!("API Gateway starting on 0.0.0.0:8080");

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    axum::serve(listener, app).await?;

    Ok(())
}

struct GatewayState {
    http_client: reqwest::Client,
    node_endpoints: Vec<String>,
    jwt_secret: String,
    hmac_secret: Vec<u8>,
    rate_limiter: Arc<RateLimiter>,
}

impl GatewayState {
    fn new(config: GatewayConfig) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            node_endpoints: config.node_endpoints,
            jwt_secret: config.jwt_secret,
            hmac_secret: config.hmac_secret.into_bytes(),
            rate_limiter: Arc::new(RateLimiter::new()),
        }
    }

    /// Select node using round-robin load balancing
    fn select_node(&self) -> &str {
        let index = rand::random::<usize>() % self.node_endpoints.len();
        &self.node_endpoints[index]
    }
}

/// Authentication middleware
async fn auth_middleware(
    State(state): State<Arc<GatewayState>>,
    mut request: Request,
    next: middleware::Next,
) -> Result<Response, StatusCode> {
    // 1. Extract JWT from Authorization header
    let auth_header = request.headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token = auth_header.strip_prefix("Bearer ")
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // 2. Verify JWT
    let claims = verify_jwt(token, &state.jwt_secret)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    // 3. Extract request signature
    let signature_header = request.headers()
        .get("X-Request-Signature")
        .and_then(|h| h.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;

    // 4. Verify request signature (HMAC-SHA256)
    // TODO: Read request body and verify HMAC

    // 5. Add user_id to request extensions
    request.extensions_mut().insert(claims.user_id.clone());

    Ok(next.run(request).await)
}

/// Rate limiting middleware
async fn rate_limit_middleware(
    State(state): State<Arc<GatewayState>>,
    request: Request,
    next: middleware::Next,
) -> Result<Response, StatusCode> {
    let user_id = request.extensions()
        .get::<String>()
        .cloned()
        .unwrap_or_else(|| "anonymous".to_string());

    if !state.rate_limiter.check_rate_limit(&user_id).await {
        warn!("Rate limit exceeded for user: {}", user_id);
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    Ok(next.run(request).await)
}

/// Proxy request to node
async fn proxy_create_wallet(
    State(state): State<Arc<GatewayState>>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let node_endpoint = state.select_node();
    let url = format!("{}/api/v1/wallets", node_endpoint);

    let response = state.http_client
        .post(&url)
        .json(&req)
        .send()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let json = response.json().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json))
}

fn verify_jwt(token: &str, secret: &str) -> anyhow::Result<Claims> {
    // TODO: Implement JWT verification using jsonwebtoken crate
    Ok(Claims {
        user_id: "alice".to_string(),
        exp: 0,
    })
}

struct Claims {
    user_id: String,
    exp: u64,
}

struct RateLimiter {
    // TODO: Implement token bucket or sliding window
}

impl RateLimiter {
    fn new() -> Self {
        Self {}
    }

    async fn check_rate_limit(&self, user_id: &str) -> bool {
        // TODO: Check Redis for rate limit counters
        // For demo: allow all requests
        true
    }
}
```

### Test Plan

```bash
# Test 1: Login and get JWT
curl -X POST http://localhost:8080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{
    "username": "alice",
    "password": "password123"
  }'

# Expected:
{
  "token": "eyJhbGciOiJIUzI1NiIs...",
  "user_id": "alice"
}

# Test 2: Create wallet with JWT
curl -X POST http://localhost:8080/api/v1/wallets \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer eyJhbGciOiJIUzI1NiIs..." \
  -H "X-Request-Signature: abc123..." \
  -d '{
    "user_id": "alice",
    "protocol": "cggmp24"
  }'

# Test 3: Rate limiting
# Send 100 requests rapidly
for i in {1..100}; do
  curl -X POST http://localhost:8080/api/v1/transactions \
    -H "Authorization: Bearer ..." \
    -d '{}' &
done

# Expected: Some requests return 429 Too Many Requests
```

---

## 🔴 REQUIREMENT #4: TX Observer Component

### Problem & Requirements

**Current State:**
- No UI for transaction monitoring
- Cannot see transaction state transitions
- No real-time updates

**Required Functionality:**
1. Real-time transaction state monitoring UI
2. Show all state transitions (pending → voting → approved → signing → signed → broadcasting → confirmed)
3. Display voting progress (votes cast, threshold status)
4. Show blockchain confirmation status (mempool, 1 conf, 6 confs)
5. Historical transaction list per user
6. Filter and search transactions

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  TX Observer Component Architecture                              │
│                                                                   │
│  Frontend (React)                                                │
│  ┌────────────────────────────────────────────────────────┐     │
│  │  Transaction Dashboard                                  │     │
│  │  ┌─────────────────────────────────────────────────┐   │     │
│  │  │  Transaction List                                │   │     │
│  │  │  [tx_1] → Broadcasting (3/6 confirmations) ✓   │   │     │
│  │  │  [tx_2] → Signing (MPC in progress...) ⏳       │   │     │
│  │  │  [tx_3] → Voting (4/5 votes) ⏳                 │   │     │
│  │  └─────────────────────────────────────────────────┘   │     │
│  │                                                          │     │
│  │  Transaction Detail View                                │     │
│  │  ┌─────────────────────────────────────────────────┐   │     │
│  │  │  TXID: tx_abc123                                │   │     │
│  │  │  State: Broadcasting                             │   │     │
│  │  │  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━   │   │     │
│  │  │  ✅ Created     (2026-01-27 10:00:00)          │   │     │
│  │  │  ✅ Voting      (5/5 votes, approved)           │   │     │
│  │  │  ✅ Signed      (MPC signature generated)       │   │     │
│  │  │  ⏳ Broadcasting (waiting for confirmations)    │   │     │
│  │  │                                                  │   │     │
│  │  │  Bitcoin TXID: def456...                        │   │     │
│  │  │  Confirmations: 3/6 ━━━━━━━░░░░░░              │   │     │
│  │  │  Mempool: ✅ | Block: 2,500,123                │   │     │
│  │  └─────────────────────────────────────────────────┘   │     │
│  └────────────────────────────────────────────────────────┘     │
│       │                                                           │
│       │ WebSocket /ws/transactions (real-time updates)           │
│       │                                                           │
│       ▼                                                           │
│  ┌────────────────────────────────────────────────────────┐     │
│  │  TX Observer Backend Service                           │     │
│  │  - Query PostgreSQL for transaction states             │     │
│  │  - Query ChainMonitor for blockchain status            │     │
│  │  - WebSocket server for real-time updates              │     │
│  │  - Event stream from OrchestrationService              │     │
│  └────────────────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────────────────┘
```

### Database Enhancements

**Add transaction confirmation tracking:**

```sql
-- Add columns to transactions table
ALTER TABLE transactions
ADD COLUMN blockchain_confirmations INT DEFAULT 0,
ADD COLUMN first_seen_block INT,
ADD COLUMN confirmed_block INT,
ADD COLUMN confirmed_at TIMESTAMP;

-- Create transaction events table for audit trail
CREATE TABLE transaction_events (
    id SERIAL PRIMARY KEY,
    tx_id VARCHAR(64) NOT NULL,
    event_type VARCHAR(50) NOT NULL,
    old_state VARCHAR(50),
    new_state VARCHAR(50),
    details JSONB,
    timestamp TIMESTAMP DEFAULT NOW(),

    INDEX idx_tx_id (tx_id),
    INDEX idx_timestamp (timestamp)
);
```

### Implementation Steps

#### Step 1: TX Observer Service

**File:** `crates/observer/src/service.rs` (NEW FILE)

```rust
//! Transaction Observer Service
//! Provides real-time transaction monitoring and blockchain status

use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, error};
use threshold_storage::PostgresStorage;

pub struct TxObserverService {
    postgres: Arc<PostgresStorage>,
    chain_monitor: Arc<ChainMonitor>,
    event_tx: broadcast::Sender<TransactionEvent>,
}

impl TxObserverService {
    pub fn new(
        postgres: Arc<PostgresStorage>,
        chain_monitor: Arc<ChainMonitor>,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(1000);
        Self {
            postgres,
            chain_monitor,
            event_tx,
        }
    }

    /// Get transaction with full status including blockchain state
    pub async fn get_transaction_status(
        &self,
        txid: &str,
    ) -> anyhow::Result<TransactionStatus> {
        // 1. Get transaction from database
        let tx = self.postgres.get_transaction(txid).await?
            .ok_or_else(|| anyhow::anyhow!("Transaction not found"))?;

        // 2. Get blockchain status if broadcast
        let chain_status = if let Some(bitcoin_txid) = &tx.bitcoin_txid {
            self.chain_monitor.get_transaction_status(bitcoin_txid).await.ok()
        } else {
            None
        };

        // 3. Get voting progress if in voting state
        let voting_progress = if tx.state == "voting" {
            self.postgres.get_voting_round(&tx.txid).await.ok()
        } else {
            None
        };

        // 4. Get event timeline
        let events = self.postgres.get_transaction_events(&tx.txid).await?;

        Ok(TransactionStatus {
            txid: tx.txid,
            state: tx.state,
            amount_sats: tx.amount_sats,
            recipient: tx.recipient,
            sender_wallet_id: tx.sender_wallet_id,
            created_at: tx.created_at,
            bitcoin_txid: tx.bitcoin_txid,
            chain_status,
            voting_progress,
            events,
        })
    }

    /// List transactions for a user with filters
    pub async fn list_user_transactions(
        &self,
        user_id: &str,
        filter: TransactionFilter,
    ) -> anyhow::Result<Vec<TransactionStatus>> {
        let txs = self.postgres.get_transactions_by_user(user_id, &filter).await?;

        let mut statuses = Vec::new();
        for tx in txs {
            if let Ok(status) = self.get_transaction_status(&tx.txid).await {
                statuses.push(status);
            }
        }

        Ok(statuses)
    }

    /// Subscribe to transaction events (for WebSocket)
    pub fn subscribe(&self) -> broadcast::Receiver<TransactionEvent> {
        self.event_tx.subscribe()
    }

    /// Publish transaction event (called by OrchestrationService)
    pub fn publish_event(&self, event: TransactionEvent) {
        if let Err(e) = self.event_tx.send(event) {
            error!("Failed to publish transaction event: {}", e);
        }
    }

    /// Background task to update blockchain confirmations
    pub async fn start_confirmation_monitor(self: Arc<Self>) {
        info!("Starting confirmation monitor");

        loop {
            if let Err(e) = self.update_confirmations().await {
                error!("Failed to update confirmations: {}", e);
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
        }
    }

    async fn update_confirmations(&self) -> anyhow::Result<()> {
        // Get all transactions in broadcasting state
        let txs = self.postgres.get_transactions_by_state("broadcasting").await?;

        for tx in txs {
            if let Some(bitcoin_txid) = &tx.bitcoin_txid {
                // Query blockchain for confirmation count
                match self.chain_monitor.get_transaction_status(bitcoin_txid).await {
                    Ok(status) if status.confirmations >= 6 => {
                        // Fully confirmed!
                        info!("Transaction {} confirmed (6+ confirmations)", tx.txid);

                        self.postgres.update_transaction_state(&tx.txid, "confirmed").await?;
                        self.postgres.update_transaction_confirmations(&tx.txid, status.confirmations).await?;

                        self.publish_event(TransactionEvent {
                            txid: tx.txid.clone(),
                            event_type: "transaction_confirmed".to_string(),
                            old_state: Some("broadcasting".to_string()),
                            new_state: Some("confirmed".to_string()),
                            details: serde_json::json!({
                                "confirmations": status.confirmations,
                                "block_height": status.block_height,
                            }),
                            timestamp: chrono::Utc::now(),
                        });
                    }
                    Ok(status) => {
                        // Update confirmation count
                        self.postgres.update_transaction_confirmations(&tx.txid, status.confirmations).await?;
                    }
                    Err(e) => {
                        error!("Failed to get blockchain status for {}: {}", bitcoin_txid, e);
                    }
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TransactionStatus {
    pub txid: String,
    pub state: String,
    pub amount_sats: i64,
    pub recipient: String,
    pub sender_wallet_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub bitcoin_txid: Option<String>,
    pub chain_status: Option<ChainStatus>,
    pub voting_progress: Option<VotingProgress>,
    pub events: Vec<TransactionEvent>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChainStatus {
    pub confirmations: u32,
    pub block_height: Option<u64>,
    pub in_mempool: bool,
    pub fee_sats: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VotingProgress {
    pub votes_received: u32,
    pub threshold: u32,
    pub approved: bool,
    pub votes: Vec<VoteInfo>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VoteInfo {
    pub node_id: u64,
    pub approve: bool,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TransactionEvent {
    pub txid: String,
    pub event_type: String,
    pub old_state: Option<String>,
    pub new_state: Option<String>,
    pub details: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

pub struct TransactionFilter {
    pub state: Option<String>,
    pub limit: usize,
    pub offset: usize,
}
```

#### Step 2: WebSocket API for Real-time Updates

**File:** `crates/api/src/handlers/ws.rs` (NEW FILE)

```rust
//! WebSocket handler for real-time transaction updates

use axum::{
    extract::{ws::{WebSocket, WebSocketUpgrade}, State},
    response::Response,
};
use futures::{StreamExt, SinkExt};
use tracing::{info, error};
use crate::state::AppState;

/// WebSocket endpoint for transaction updates
///
/// GET /ws/transactions
pub async fn transaction_updates(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    info!("WebSocket client connected");

    let (mut sender, mut receiver) = socket.split();

    // Subscribe to transaction events
    let mut event_rx = state.tx_observer.subscribe();

    // Forward events to WebSocket client
    let mut send_task = tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            let json = serde_json::to_string(&event).unwrap();

            if sender.send(axum::extract::ws::Message::Text(json)).await.is_err() {
                break;
            }
        }
    });

    // Handle incoming messages (ping/pong)
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                axum::extract::ws::Message::Ping(data) => {
                    // Respond to ping
                }
                axum::extract::ws::Message::Close(_) => {
                    break;
                }
                _ => {}
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = &mut send_task => {
            recv_task.abort();
        }
        _ = &mut recv_task => {
            send_task.abort();
        }
    }

    info!("WebSocket client disconnected");
}
```

#### Step 3: Frontend Component (React)

**File:** `frontend/src/components/TransactionObserver.tsx` (NEW FILE)

```typescript
import React, { useEffect, useState } from 'react';
import useWebSocket from 'react-use-websocket';

interface Transaction {
  txid: string;
  state: string;
  amount_sats: number;
  recipient: string;
  created_at: string;
  bitcoin_txid?: string;
  chain_status?: {
    confirmations: number;
    in_mempool: boolean;
  };
  voting_progress?: {
    votes_received: number;
    threshold: number;
    approved: boolean;
  };
}

export const TransactionObserver: React.FC = () => {
  const [transactions, setTransactions] = useState<Transaction[]>([]);
  const { lastMessage } = useWebSocket('ws://localhost:8080/ws/transactions');

  useEffect(() => {
    // Fetch initial transaction list
    fetch('/api/v1/transactions')
      .then(res => res.json())
      .then(data => setTransactions(data));
  }, []);

  useEffect(() => {
    if (lastMessage !== null) {
      const event = JSON.parse(lastMessage.data);

      // Update transaction in list
      setTransactions(prev =>
        prev.map(tx =>
          tx.txid === event.txid
            ? { ...tx, state: event.new_state }
            : tx
        )
      );
    }
  }, [lastMessage]);

  return (
    <div className="transaction-observer">
      <h2>Transaction Monitor</h2>
      <div className="transaction-list">
        {transactions.map(tx => (
          <TransactionCard key={tx.txid} transaction={tx} />
        ))}
      </div>
    </div>
  );
};

const TransactionCard: React.FC<{ transaction: Transaction }> = ({ transaction }) => {
  const getStateIcon = (state: string) => {
    switch (state) {
      case 'confirmed': return '✅';
      case 'broadcasting': return '⏳';
      case 'signing': return '🔐';
      case 'voting': return '🗳️';
      default: return '📝';
    }
  };

  return (
    <div className="transaction-card">
      <div className="tx-header">
        <span className="state-icon">{getStateIcon(transaction.state)}</span>
        <span className="txid">{transaction.txid.substring(0, 16)}...</span>
        <span className="state">{transaction.state}</span>
      </div>

      <div className="tx-details">
        <div>Amount: {transaction.amount_sats} sats</div>
        <div>Recipient: {transaction.recipient.substring(0, 20)}...</div>

        {transaction.voting_progress && (
          <div className="voting-progress">
            Votes: {transaction.voting_progress.votes_received}/{transaction.voting_progress.threshold}
          </div>
        )}

        {transaction.chain_status && (
          <div className="chain-status">
            <div>Confirmations: {transaction.chain_status.confirmations}/6</div>
            <div className="progress-bar">
              <div
                className="progress-fill"
                style={{ width: `${(transaction.chain_status.confirmations / 6) * 100}%` }}
              />
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
```

### Test Plan

```bash
# Test 1: Create transaction and monitor via WebSocket
# Terminal 1: WebSocket listener
wscat -c ws://localhost:8080/ws/transactions

# Terminal 2: Create transaction
curl -X POST http://localhost:8080/api/v1/transactions \
  -H "Authorization: Bearer ..." \
  -d '{
    "recipient": "tb1q...",
    "amount_sats": 50000
  }'

# Expected in Terminal 1:
{"txid":"abc123","event_type":"transaction_created","new_state":"pending",...}
{"txid":"abc123","event_type":"voting_initiated","new_state":"voting",...}
{"txid":"abc123","event_type":"voting_approved","new_state":"approved",...}
{"txid":"abc123","event_type":"transaction_signed","new_state":"signed",...}
{"txid":"abc123","event_type":"transaction_broadcast","new_state":"broadcasting",...}
{"txid":"abc123","event_type":"transaction_confirmed","new_state":"confirmed",...}

# Test 2: List user transactions
curl http://localhost:8080/api/v1/users/alice/transactions

# Test 3: Get transaction detail
curl http://localhost:8080/api/v1/transactions/abc123
```

---

**(Continuing in next message due to length limit...)**