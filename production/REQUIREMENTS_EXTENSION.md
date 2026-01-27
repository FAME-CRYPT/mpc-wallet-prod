# 📋 EXTENDED DEMO REQUIREMENTS (Continued from FINAL_DEMO_PLAN.md)

**Date:** 2026-01-27
**This file contains requirements #5-#9 for the 1-week demo**

---

## 🔴 REQUIREMENT #5: Log Screen

*(Architecture and implementation already added to FINAL_DEMO_PLAN.md)*

### Summary
- Real-time log aggregation from all 5 nodes via Docker logs
- WebSocket streaming to frontend
- Filterable by node, severity level, search keywords
- Color-coded display (ERROR=red, WARN=orange, INFO=blue, DEBUG=gray)
- Export logs functionality
- Auto-scroll with pause option

### Key Files
- `crates/common/src/logging.rs` - Structured JSON logging setup
- `crates/logs/src/aggregator.rs` - Log collection from Docker containers
- `crates/api/src/handlers/ws.rs` - WebSocket /ws/logs endpoint
- `frontend/src/components/LogViewer.tsx` - React log viewer UI

---

## 🔴 REQUIREMENT #6: Multi-User Demo with UI

### Summary
Complete user-facing application for demo day.

### Core Features

#### 1. User Management
```sql
CREATE TABLE users (
    user_id VARCHAR(64) PRIMARY KEY,
    username VARCHAR(100) UNIQUE NOT NULL,
    email VARCHAR(255) UNIQUE,
    password_hash TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT NOW(),
    last_login_at TIMESTAMP
);

-- Mock users for demo
INSERT INTO users VALUES
('user_alice', 'alice', 'alice@example.com', '$2b$10$...'),  -- password: demo123
('user_bob', 'bob', 'bob@example.com', '$2b$10$...'),
('user_charlie', 'charlie', 'charlie@example.com', '$2b$10$...'),
('user_diana', 'diana', 'diana@example.com', '$2b$10$...');
```

#### 2. Authentication Service
**File:** `crates/auth/src/service.rs`

```rust
pub struct AuthService {
    jwt_secret: String,
    postgres: Arc<PostgresStorage>,
}

impl AuthService {
    /// Register new user
    pub async fn register(&self, username: &str, email: &str, password: &str) -> Result<String>;

    /// Login and return JWT
    pub async fn login(&self, username: &str, password: &str) -> Result<(String, User)>;

    /// Verify JWT token
    pub fn verify_jwt(&self, token: &str) -> Result<Claims>;
}
```

#### 3. Frontend Application Structure

```
frontend/
├── src/
│   ├── pages/
│   │   ├── Login.tsx           # Login/register page with quick user buttons
│   │   ├── Dashboard.tsx       # User dashboard (balance, send, history)
│   │   └── TransactionDetail.tsx  # Detailed transaction view
│   ├── components/
│   │   ├── TransactionObserver.tsx  # Real-time transaction list
│   │   ├── LogViewer.tsx       # Log streaming component
│   │   ├── WalletInfo.tsx      # Wallet address and balance display
│   │   └── SendForm.tsx        # Send transaction form
│   ├── hooks/
│   │   ├── useAuth.tsx         # JWT auth hook
│   │   └── useWebSocket.tsx    # WebSocket connection hook
│   └── App.tsx                 # Main app with routing
```

#### 4. Demo Setup Script

**File:** `scripts/setup_demo_users.sh`

```bash
#!/bin/bash
# One-click demo setup

# 1. Register 4 users (alice, bob, charlie, diana)
# 2. Login each user to get JWT
# 3. Create wallet for each user
# 4. Fund Alice's wallet with testnet BTC

for user in alice bob charlie diana; do
  # Register
  curl -X POST "$API/auth/register" -d '{"username":"'$user'","password":"demo123"}'

  # Get JWT
  TOKEN=$(curl -X POST "$API/auth/login" -d '{"username":"'$user'","password":"demo123"}' | jq -r '.token')

  # Create wallet
  curl -X POST "$API/wallets" -H "Authorization: Bearer $TOKEN" -d '{"user_id":"'$user'"}'
done

echo "✅ Demo users ready! Login with username: alice/bob/charlie/diana, password: demo123"
```

#### 5. UI Screenshots

**Login Page:**
```
┌─────────────────────────────────────────┐
│       MPC Wallet Demo                   │
│                                          │
│   Username: [____________]              │
│   Password: [____________]              │
│                                          │
│   [Login]  [Register]                   │
│                                          │
│   Quick Login (Demo):                   │
│   [👤 Alice] [👤 Bob]                   │
│   [👤 Charlie] [👤 Diana]               │
└─────────────────────────────────────────┘
```

**Dashboard:**
```
┌─────────────────────────────────────────┐
│  👤 Alice                    [Logout]   │
│  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │
│  💰 Balance: 0.05000000 BTC            │
│  📍 Wallet: tb1q...                     │
│                                          │
│  Send Transaction:                      │
│  To: [Select User ▼] Bob                │
│  Amount: [0.001] BTC  [Send]            │
│                                          │
│  Transaction History:                   │
│  ✅ Sent 0.001 BTC to Bob (Confirmed)  │
│  ⏳ Sent 0.002 BTC to Charlie (Signing) │
│  ✅ Received 0.005 BTC from Diana      │
└─────────────────────────────────────────┘
```

### Test Scenario

```bash
# Demo Day Flow:

# 1. Show login page
# 2. Click "Alice" quick login
# 3. Dashboard appears with balance
# 4. Select "Bob" as recipient
# 5. Enter amount: 0.001 BTC
# 6. Click Send
# 7. Transaction appears with state "pending"
# 8. Watch real-time state changes:
#    pending → voting → approved → signing → signed → broadcasting → confirmed
# 9. Open ChainMonitor to show blockchain status
# 10. Open Log Screen to show MPC protocol execution
# 11. Switch to Bob's account - show incoming transaction

# Total demo time: < 2 minutes for one full flow
```

---

## 🔴 REQUIREMENT #7: Admin Panel

### Problem & Requirements

**Current State:**
- No administrative interface
- Cannot monitor system health
- Cannot configure policies
- No centralized control

**Required Functionality:**
1. System health dashboard (node status, performance metrics)
2. Policy engine configuration (whitelists, blacklists, limits)
3. User management (view users, wallets, balances)
4. Transaction monitoring (all transactions, not just one user)
5. Log viewer (integrated from #5)
6. Manual intervention controls (approve/reject transactions)

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  Admin Panel Architecture                                        │
│                                                                   │
│  Admin UI (React - Port 3001)                                    │
│  ┌────────────────────────────────────────────────────────┐     │
│  │  Navigation:                                            │     │
│  │  [Dashboard] [Users] [Transactions] [Policies] [Logs]  │     │
│  └────────────────────────────────────────────────────────┘     │
│       │                                                           │
│       ▼                                                           │
│  ┌────────────────────────────────────────────────────────┐     │
│  │  Dashboard Tab:                                         │     │
│  │  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │     │
│  │  System Status:                                         │     │
│  │  ✅ Node-1: Healthy (CPU: 45%, RAM: 2.1GB)            │     │
│  │  ✅ Node-2: Healthy (CPU: 42%, RAM: 2.0GB)            │     │
│  │  ✅ Node-3: Healthy (CPU: 48%, RAM: 2.2GB)            │     │
│  │  ✅ Node-4: Healthy (CPU: 40%, RAM: 1.9GB)            │     │
│  │  ✅ Node-5: Healthy (CPU: 44%, RAM: 2.1GB)            │     │
│  │                                                          │     │
│  │  Transaction Stats (Last 24h):                          │     │
│  │  Total: 127 | Pending: 3 | Completed: 120 | Failed: 4  │     │
│  │                                                          │     │
│  │  MPC Signing Performance:                               │     │
│  │  Avg Time: 1.2s | P95: 2.1s | P99: 3.5s               │     │
│  └────────────────────────────────────────────────────────┘     │
│       │                                                           │
│       ▼                                                           │
│  ┌────────────────────────────────────────────────────────┐     │
│  │  Policies Tab:                                          │     │
│  │  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │     │
│  │  Global Policies:                                       │     │
│  │  Max Amount: [1.0] BTC                                 │     │
│  │  Voting Timeout: [60] seconds                          │     │
│  │  Min Threshold: [4] of 5 nodes                         │     │
│  │  [Save]                                                 │     │
│  │                                                          │     │
│  │  Whitelist:                                             │     │
│  │  tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx ✓         │     │
│  │  [+ Add Address]                                        │     │
│  │                                                          │     │
│  │  Blacklist:                                             │     │
│  │  tb1q9vza2e8x573nczrlzms0wvx3gsqjx7vavgkx0l ✗         │     │
│  │  [+ Add Address]                                        │     │
│  │                                                          │     │
│  │  Per-User Policies:                                     │     │
│  │  Alice: Max 0.1 BTC/tx, 1.0 BTC/day                   │     │
│  │  Bob: Max 0.5 BTC/tx, 5.0 BTC/day                     │     │
│  │  [Edit] [Add User Policy]                              │     │
│  └────────────────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────────────────┘
```

### Database Schema

```sql
-- Admin users (separate from regular users)
CREATE TABLE admin_users (
    admin_id VARCHAR(64) PRIMARY KEY,
    username VARCHAR(100) UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    role VARCHAR(50) DEFAULT 'admin',  -- admin, superadmin
    created_at TIMESTAMP DEFAULT NOW()
);

-- Policy configurations
CREATE TABLE policies (
    policy_id SERIAL PRIMARY KEY,
    policy_type VARCHAR(50) NOT NULL,  -- global, user, address
    target VARCHAR(255),  -- user_id or address (NULL for global)
    rules JSONB NOT NULL,
    enabled BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW()
);

-- Example policy rules JSONB format:
{
  "max_amount_sats": 100000000,  -- 1 BTC
  "max_daily_amount_sats": 1000000000,  -- 10 BTC
  "voting_timeout_secs": 60,
  "required_approvals": 4,
  "whitelist": ["tb1q...", "tb1q..."],
  "blacklist": ["tb1q..."],
  "allowed_protocols": ["cggmp24", "frost"]
}
```

### Implementation Steps

#### Step 1: Admin Backend Service

**File:** `crates/admin/src/service.rs` (NEW CRATE)

```rust
//! Admin Panel Backend Service

pub struct AdminService {
    postgres: Arc<PostgresStorage>,
    cluster_health: Arc<ClusterHealthMonitor>,
    policy_engine: Arc<PolicyEngine>,
}

impl AdminService {
    /// Get system health status
    pub async fn get_system_status(&self) -> Result<SystemStatus>;

    /// Get transaction statistics
    pub async fn get_transaction_stats(&self, period: TimePeriod) -> Result<TxStats>;

    /// Get all users
    pub async fn list_all_users(&self) -> Result<Vec<UserInfo>>;

    /// Update policy
    pub async fn update_policy(&self, policy: PolicyUpdate) -> Result<()>;

    /// Get policy by type
    pub async fn get_policy(&self, policy_type: PolicyType) -> Result<Policy>;

    /// Manual transaction intervention
    pub async fn override_transaction_decision(
        &self,
        txid: &str,
        decision: Decision,  // Approve or Reject
        reason: &str,
    ) -> Result<()>;
}

#[derive(Debug, Serialize)]
pub struct SystemStatus {
    pub nodes: Vec<NodeStatus>,
    pub postgres_status: DbStatus,
    pub etcd_status: EtcdStatus,
}

#[derive(Debug, Serialize)]
pub struct NodeStatus {
    pub node_id: u64,
    pub healthy: bool,
    pub cpu_percent: f32,
    pub memory_mb: u64,
    pub uptime_secs: u64,
}

#[derive(Debug, Serialize)]
pub struct TxStats {
    pub total: u64,
    pub pending: u64,
    pub approved: u64,
    pub rejected: u64,
    pub completed: u64,
    pub failed: u64,
    pub avg_signing_time_ms: u64,
    pub p95_signing_time_ms: u64,
    pub p99_signing_time_ms: u64,
}
```

#### Step 2: Admin API Endpoints

**File:** `crates/admin/src/api.rs`

```rust
/// GET /admin/api/status
pub async fn get_status(State(state): State<AdminState>) -> Result<Json<SystemStatus>>;

/// GET /admin/api/stats?period=24h
pub async fn get_stats(State(state): State<AdminState>, Query(params): Query<StatsQuery>) -> Result<Json<TxStats>>;

/// GET /admin/api/users
pub async fn list_users(State(state): State<AdminState>) -> Result<Json<Vec<UserInfo>>>;

/// GET /admin/api/policies/:type
pub async fn get_policy(State(state): State<AdminState>, Path(policy_type): Path<String>) -> Result<Json<Policy>>;

/// POST /admin/api/policies
pub async fn update_policy(State(state): State<AdminState>, Json(policy): Json<PolicyUpdate>) -> Result<Json<&'static str>>;

/// POST /admin/api/transactions/:txid/override
pub async fn override_transaction(
    State(state): State<AdminState>,
    Path(txid): Path<String>,
    Json(req): Json<OverrideRequest>,
) -> Result<Json<&'static str>>;
```

### Test Plan

```bash
# Test 1: Admin login
curl -X POST http://localhost:3001/admin/auth/login \
  -d '{"username":"admin","password":"admin123"}'

# Test 2: Get system status
curl http://localhost:3001/admin/api/status \
  -H "Authorization: Bearer $ADMIN_TOKEN"

# Expected:
{
  "nodes": [
    {"node_id": 1, "healthy": true, "cpu_percent": 45.2, "memory_mb": 2100},
    ...
  ]
}

# Test 3: Update policy
curl -X POST http://localhost:3001/admin/api/policies \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -d '{
    "policy_type": "global",
    "rules": {
      "max_amount_sats": 50000000,
      "voting_timeout_secs": 90
    }
  }'

# Test 4: Override transaction decision
curl -X POST http://localhost:3001/admin/api/transactions/tx_abc123/override \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -d '{"decision": "approve", "reason": "Manual approval for high-value customer"}'
```

---

## 🔴 REQUIREMENT #8: Policy Engine

### Problem & Requirements

**Current State:**
- Auto-approval for all transactions (no policy enforcement)
- No user-specific limits
- No address whitelisting/blacklisting
- No timeout enforcement

**Required Functionality:**
1. Global policies (max amount, voting timeout, threshold)
2. Per-user policies (daily limits, per-transaction limits)
3. Address-based policies (whitelist, blacklist)
4. Automatic policy evaluation during voting
5. Timeout enforcement (reject if not approved within 1 minute)
6. Policy audit logging

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  Policy Engine Flow                                              │
│                                                                   │
│  1. Transaction Created                                          │
│     │                                                             │
│     ▼                                                             │
│  2. PolicyEngine::evaluate(tx)                                   │
│     │                                                             │
│     ├─► Check Global Policies                                    │
│     │   ├─ Max amount? (1 BTC)                                   │
│     │   ├─ Voting timeout? (60s)                                 │
│     │   └─ Threshold? (4/5)                                      │
│     │                                                             │
│     ├─► Check User Policies                                      │
│     │   ├─ User daily limit? (Alice: 1 BTC/day)                  │
│     │   ├─ User tx limit? (Alice: 0.1 BTC/tx)                    │
│     │   └─ User allowed protocols? (CGGMP24 only)                │
│     │                                                             │
│     ├─► Check Address Policies                                   │
│     │   ├─ Recipient on whitelist? ✓                             │
│     │   ├─ Recipient on blacklist? ✗                             │
│     │   └─ Address-to-address rules? (Alice→Bob allowed)         │
│     │                                                             │
│     ▼                                                             │
│  3. PolicyDecision                                               │
│     ├─ APPROVED → Proceed to voting                              │
│     ├─ REJECTED → Update state to "rejected"                     │
│     └─ REQUIRES_MANUAL_REVIEW → Notify admin                     │
│                                                                   │
│  4. During Voting: Timeout Monitor                               │
│     │                                                             │
│     ├─ Start timer: 60 seconds                                   │
│     │                                                             │
│     ├─ If timeout expires before threshold reached:              │
│     │   └─► Auto-reject transaction                              │
│     │                                                             │
│     └─ If threshold reached:                                     │
│         └─► Proceed to signing                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Database Schema

```sql
-- Policy violations log
CREATE TABLE policy_violations (
    id SERIAL PRIMARY KEY,
    tx_id VARCHAR(64) NOT NULL,
    user_id VARCHAR(64),
    policy_type VARCHAR(50) NOT NULL,
    violation_reason TEXT NOT NULL,
    timestamp TIMESTAMP DEFAULT NOW(),

    INDEX idx_tx_id (tx_id),
    INDEX idx_user_id (user_id),
    INDEX idx_timestamp (timestamp)
);

-- User spending limits tracking
CREATE TABLE user_spending (
    user_id VARCHAR(64) PRIMARY KEY,
    daily_spent_sats BIGINT DEFAULT 0,
    last_reset_date DATE DEFAULT CURRENT_DATE,
    total_spent_sats BIGINT DEFAULT 0
);
```

### Implementation Steps

#### Step 1: Policy Engine Core

**File:** `crates/policy/src/engine.rs` (NEW FILE)

```rust
//! Policy Engine - Transaction Policy Evaluation

use anyhow::Result;
use tracing::{info, warn};

pub struct PolicyEngine {
    postgres: Arc<PostgresStorage>,
}

impl PolicyEngine {
    /// Evaluate transaction against all policies
    pub async fn evaluate(&self, tx: &Transaction) -> Result<PolicyDecision> {
        info!("Evaluating policies for transaction: {}", tx.txid);

        // 1. Global policies
        if let Err(reason) = self.check_global_policies(tx).await {
            return Ok(PolicyDecision::Rejected(reason));
        }

        // 2. User policies
        if let Some(user_id) = &tx.sender_wallet_id {
            if let Err(reason) = self.check_user_policies(user_id, tx).await {
                return Ok(PolicyDecision::Rejected(reason));
            }
        }

        // 3. Address policies
        if let Err(reason) = self.check_address_policies(tx).await {
            return Ok(PolicyDecision::Rejected(reason));
        }

        // 4. Check if manual review required
        if self.requires_manual_review(tx).await {
            return Ok(PolicyDecision::RequiresManualReview);
        }

        Ok(PolicyDecision::Approved)
    }

    /// Check global policies
    async fn check_global_policies(&self, tx: &Transaction) -> Result<(), String> {
        let global_policy = self.postgres.get_global_policy().await
            .map_err(|e| format!("Failed to load global policy: {}", e))?;

        // Check max amount
        if tx.amount_sats > global_policy.max_amount_sats {
            return Err(format!(
                "Amount {} exceeds global limit {}",
                tx.amount_sats, global_policy.max_amount_sats
            ));
        }

        Ok(())
    }

    /// Check user-specific policies
    async fn check_user_policies(&self, user_id: &str, tx: &Transaction) -> Result<(), String> {
        // Load user policy
        let user_policy = self.postgres.get_user_policy(user_id).await
            .map_err(|e| format!("Failed to load user policy: {}", e))?;

        // Check per-transaction limit
        if tx.amount_sats > user_policy.max_tx_amount_sats {
            return Err(format!(
                "Transaction amount {} exceeds user limit {}",
                tx.amount_sats, user_policy.max_tx_amount_sats
            ));
        }

        // Check daily limit
        let daily_spent = self.postgres.get_user_daily_spending(user_id).await
            .map_err(|e| format!("Failed to get user spending: {}", e))?;

        if daily_spent + tx.amount_sats > user_policy.max_daily_amount_sats {
            return Err(format!(
                "Daily limit exceeded: {} + {} > {}",
                daily_spent, tx.amount_sats, user_policy.max_daily_amount_sats
            ));
        }

        Ok(())
    }

    /// Check address-based policies (whitelist/blacklist)
    async fn check_address_policies(&self, tx: &Transaction) -> Result<(), String> {
        // Check blacklist
        if self.postgres.is_address_blacklisted(&tx.recipient).await? {
            return Err(format!("Recipient address {} is blacklisted", tx.recipient));
        }

        // Check whitelist (if enabled)
        let global_policy = self.postgres.get_global_policy().await?;
        if global_policy.whitelist_enabled {
            if !self.postgres.is_address_whitelisted(&tx.recipient).await? {
                return Err(format!("Recipient address {} not on whitelist", tx.recipient));
            }
        }

        // Check address-to-address rules
        if let Some(sender_addr) = &tx.sender_address {
            if !self.check_address_to_address_allowed(sender_addr, &tx.recipient).await? {
                return Err(format!(
                    "Transfer from {} to {} not allowed by policy",
                    sender_addr, tx.recipient
                ));
            }
        }

        Ok(())
    }

    /// Check if transaction requires manual admin review
    async fn requires_manual_review(&self, tx: &Transaction) -> bool {
        // High-value transactions require manual review
        if tx.amount_sats > 50_000_000 {  // 0.5 BTC
            return true;
        }

        // First-time recipients require review
        if !self.postgres.is_known_recipient(&tx.recipient).await.unwrap_or(false) {
            return true;
        }

        false
    }
}

#[derive(Debug)]
pub enum PolicyDecision {
    Approved,
    Rejected(String),
    RequiresManualReview,
}
```

#### Step 2: Timeout Monitor

**File:** `crates/policy/src/timeout.rs` (NEW FILE)

```rust
//! Voting Timeout Monitor

pub struct VotingTimeoutMonitor {
    postgres: Arc<PostgresStorage>,
}

impl VotingTimeoutMonitor {
    /// Start monitoring a voting round for timeout
    pub async fn monitor_voting_round(&self, tx_id: String, timeout_secs: u64) {
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(timeout_secs)).await;

            // Check if voting completed
            match self.postgres.get_voting_round(&tx_id).await {
                Ok(round) if round.completed => {
                    // Already completed, nothing to do
                    return;
                }
                Ok(_) => {
                    // Timeout expired, reject transaction
                    warn!("Voting timeout for transaction: {}", tx_id);

                    if let Err(e) = self.postgres.update_transaction_state(&tx_id, "rejected").await {
                        error!("Failed to reject timed-out transaction: {}", e);
                    }

                    // Log violation
                    let _ = self.postgres.log_policy_violation(&PolicyViolation {
                        tx_id: tx_id.clone(),
                        user_id: None,
                        policy_type: "voting_timeout".to_string(),
                        violation_reason: format!("Voting not completed within {} seconds", timeout_secs),
                        timestamp: chrono::Utc::now(),
                    }).await;
                }
                Err(e) => {
                    error!("Failed to check voting round: {}", e);
                }
            }
        });
    }
}
```

#### Step 3: Integration into OrchestrationService

**File:** `crates/orchestrator/src/service.rs`

Add policy evaluation before voting:

```rust
// Before initiate_voting()
let policy_decision = self.policy_engine.evaluate(&tx).await?;

match policy_decision {
    PolicyDecision::Approved => {
        // Proceed with voting
        self.initiate_voting(&tx).await?;

        // Start timeout monitor
        self.timeout_monitor.monitor_voting_round(
            tx.txid.clone(),
            self.config.voting_timeout.as_secs(),
        ).await;
    }
    PolicyDecision::Rejected(reason) => {
        warn!("Transaction rejected by policy: {}", reason);
        self.postgres.update_transaction_state(&tx.txid, "rejected").await?;

        // Log violation
        self.postgres.log_policy_violation(&PolicyViolation {
            tx_id: tx.txid.clone(),
            user_id: tx.sender_wallet_id.clone(),
            policy_type: "policy_evaluation".to_string(),
            violation_reason: reason,
            timestamp: chrono::Utc::now(),
        }).await?;
    }
    PolicyDecision::RequiresManualReview => {
        info!("Transaction requires manual admin review: {}", tx.txid);
        self.postgres.update_transaction_state(&tx.txid, "pending_review").await?;

        // Notify admin (could send notification, email, etc.)
        self.notify_admin_review_required(&tx).await?;
    }
}
```

### Test Plan

```bash
# Test 1: Transaction exceeding global limit
curl -X POST http://localhost:8080/api/v1/transactions \
  -H "Authorization: Bearer $ALICE_TOKEN" \
  -d '{
    "recipient": "tb1q...",
    "amount_sats": 150000000
  }'

# Expected: 400 Bad Request
# "Amount exceeds global limit of 100000000 sats"

# Test 2: User daily limit
# Alice's limit: 1 BTC/day
# Send 0.6 BTC
curl -X POST http://localhost:8080/api/v1/transactions \
  -H "Authorization: Bearer $ALICE_TOKEN" \
  -d '{"recipient": "tb1q...", "amount_sats": 60000000}'

# Send another 0.6 BTC (should fail - exceeds daily limit)
curl -X POST http://localhost:8080/api/v1/transactions \
  -H "Authorization: Bearer $ALICE_TOKEN" \
  -d '{"recipient": "tb1q...", "amount_sats": 60000000}'

# Expected: 400 Bad Request
# "Daily limit exceeded"

# Test 3: Blacklisted address
curl -X POST http://localhost:8080/api/v1/transactions \
  -H "Authorization: Bearer $ALICE_TOKEN" \
  -d '{
    "recipient": "tb1qBLACKLISTED_ADDRESS",
    "amount_sats": 10000
  }'

# Expected: 400 Bad Request
# "Recipient address is blacklisted"

# Test 4: Voting timeout
# Create transaction
TXID=$(curl -X POST http://localhost:8080/api/v1/transactions -H "Authorization: Bearer $ALICE_TOKEN" -d '{"recipient":"tb1q...","amount_sats":50000}' | jq -r '.txid')

# Wait 65 seconds (timeout is 60s)
sleep 65

# Check transaction state
curl http://localhost:8080/api/v1/transactions/$TXID

# Expected: state = "rejected", reason = "Voting timeout"
```

---

## 🔴 REQUIREMENT #9: ChainMonitor Component

### Problem & Requirements

**Current State:**
- No blockchain monitoring
- Cannot track transaction confirmations
- No balance updates
- No mempool status

**Required Functionality:**
1. Monitor Bitcoin blockchain for transactions
2. Track confirmation counts (0, 1, 3, 6+)
3. Update wallet balances from blockchain
4. Detect incoming transactions to wallet addresses
5. Monitor mempool for pending transactions
6. Alert on stuck transactions (low fee)

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  ChainMonitor Component Architecture                             │
│                                                                   │
│  ChainMonitor Service                                            │
│  ┌────────────────────────────────────────────────────────┐     │
│  │  Background Tasks (run every 30s):                     │     │
│  │                                                          │     │
│  │  1. Monitor Broadcasting Transactions                   │     │
│  │     ├─ Query Esplora API for tx status                 │     │
│  │     ├─ Update confirmation count                        │     │
│  │     └─ Mark as confirmed (6+ confirmations)            │     │
│  │                                                          │     │
│  │  2. Scan Wallet Addresses                               │     │
│  │     ├─ Query Esplora for each wallet address           │     │
│  │     ├─ Detect new incoming transactions                │     │
│  │     └─ Update wallet balances                          │     │
│  │                                                          │     │
│  │  3. Check Mempool Status                                │     │
│  │     ├─ Get current recommended fees                    │     │
│  │     ├─ Detect stuck transactions (low fee)             │     │
│  │     └─ Alert admin if RBF needed                       │     │
│  └────────────────────────────────────────────────────────┘     │
│       │                                                           │
│       │ HTTP(S)                                                  │
│       │                                                           │
│       ▼                                                           │
│  ┌────────────────────────────────────────────────────────┐     │
│  │  Esplora API (Bitcoin Blockchain Explorer)             │     │
│  │  https://blockstream.info/testnet/api                  │     │
│  │                                                          │     │
│  │  Endpoints:                                             │     │
│  │  GET /tx/:txid                - Transaction details    │     │
│  │  GET /tx/:txid/status         - Confirmation status    │     │
│  │  GET /address/:addr/txs       - Address transactions   │     │
│  │  GET /address/:addr/utxo      - Unspent outputs        │     │
│  │  GET /mempool                 - Mempool stats          │     │
│  │  GET /fee-estimates           - Fee recommendations    │     │
│  └────────────────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────────────────┘
```

### Implementation Steps

#### Step 1: ChainMonitor Service

**File:** `crates/chain/src/monitor.rs` (NEW FILE)

```rust
//! Blockchain Monitoring Service

use std::sync::Arc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn, error};

pub struct ChainMonitor {
    esplora_url: String,
    client: Client,
    postgres: Arc<PostgresStorage>,
}

impl ChainMonitor {
    pub fn new(esplora_url: String, postgres: Arc<PostgresStorage>) -> Self {
        Self {
            esplora_url,
            client: Client::new(),
            postgres,
        }
    }

    /// Start background monitoring tasks
    pub async fn start(self: Arc<Self>) {
        let monitor = Arc::clone(&self);
        tokio::spawn(async move {
            loop {
                if let Err(e) = monitor.run_monitoring_cycle().await {
                    error!("ChainMonitor error: {}", e);
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
            }
        });
    }

    async fn run_monitoring_cycle(&self) -> anyhow::Result<()> {
        // 1. Update transaction confirmations
        self.update_transaction_confirmations().await?;

        // 2. Scan wallet addresses for new transactions
        self.scan_wallet_addresses().await?;

        // 3. Check mempool and detect stuck transactions
        self.check_mempool_status().await?;

        Ok(())
    }

    /// Get transaction status from blockchain
    pub async fn get_transaction_status(&self, bitcoin_txid: &str) -> anyhow::Result<TxStatus> {
        let url = format!("{}/tx/{}/status", self.esplora_url, bitcoin_txid);
        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Transaction not found on blockchain"));
        }

        let status: EsploraTxStatus = response.json().await?;

        Ok(TxStatus {
            confirmations: if status.confirmed {
                status.confirmations.unwrap_or(0)
            } else {
                0
            },
            block_height: status.block_height,
            in_mempool: !status.confirmed,
            fee_sats: 0,  // TODO: Calculate from tx details
        })
    }

    /// Update confirmations for all broadcasting transactions
    async fn update_transaction_confirmations(&self) -> anyhow::Result<()> {
        let txs = self.postgres.get_transactions_by_state("broadcasting").await?;

        for tx in txs {
            if let Some(bitcoin_txid) = &tx.bitcoin_txid {
                match self.get_transaction_status(bitcoin_txid).await {
                    Ok(status) => {
                        // Update confirmation count
                        self.postgres.update_transaction_confirmations(&tx.txid, status.confirmations).await?;

                        // Mark as confirmed if 6+ confirmations
                        if status.confirmations >= 6 {
                            info!("Transaction {} fully confirmed", tx.txid);
                            self.postgres.update_transaction_state(&tx.txid, "confirmed").await?;
                        }
                    }
                    Err(e) => {
                        warn!("Failed to get status for {}: {}", bitcoin_txid, e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Scan all wallet addresses for incoming transactions
    async fn scan_wallet_addresses(&self) -> anyhow::Result<()> {
        let wallets = self.postgres.get_all_wallets().await?;

        for wallet in wallets {
            // Get address transactions from Esplora
            let url = format!("{}/address/{}/txs", self.esplora_url, wallet.address_p2wpkh.as_ref().unwrap_or(&"".to_string()));
            let response = self.client.get(&url).send().await?;

            if !response.status().is_success() {
                continue;
            }

            let blockchain_txs: Vec<EsploraTx> = response.json().await?;

            // Calculate balance from UTXOs
            let balance = self.calculate_address_balance(&wallet.address_p2wpkh.unwrap()).await?;

            // Update wallet balance if changed
            if balance != wallet.balance_sats {
                info!("Updating balance for wallet {}: {} sats", wallet.user_id, balance);
                self.postgres.update_wallet_balance(&wallet.wallet_id, balance).await?;
            }

            // Check for new incoming transactions
            for blockchain_tx in blockchain_txs {
                // Check if this is a new incoming transaction we didn't create
                let is_ours = self.postgres.transaction_exists_by_bitcoin_txid(&blockchain_tx.txid).await?;

                if !is_ours {
                    info!("Detected incoming transaction to {}: {}", wallet.user_id, blockchain_tx.txid);
                    // TODO: Record incoming transaction in database
                }
            }
        }

        Ok(())
    }

    /// Calculate address balance from UTXOs
    async fn calculate_address_balance(&self, address: &str) -> anyhow::Result<i64> {
        let url = format!("{}/address/{}/utxo", self.esplora_url, address);
        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Ok(0);
        }

        let utxos: Vec<EsploraUtxo> = response.json().await?;

        let total: i64 = utxos.iter().map(|utxo| utxo.value as i64).sum();
        Ok(total)
    }

    /// Check mempool and detect stuck transactions
    async fn check_mempool_status(&self) -> anyhow::Result<()> {
        // Get current fee estimates
        let url = format!("{}/fee-estimates", self.esplora_url);
        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Ok(());
        }

        let fee_estimates: FeeEstimates = response.json().await?;
        let current_fast_fee = fee_estimates.get("1").copied().unwrap_or(10.0);

        // Check if any of our transactions are stuck in mempool with low fee
        let txs = self.postgres.get_transactions_by_state("broadcasting").await?;

        for tx in txs {
            if let Some(bitcoin_txid) = &tx.bitcoin_txid {
                // Get transaction details
                if let Ok(tx_details) = self.get_transaction_details(bitcoin_txid).await {
                    if tx_details.in_mempool {
                        let fee_rate = tx_details.fee_sats as f64 / tx_details.vsize as f64;

                        // If fee rate is significantly lower than current fast fee
                        if fee_rate < current_fast_fee * 0.5 {
                            warn!(
                                "Transaction {} may be stuck: fee rate {:.1} sat/vB (recommended: {:.1})",
                                bitcoin_txid, fee_rate, current_fast_fee
                            );

                            // TODO: Send alert to admin for potential RBF
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Get full transaction details
    async fn get_transaction_details(&self, bitcoin_txid: &str) -> anyhow::Result<TxDetails> {
        let url = format!("{}/tx/{}", self.esplora_url, bitcoin_txid);
        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Transaction not found"));
        }

        let tx: EsploraTx = response.json().await?;
        let status = self.get_transaction_status(bitcoin_txid).await?;

        Ok(TxDetails {
            txid: bitcoin_txid.to_string(),
            in_mempool: status.in_mempool,
            confirmations: status.confirmations,
            fee_sats: tx.fee,
            vsize: tx.vsize,
            block_height: status.block_height,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TxStatus {
    pub confirmations: u32,
    pub block_height: Option<u64>,
    pub in_mempool: bool,
    pub fee_sats: u64,
}

#[derive(Debug, Serialize)]
pub struct TxDetails {
    pub txid: String,
    pub in_mempool: bool,
    pub confirmations: u32,
    pub fee_sats: u64,
    pub vsize: u32,
    pub block_height: Option<u64>,
}

// Esplora API response types
#[derive(Debug, Deserialize)]
struct EsploraTxStatus {
    confirmed: bool,
    #[serde(default)]
    confirmations: Option<u32>,
    block_height: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct EsploraTx {
    txid: String,
    fee: u64,
    vsize: u32,
}

#[derive(Debug, Deserialize)]
struct EsploraUtxo {
    value: u64,
}

type FeeEstimates = std::collections::HashMap<String, f64>;
```

### Test Plan

```bash
# Test 1: Monitor transaction confirmations
# Create and broadcast a transaction
TXID=$(curl -X POST http://localhost:8080/api/v1/transactions \
  -H "Authorization: Bearer $ALICE_TOKEN" \
  -d '{"recipient":"tb1q...","amount_sats":50000}' | jq -r '.txid')

# Wait for transaction to be broadcast
sleep 10

# Check ChainMonitor logs
docker logs mpc-node-1 2>&1 | grep ChainMonitor

# Expected:
# "Updating confirmations for tx_abc123: 0 confirmations (mempool)"
# (after 10 minutes): "Updating confirmations for tx_abc123: 1 confirmation"
# (after 1 hour): "Transaction tx_abc123 fully confirmed (6 confirmations)"

# Test 2: Balance updates
# Send testnet BTC to Alice's wallet address from external wallet
ALICE_ADDR=$(curl http://localhost:8080/api/v1/wallets/alice -H "Authorization: Bearer $ALICE_TOKEN" | jq -r '.address')

echo "Send testnet BTC to: $ALICE_ADDR"

# Wait for ChainMonitor to detect
sleep 60

# Check balance updated
curl http://localhost:8080/api/v1/wallets/alice -H "Authorization: Bearer $ALICE_TOKEN" | jq '.balance_sats'

# Test 3: Stuck transaction detection
# Create transaction with very low fee (if possible)
# Wait for ChainMonitor alert in logs
docker logs mpc-node-1 2>&1 | grep "may be stuck"
```

---

## 📋 COMPLETE CHECKLIST - All Requirements

### Phase 1: Voting Mechanism (Days 1-2) - From Original FINAL_DEMO_PLAN.md
- [ ] Implement vote request broadcasting
- [ ] Implement AutoVoter for automatic voting
- [ ] Implement transaction broadcasting to Bitcoin

### Phase 2: Wallet Creation (Day 3)
- [ ] Implement child share derivation
- [ ] Create WalletService
- [ ] Add wallet API endpoints
- [ ] Test wallet creation for multiple users

### Phase 3: BackupNet Integration (Day 3)
- [ ] Integrate BackupNet client from Cemil
- [ ] Add backup calls after DKG
- [ ] Add backup calls after wallet creation
- [ ] Test restore after node restart

### Phase 4: API Gateway (Day 4)
- [ ] Implement JWT authentication
- [ ] Implement request signing
- [ ] Add rate limiting
- [ ] Add request routing
- [ ] Test with multiple concurrent requests

### Phase 5: TX Observer & Log Screen (Day 4-5)
- [ ] Implement TxObserverService
- [ ] Add WebSocket endpoints for transactions and logs
- [ ] Implement LogAggregator
- [ ] Create React components (TransactionObserver, LogViewer)
- [ ] Test real-time updates

### Phase 6: Multi-User Demo UI (Day 5-6)
- [ ] Create user management system
- [ ] Implement authentication service
- [ ] Setup mock users (Alice, Bob, Charlie, Diana)
- [ ] Build frontend (Login, Dashboard, TransactionDetail pages)
- [ ] Test full user flow (login → send → monitor)

### Phase 7: Admin Panel (Day 6)
- [ ] Create AdminService
- [ ] Implement admin API endpoints
- [ ] Build admin frontend (Dashboard, Users, Policies, Logs tabs)
- [ ] Test system monitoring and policy configuration

### Phase 8: Policy Engine (Day 6-7)
- [ ] Implement PolicyEngine core
- [ ] Add global, user, and address policies
- [ ] Implement VotingTimeoutMonitor
- [ ] Integrate into OrchestrationService
- [ ] Test policy enforcement and violations

### Phase 9: ChainMonitor (Day 7)
- [ ] Implement ChainMonitor service
- [ ] Add Esplora API integration
- [ ] Implement background monitoring tasks
- [ ] Add balance updates
- [ ] Test confirmation tracking and incoming transactions

### Phase 10: Integration Testing (Day 7)
- [ ] End-to-end test: Complete flow from wallet creation to confirmed transaction
- [ ] Test with all 4 mock users
- [ ] Test policy enforcement
- [ ] Test admin panel functionality
- [ ] Test ChainMonitor accuracy
- [ ] Prepare demo script

---

## 🎬 FINAL DEMO SCRIPT (Complete 1-Week Version)

### Setup (Before Demo Day)

```bash
# 1. Start all services
docker-compose -f docker/docker-compose.yml up -d

# 2. Run DKG (one-time setup)
curl -X POST http://localhost:8081/api/v1/dkg/initiate \
  -d '{"protocol":"cggmp24","threshold":4,"total_parties":5}'

# 3. Setup demo users
./scripts/setup_demo_users.sh

# 4. Fund Alice's wallet with testnet BTC
# (Manual step - send testnet BTC to Alice's address)
```

### Demo Day Flow (5-7 minutes)

**Part 1: Introduction (30 seconds)**
- "Today we'll demonstrate a production-ready MPC wallet system"
- "5 nodes running threshold signatures (4-of-5)"
- "Complete transaction lifecycle from creation to blockchain confirmation"

**Part 2: User Login & Wallet (30 seconds)**
1. Open browser: http://localhost:3000
2. Click "Alice" quick login
3. Show dashboard: balance, wallet address

**Part 3: Send Transaction (1 minute)**
4. Select "Bob" as recipient
5. Enter amount: 0.001 BTC
6. Click "Send"
7. Show transaction appearing with state "pending"

**Part 4: Real-time Monitoring (2 minutes)**
8. Switch to TX Observer tab
9. Watch state progression in real-time:
   - pending → voting (show 5/5 votes being cast)
   - voting → approved (threshold reached)
   - approved → signing (MPC protocol executing)
   - signing → signed (signature generated)
   - signed → broadcasting (sent to Bitcoin network)
   - broadcasting → confirmed (show confirmation count: 1/6, 2/6, ... 6/6)

**Part 5: Log Screen (1 minute)**
10. Switch to Logs tab
11. Show real-time logs from all 5 nodes
12. Filter by transaction ID
13. Show MPC signing protocol execution
14. Show vote casting and consensus

**Part 6: Admin Panel (1 minute)**
15. Open admin panel: http://localhost:3001/admin
16. Login as admin
17. Show system dashboard: all 5 nodes healthy
18. Show transaction statistics
19. Show policy configuration (limits, whitelist, blacklist)

**Part 7: Policy Enforcement (1 minute)**
20. Switch back to Alice's account
21. Try to send 2 BTC (exceeds limit)
22. Show error: "Amount exceeds limit"
23. Try to send to blacklisted address
24. Show error: "Recipient blacklisted"

**Part 8: ChainMonitor (30 seconds)**
25. Show blockchain explorer: https://blockstream.info/testnet/tx/...
26. Show transaction confirmed on actual Bitcoin testnet
27. Switch to Bob's account
28. Show balance updated with incoming transaction

**Conclusion (30 seconds)**
- "Complete MPC wallet demonstrated"
- "Threshold signatures, policy engine, real-time monitoring"
- "Production-ready architecture"
- "Questions?"

---

## ✅ DEFINITION OF DONE - Complete Demo

The demo is complete when ALL of these criteria are met:

### Core Functionality
1. ✅ DKG generates distributed keys across 5 nodes
2. ✅ User wallets can be created with threshold-distributed child shares
3. ✅ Transactions can be created via API
4. ✅ Voting mechanism automatically approves/rejects transactions
5. ✅ MPC signing protocol generates valid Bitcoin signatures
6. ✅ Transactions broadcast to Bitcoin testnet
7. ✅ Transactions confirmed on blockchain (visible on explorer)

### UI & Monitoring
8. ✅ Login system with JWT authentication works
9. ✅ User dashboard shows balance and transaction history
10. ✅ Real-time transaction monitoring via WebSocket
11. ✅ Log screen shows live logs from all nodes
12. ✅ Admin panel displays system health and statistics

### Policy & Security
13. ✅ Policy engine enforces global limits
14. ✅ Policy engine enforces per-user limits
15. ✅ Whitelist/blacklist address filtering works
16. ✅ Voting timeout (1 minute) enforced
17. ✅ Policy violations logged in database

### Blockchain Integration
18. ✅ ChainMonitor tracks transaction confirmations
19. ✅ ChainMonitor updates wallet balances
20. ✅ ChainMonitor detects incoming transactions
21. ✅ ChainMonitor alerts on stuck transactions

### Data Integrity
22. ✅ BackupNet backs up key share metadata
23. ✅ BackupNet backs up wallet metadata
24. ✅ System can restore after node restart
25. ✅ All transactions logged in audit trail

### Demo Requirements
26. ✅ 4 mock users (Alice, Bob, Charlie, Diana) created
27. ✅ Complete flow works: Alice → Bob transaction
28. ✅ Total time from creation to confirmation < 3 minutes
29. ✅ All state transitions visible in UI
30. ✅ Demo script runs without errors

---

**Last Updated:** 2026-01-27
**Status:** 🟢 COMPREHENSIVE PLAN COMPLETE
**Total Estimated Time:** 7 days (1 week)
**Priority:** 🔴 CRITICAL - Demo in 1 week
