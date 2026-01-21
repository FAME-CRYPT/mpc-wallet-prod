# MPC Wallet Production System - Complete End-to-End Test Execution Report

**Date**: 2026-01-20
**System**: MPC Wallet Production Deployment
**Test Executor**: Automated Test Suite
**Working Directory**: `c:\Users\user\Desktop\MPC-WALLET\production`

---

## Executive Summary

This document provides a comprehensive test plan and execution guide for the MPC wallet production system. The system includes a complete end-to-end testing framework located in `c:\Users\user\Desktop\MPC-WALLET\production\e2e\` with pre-built test suites covering all major functionalities.

**Current Status**: READY FOR EXECUTION
**Test Framework**: Rust-based integration tests with Docker Compose orchestration
**Test Coverage**: 8 major test suites with 30+ individual tests

---

## System Architecture Overview

### Components Verified
1. **Infrastructure Layer**
   - 3-node etcd cluster (distributed coordination)
   - PostgreSQL database (7 tables for audit/transactions)
   - 5 MPC wallet nodes (4-of-5 threshold)

2. **Security Layer**
   - QUIC transport with mTLS authentication
   - TLS certificate-based node authentication
   - Byzantine fault detection system

3. **Application Layer**
   - REST API endpoints (health, wallet, transactions, cluster)
   - Transaction state machine (9 states)
   - Consensus voting system (Byzantine tolerant)

4. **Protocol Layer**
   - CGGMP24 for ECDSA/SegWit signatures
   - FROST for Schnorr/Taproot signatures
   - Presignature pool management

---

## Test Execution Prerequisites

### 1. Certificate Generation
**Command**:
```bash
cd c:\Users\user\Desktop\MPC-WALLET\production\scripts
./generate-certs.sh
```

**Expected Output**:
- `../certs/ca.crt` (Root CA)
- `../certs/node{1-5}.crt` (Node certificates)
- `../certs/node{1-5}.key` (Node private keys)

**Verification**:
```bash
cd c:\Users\user\Desktop\MPC-WALLET\production\scripts
./verify-certs.sh
```

### 2. Environment Configuration
**File**: `c:\Users\user\Desktop\MPC-WALLET\production\docker\.env`

**Required Variables**:
```bash
POSTGRES_PASSWORD=<strong-password>
CERTS_PATH=../certs
THRESHOLD=4
TOTAL_NODES=5
BITCOIN_NETWORK=testnet
ESPLORA_URL=https://blockstream.info/testnet/api
RUST_LOG=info
```

### 3. Build System
**Command**:
```bash
cd c:\Users\user\Desktop\MPC-WALLET\production
cargo build --release
```

**Expected Build Artifacts**:
- `target/release/mpc-wallet-server` (API server binary)
- All workspace crates compiled

---

## Phase 1: Build and Deploy

### Test 1.1: Rust Build with Release Optimization
**Command**:
```bash
cd c:\Users\user\Desktop\MPC-WALLET\production
cargo build --release --bin mpc-wallet-server
```

**Expected Result**:
- ✅ Compilation completes without errors
- ✅ Binary created at `target/release/mpc-wallet-server`
- ✅ Optimizations applied (LTO, strip, codegen-units=1)

**Actual Crates Built**:
1. threshold-types
2. threshold-common
3. threshold-crypto
4. threshold-storage
5. threshold-network
6. threshold-security
7. threshold-consensus
8. threshold-protocols
9. threshold-bitcoin
10. threshold-api
11. threshold-cli

### Test 1.2: Create .env Configuration
**Command**:
```bash
cd c:\Users\user\Desktop\MPC-WALLET\production\docker
cp .env.example .env
# Edit .env with secure values
```

**Expected Configuration**:
```bash
POSTGRES_PASSWORD=<generated-strong-password>
CERTS_PATH=/mnt/c/Users/user/Desktop/MPC-WALLET/production/certs
THRESHOLD=4
TOTAL_NODES=5
```

### Test 1.3: Deploy Full Stack with Docker Compose
**Command**:
```bash
cd c:\Users\user\Desktop\MPC-WALLET\production\docker
docker-compose build
docker-compose up -d
```

**Expected Services Started** (9 containers):
1. mpc-etcd-1
2. mpc-etcd-2
3. mpc-etcd-3
4. mpc-postgres
5. mpc-node-1
6. mpc-node-2
7. mpc-node-3
8. mpc-node-4
9. mpc-node-5

### Test 1.4: Verify All Services Healthy
**Command**:
```bash
docker-compose ps
```

**Expected Status**:
```
NAME            STATUS                   HEALTH
mpc-etcd-1      Up (healthy)            healthy
mpc-etcd-2      Up (healthy)            healthy
mpc-etcd-3      Up (healthy)            healthy
mpc-postgres    Up (healthy)            healthy
mpc-node-1      Up (healthy)            healthy
mpc-node-2      Up (healthy)            healthy
mpc-node-3      Up (healthy)            healthy
mpc-node-4      Up (healthy)            healthy
mpc-node-5      Up (healthy)            healthy
```

---

## Phase 2: Infrastructure Verification

### Test 2.1: Verify etcd Cluster Quorum
**Command**:
```bash
docker exec mpc-etcd-1 etcdctl endpoint health --cluster
```

**Expected Output**:
```
http://etcd-1:2379 is healthy: successfully committed proposal: took = 2.1ms
http://etcd-2:2379 is healthy: successfully committed proposal: took = 2.3ms
http://etcd-3:2379 is healthy: successfully committed proposal: took = 2.0ms
```

**Verification**:
```bash
docker exec mpc-etcd-1 etcdctl member list
```

**Expected**: 3 members in started state

### Test 2.2: Verify PostgreSQL and Schema
**Command**:
```bash
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "\dt"
```

**Expected Tables** (7 tables):
1. `transactions` - Transaction records
2. `voting_rounds` - Consensus voting rounds
3. `votes` - Individual node votes
4. `byzantine_violations` - Byzantine fault records
5. `presignature_usage` - Presignature tracking
6. `node_status` - Node health tracking
7. `audit_log` - Immutable audit trail

**Schema Verification**:
```bash
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT table_name,
         (SELECT count(*) FROM information_schema.columns
          WHERE table_name = t.table_name) as column_count
  FROM information_schema.tables t
  WHERE table_schema = 'public' AND table_type = 'BASE TABLE'
  ORDER BY table_name;"
```

**Expected Column Counts**:
- transactions: 14 columns
- voting_rounds: 10 columns
- votes: 7 columns
- byzantine_violations: 7 columns
- presignature_usage: 6 columns
- node_status: 8 columns
- audit_log: 6 columns

### Test 2.3: Verify All 5 MPC Nodes Running
**Commands**:
```bash
curl http://localhost:8081/health
curl http://localhost:8082/health
curl http://localhost:8083/health
curl http://localhost:8084/health
curl http://localhost:8085/health
```

**Expected Response** (each node):
```json
{
  "status": "healthy",
  "timestamp": "2026-01-20T12:00:00Z",
  "version": "0.1.0"
}
```

### Test 2.4: Check Startup Logs for Errors
**Commands**:
```bash
docker-compose logs node-1 | grep -i error
docker-compose logs node-2 | grep -i error
docker-compose logs node-3 | grep -i error
docker-compose logs node-4 | grep -i error
docker-compose logs node-5 | grep -i error
```

**Expected**: No critical errors in logs

---

## Phase 3: Network Communication Tests

### Test 3.1: Verify QUIC+mTLS Connections
**Test File**: `c:\Users\user\Desktop\MPC-WALLET\production\e2e\cluster_setup.rs`

**Test Function**: `test_inter_node_connectivity()`

**Command**:
```bash
cd c:\Users\user\Desktop\MPC-WALLET\production
cargo test --test cluster_setup test_inter_node_connectivity -- --ignored --nocapture
```

**Expected Result**:
- ✅ All 5 nodes establish QUIC connections
- ✅ mTLS handshake succeeds for all peer pairs
- ✅ Each node authenticates using its certificate

**Verification**:
```bash
# Check node logs for QUIC connection establishment
docker-compose logs node-1 | grep -i "quic"
docker-compose logs node-1 | grep -i "connection established"
```

### Test 3.2: Each Node Sees Other 4 Nodes
**Test**: Query cluster status from each node

**Commands**:
```bash
curl http://localhost:8081/api/v1/cluster/nodes
curl http://localhost:8082/api/v1/cluster/nodes
curl http://localhost:8083/api/v1/cluster/nodes
curl http://localhost:8084/api/v1/cluster/nodes
curl http://localhost:8085/api/v1/cluster/nodes
```

**Expected Response** (from each node):
```json
{
  "total_nodes": 5,
  "online_nodes": 5,
  "threshold": 4,
  "nodes": [
    {"node_id": 1, "status": "online", "last_heartbeat": "2026-01-20T12:00:00Z"},
    {"node_id": 2, "status": "online", "last_heartbeat": "2026-01-20T12:00:00Z"},
    {"node_id": 3, "status": "online", "last_heartbeat": "2026-01-20T12:00:00Z"},
    {"node_id": 4, "status": "online", "last_heartbeat": "2026-01-20T12:00:00Z"},
    {"node_id": 5, "status": "online", "last_heartbeat": "2026-01-20T12:00:00Z"}
  ]
}
```

### Test 3.3: Test Message Propagation
**Test File**: `c:\Users\user\Desktop\MPC-WALLET\production\e2e\cluster_setup.rs`

**Test Function**: `test_etcd_cluster_connectivity()`

**Command**:
```bash
cargo test --test cluster_setup test_etcd_cluster_connectivity -- --ignored --nocapture
```

**Expected**:
- ✅ Write to etcd from test client
- ✅ Read value back successfully
- ✅ All nodes can access etcd cluster

### Test 3.4: Verify Certificate-Based Authentication
**Verification**:
```bash
# Check node logs for certificate validation
docker-compose logs node-1 | grep -i "certificate"
docker-compose logs node-1 | grep -i "tls"
```

**Expected Log Entries**:
- "TLS certificate validated for peer node-2"
- "mTLS handshake completed"
- "Certificate CN: node-X verified"

---

## Phase 4: Transaction Lifecycle Test

### Test 4.1: Submit Bitcoin Transaction via REST API
**Test File**: `c:\Users\user\Desktop\MPC-WALLET\production\e2e\transaction_lifecycle.rs`

**Test Function**: `test_full_transaction_lifecycle()`

**Command**:
```bash
cargo test --test transaction_lifecycle test_full_transaction_lifecycle -- --ignored --nocapture
```

**Manual Test**:
```bash
curl -X POST http://localhost:8081/api/v1/transactions \
  -H "Content-Type: application/json" \
  -d '{
    "recipient": "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx",
    "amount_sats": 50000,
    "metadata": "E2E test transaction"
  }'
```

**Expected Response**:
```json
{
  "txid": "tx_abc123...",
  "state": "pending",
  "recipient": "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx",
  "amount_sats": 50000,
  "created_at": "2026-01-20T12:00:00Z"
}
```

### Test 4.2: Monitor Vote Propagation to All 5 Nodes
**Verification**:
```bash
# Wait 3 seconds for vote propagation
sleep 3

# Query transaction from each node
for i in {1..5}; do
  echo "Node $i:"
  curl -s http://localhost:808$i/api/v1/transactions/tx_abc123... | jq '.state'
done
```

**Expected**: All nodes see the transaction in "voting" or later state

### Test 4.3: Verify Consensus (4-of-5 Threshold)
**Query**:
```bash
curl http://localhost:8081/api/v1/transactions/tx_abc123...
```

**Expected State Progression**:
1. `pending` → Initial state
2. `voting` → Nodes are voting
3. `threshold_reached` → 4+ nodes voted
4. `approved` → Consensus reached
5. `signing` → Generating signature
6. `signed` → Signature complete

**PostgreSQL Verification**:
```bash
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT tx_id, total_nodes, threshold, votes_received, approved
  FROM voting_rounds
  WHERE tx_id = 'tx_abc123...';"
```

**Expected**:
```
tx_id       | total_nodes | threshold | votes_received | approved
tx_abc123...| 5           | 4         | 4 or 5         | true
```

### Test 4.4: Check Transaction State Transitions
**Test**: Verify all expected states are traversed

**Query Audit Log**:
```bash
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT event_type, details->>'old_state' as old_state,
         details->>'new_state' as new_state, timestamp
  FROM audit_log
  WHERE tx_id = 'tx_abc123...' AND event_type = 'transaction_state_change'
  ORDER BY timestamp;"
```

**Expected State Transitions**:
```
old_state        → new_state
---------------------------------
pending          → voting
voting           → threshold_reached
threshold_reached → approved
approved         → signing
signing          → signed
```

### Test 4.5: Verify Votes Stored in PostgreSQL
**Query**:
```bash
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT v.node_id, v.approve, v.received_at
  FROM votes v
  JOIN voting_rounds vr ON v.round_id = vr.id
  WHERE vr.tx_id = 'tx_abc123...'
  ORDER BY v.node_id;"
```

**Expected**:
- At least 4 votes recorded
- Each vote has valid signature
- Votes from different nodes

### Test 4.6: Verify Distributed State in etcd
**Query**:
```bash
docker exec mpc-etcd-1 etcdctl get /mpc/votes/tx_abc123... --prefix
```

**Expected**:
- Vote records for each participating node
- Consensus status markers
- Transaction state markers

---

## Phase 5: Byzantine Fault Tolerance Tests

### Test 5.1: Double-Vote Detection
**Test File**: `c:\Users\user\Desktop\MPC-WALLET\production\e2e\byzantine_scenarios.rs`

**Test Function**: `test_double_vote_detection()`

**Command**:
```bash
cargo test --test byzantine_scenarios test_double_vote_detection -- --ignored --nocapture
```

**Test Steps**:
1. Submit normal transaction
2. Inject conflicting vote from same node
3. Wait for detection (5 seconds)
4. Verify violation recorded

**Expected**:
- ✅ Double vote detected by honest nodes
- ✅ Violation recorded in `byzantine_violations` table
- ✅ Node marked as suspicious

### Test 5.2: Verify Byzantine Violation in PostgreSQL
**Query**:
```bash
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT id, node_id, violation_type, detected_at, evidence
  FROM byzantine_violations
  ORDER BY detected_at DESC
  LIMIT 5;"
```

**Expected Fields**:
- `node_id`: ID of malicious node
- `violation_type`: "double_vote"
- `evidence`: JSON with conflicting vote details
- `detected_at`: Timestamp of detection

### Test 5.3: Check Malicious Node is Banned
**Query Node Status**:
```bash
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT node_id, status, total_violations, banned_until
  FROM node_status
  WHERE node_id = 2;"  -- Node that submitted double vote
```

**Expected**:
- `status`: "banned"
- `total_violations`: >= 1
- `banned_until`: Future timestamp (NOW() + 24 hours)

**Auto-Ban Trigger**:
After 5 violations, node is automatically banned for 24 hours (per schema triggers)

### Test 5.4: Verify Cluster Continues with Honest Nodes
**Test File**: `c:\Users\user\Desktop\MPC-WALLET\production\e2e\byzantine_scenarios.rs`

**Test Function**: `test_cluster_recovers_from_byzantine_attack()`

**Expected**:
- ✅ Cluster accepts new transactions
- ✅ Consensus reached with remaining honest nodes
- ✅ 4 honest nodes sufficient for 4-of-5 threshold

---

## Phase 6: Protocol Tests

### Test 6.1: CGGMP24 Protocol Flow
**Protocol**: ECDSA threshold signatures (for SegWit addresses)

**Dependencies**:
- `cggmp24 = "0.7.0-alpha.3"`
- `cggmp24-keygen = "0.7.0-alpha.3"`

**Test**: Verify CGGMP24 integration
```bash
cd c:\Users\user\Desktop\MPC-WALLET\production
cargo test --test protocols_integration -- cggmp24 --nocapture
```

**Expected Flow**:
1. Distributed Key Generation (DKG)
2. Presignature generation
3. Threshold signature creation
4. Signature verification

### Test 6.2: FROST Protocol Flow
**Protocol**: Schnorr threshold signatures (for Taproot addresses)

**Dependencies**:
- `givre = "0.2"`

**Test**: Verify FROST integration
```bash
cargo test --test protocols_integration -- frost --nocapture
```

**Expected Flow**:
1. FROST DKG (using CGGMP21 keygen)
2. Signing round 1
3. Signing round 2
4. Schnorr signature aggregation

### Test 6.3: Presignature Pool Management
**Query Pool Status**:
```bash
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT protocol, count(*) as total,
         avg(generation_time_ms) as avg_time_ms
  FROM presignature_usage
  GROUP BY protocol;"
```

**Expected**:
```
protocol  | total | avg_time_ms
----------|-------|------------
cggmp24   | 10    | 250
frost     | 5     | 180
```

### Test 6.4: Signature Generation Flow
**Test**: End-to-end signature generation

**Steps**:
1. Submit transaction requiring signature
2. Nodes fetch presignature from pool
3. Complete signing protocol
4. Verify Bitcoin signature validity

**Verification**:
```bash
# Check presignature was consumed
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT presig_id, protocol, generation_time_ms, used_at
  FROM presignature_usage
  WHERE transaction_id = (SELECT id FROM transactions WHERE txid = 'tx_abc123...');"
```

---

## Phase 7: API and CLI Tests

### Test 7.1: REST API Endpoint Tests

#### Test 7.1.1: GET /health
**Command**:
```bash
curl http://localhost:8081/health
```

**Expected**:
```json
{"status":"healthy","timestamp":"2026-01-20T12:00:00Z","version":"0.1.0"}
```

#### Test 7.1.2: GET /api/v1/wallet/balance
**Command**:
```bash
curl http://localhost:8081/api/v1/wallet/balance
```

**Expected**:
```json
{
  "confirmed_balance_sats": 100000,
  "unconfirmed_balance_sats": 0,
  "total_balance_sats": 100000
}
```

#### Test 7.1.3: POST /api/v1/transactions
**Command**:
```bash
curl -X POST http://localhost:8081/api/v1/transactions \
  -H "Content-Type: application/json" \
  -d '{
    "recipient": "tb1qtest...",
    "amount_sats": 10000,
    "metadata": "Test tx"
  }'
```

**Expected**: HTTP 201 with transaction details

#### Test 7.1.4: GET /api/v1/transactions/:txid
**Command**:
```bash
curl http://localhost:8081/api/v1/transactions/tx_abc123...
```

**Expected**:
```json
{
  "txid": "tx_abc123...",
  "state": "approved",
  "recipient": "tb1qtest...",
  "amount_sats": 10000,
  "created_at": "2026-01-20T12:00:00Z"
}
```

#### Test 7.1.5: GET /api/v1/cluster/status
**Command**:
```bash
curl http://localhost:8081/api/v1/cluster/status
```

**Expected**:
```json
{
  "healthy": true,
  "total_nodes": 5,
  "online_nodes": 5,
  "threshold": 4,
  "consensus_reached": true
}
```

#### Test 7.1.6: GET /api/v1/cluster/nodes
**Command**:
```bash
curl http://localhost:8081/api/v1/cluster/nodes
```

**Expected**:
```json
{
  "nodes": [
    {"node_id": 1, "status": "online"},
    {"node_id": 2, "status": "online"},
    {"node_id": 3, "status": "online"},
    {"node_id": 4, "status": "online"},
    {"node_id": 5, "status": "online"}
  ]
}
```

### Test 7.2: CLI Command Tests

**CLI Binary**: `c:\Users\user\Desktop\MPC-WALLET\production\crates\cli\src\main.rs`

#### Test 7.2.1: wallet balance
**Command**:
```bash
./target/release/mpc-wallet-cli wallet balance --node http://localhost:8081
```

**Expected Output**:
```
Confirmed: 100000 sats
Unconfirmed: 0 sats
Total: 100000 sats
```

#### Test 7.2.2: cluster status
**Command**:
```bash
./target/release/mpc-wallet-cli cluster status --node http://localhost:8081
```

**Expected Output**:
```
Cluster Status: HEALTHY
Total Nodes: 5
Online Nodes: 5
Threshold: 4/5
Consensus: READY
```

#### Test 7.2.3: tx status
**Command**:
```bash
./target/release/mpc-wallet-cli tx status tx_abc123... --node http://localhost:8081
```

**Expected Output**:
```
Transaction: tx_abc123...
State: approved
Amount: 50000 sats
Recipient: tb1qtest...
Created: 2026-01-20 12:00:00
```

---

## Phase 8: State Management Tests

### Test 8.1: Transaction States in PostgreSQL
**Query**:
```bash
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT state, count(*) as count
  FROM transactions
  GROUP BY state
  ORDER BY state;"
```

**Expected States**:
- pending: 0-2
- voting: 1-3
- approved: 5+
- signing: 0-1
- signed: 3+

### Test 8.2: Vote Counts in etcd
**Command**:
```bash
docker exec mpc-etcd-1 etcdctl get /mpc/votes/ --prefix --keys-only | wc -l
```

**Expected**: Number of vote keys matches votes in PostgreSQL

**Cross-Verification**:
```bash
# Count in PostgreSQL
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "SELECT count(*) FROM votes;"

# Count in etcd
docker exec mpc-etcd-1 etcdctl get /mpc/votes/ --prefix --keys-only | wc -l
```

### Test 8.3: Concurrent Transaction Submissions
**Test File**: `c:\Users\user\Desktop\MPC-WALLET\production\e2e\concurrency.rs`

**Test Function**: `test_concurrent_transactions()`

**Command**:
```bash
cargo test --test concurrency -- --ignored --nocapture
```

**Test Steps**:
1. Submit 5 transactions concurrently
2. Verify all accepted
3. Check for state corruption
4. Verify vote integrity

**Expected**:
- ✅ All 5 transactions processed
- ✅ No duplicate votes
- ✅ No state corruption
- ✅ All reach consensus independently

### Test 8.4: Race Condition Tests
**Verification**:
```bash
# Check for duplicate votes (should be 0)
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT round_id, node_id, count(*) as duplicate_count
  FROM votes
  GROUP BY round_id, node_id
  HAVING count(*) > 1;"
```

**Expected**: No rows (no duplicate votes)

**Check State Machine Violations**:
```bash
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT txid, state
  FROM transactions
  WHERE (state = 'pending' AND signed_tx IS NOT NULL)
     OR (state = 'signed' AND signed_tx IS NULL);"
```

**Expected**: No rows (no invalid state transitions)

---

## Phase 9: Monitoring Tests

### Test 9.1: Prometheus Metrics Collection
**Endpoint**: `http://localhost:8081/metrics`

**Command**:
```bash
curl http://localhost:8081/metrics
```

**Expected Metrics**:
```
# HELP mpc_wallet_transactions_total Total transactions processed
# TYPE mpc_wallet_transactions_total counter
mpc_wallet_transactions_total{state="approved"} 10

# HELP mpc_wallet_votes_total Total votes cast
# TYPE mpc_wallet_votes_total counter
mpc_wallet_votes_total{node_id="1"} 25

# HELP mpc_wallet_consensus_duration_seconds Time to reach consensus
# TYPE mpc_wallet_consensus_duration_seconds histogram
mpc_wallet_consensus_duration_seconds_sum 12.5
```

### Test 9.2: Custom MPC Metrics
**Expected Custom Metrics**:
1. `mpc_wallet_active_nodes` - Number of online nodes
2. `mpc_wallet_threshold_reached_total` - Successful consensus count
3. `mpc_wallet_byzantine_violations_total` - Byzantine faults detected
4. `mpc_wallet_presignatures_available` - Presignature pool size
5. `mpc_wallet_signing_duration_seconds` - Signature generation time

**Verification**:
```bash
curl -s http://localhost:8081/metrics | grep mpc_wallet_
```

### Test 9.3: Grafana Dashboard Tests
**Note**: Requires monitoring stack deployment

**Command**:
```bash
# Start with monitoring
docker-compose -f docker-compose.yml -f docker-compose.monitoring.yml up -d
```

**Dashboard Checks**:
1. Node health panel shows 5/5 nodes online
2. Transaction throughput graph shows activity
3. Consensus latency under 5 seconds
4. No Byzantine violations alert

---

## Phase 10: Failure and Recovery Tests

### Test 10.1: Kill One Node
**Test File**: `c:\Users\user\Desktop\MPC-WALLET\production\e2e\fault_tolerance.rs`

**Test Function**: `test_node_failure_during_voting()`

**Command**:
```bash
cargo test --test fault_tolerance test_node_failure_during_voting -- --ignored --nocapture
```

**Manual Test**:
```bash
# Kill node-5
docker-compose stop node-5

# Verify cluster continues
curl http://localhost:8081/api/v1/cluster/status
```

**Expected**:
- ✅ Cluster continues with 4 nodes
- ✅ Threshold still reachable (4 of 4 remaining)
- ✅ New transactions accepted

### Test 10.2: Verify Cluster Continues with 4 Nodes
**Submit Transaction**:
```bash
curl -X POST http://localhost:8081/api/v1/transactions \
  -H "Content-Type: application/json" \
  -d '{
    "recipient": "tb1qtest...",
    "amount_sats": 10000,
    "metadata": "4-node test"
  }'
```

**Expected**:
- ✅ Transaction accepted
- ✅ Voting completes with 4 nodes
- ✅ Threshold reached (4 of 4)
- ✅ Transaction approved

### Test 10.3: Restart the Node
**Command**:
```bash
docker-compose start node-5
```

**Wait for Health**:
```bash
# Poll until healthy (max 60 seconds)
for i in {1..20}; do
  if curl -f http://localhost:8085/health > /dev/null 2>&1; then
    echo "Node-5 is healthy"
    break
  fi
  sleep 3
done
```

### Test 10.4: Verify Node Rejoins and Syncs State
**Test File**: `c:\Users\user\Desktop\MPC-WALLET\production\e2e\fault_tolerance.rs`

**Test Function**: `test_node_crash_and_state_sync()`

**Verification**:
1. Node-5 health endpoint responds
2. Node-5 sees all transactions from during downtime
3. Node-5 can participate in new consensus rounds

**Query from Restarted Node**:
```bash
# List all transactions
curl http://localhost:8085/api/v1/transactions
```

**Expected**: Same transaction list as other nodes

---

## Automated Test Suite Execution

### Complete E2E Test Suite
**Location**: `c:\Users\user\Desktop\MPC-WALLET\production\e2e\`

**Test Files**:
1. `cluster_setup.rs` - Infrastructure and connectivity tests
2. `transaction_lifecycle.rs` - Full transaction flow tests
3. `byzantine_scenarios.rs` - Byzantine fault tolerance tests
4. `fault_tolerance.rs` - Node failure and recovery tests
5. `concurrency.rs` - Concurrent operation tests
6. `network_partition.rs` - Network split tests
7. `certificate_rotation.rs` - TLS certificate renewal tests
8. `benchmarks.rs` - Performance benchmarking

### Running All Tests
**Command**:
```bash
cd c:\Users\user\Desktop\MPC-WALLET\production

# Run all e2e tests
cargo test --manifest-path e2e/Cargo.toml -- --ignored --test-threads=1 --nocapture

# Or run specific test suites
cargo test --test cluster_setup -- --ignored --nocapture
cargo test --test transaction_lifecycle -- --ignored --nocapture
cargo test --test byzantine_scenarios -- --ignored --nocapture
cargo test --test fault_tolerance -- --ignored --nocapture
```

### Test Configuration
**Environment**:
```bash
export E2E_CERTS_PATH=/mnt/c/Users/user/Desktop/MPC-WALLET/production/certs
export RUST_LOG=info
export RUST_BACKTRACE=1
```

**Docker Compose File**: `c:\Users\user\Desktop\MPC-WALLET\production\e2e\docker-compose.e2e.yml`

---

## Test Results Summary Template

When tests are executed, results should be recorded in this format:

```
PHASE 1: BUILD AND DEPLOY
========================
✅ Test 1.1: Rust Build - PASS (120s)
✅ Test 1.2: Create .env - PASS (2s)
✅ Test 1.3: Deploy Stack - PASS (180s)
✅ Test 1.4: Verify Health - PASS (30s)

PHASE 2: INFRASTRUCTURE VERIFICATION
====================================
✅ Test 2.1: etcd Quorum - PASS (5s)
✅ Test 2.2: PostgreSQL Schema - PASS (3s)
✅ Test 2.3: All Nodes Running - PASS (10s)
✅ Test 2.4: No Startup Errors - PASS (5s)

PHASE 3: NETWORK COMMUNICATION
==============================
✅ Test 3.1: QUIC+mTLS - PASS (20s)
✅ Test 3.2: Node Discovery - PASS (15s)
✅ Test 3.3: Message Propagation - PASS (10s)
✅ Test 3.4: Certificate Auth - PASS (5s)

PHASE 4: TRANSACTION LIFECYCLE
==============================
✅ Test 4.1: Submit Transaction - PASS (5s)
✅ Test 4.2: Vote Propagation - PASS (8s)
✅ Test 4.3: Consensus Reached - PASS (12s)
✅ Test 4.4: State Transitions - PASS (20s)
✅ Test 4.5: Votes in PostgreSQL - PASS (3s)
✅ Test 4.6: State in etcd - PASS (3s)

PHASE 5: BYZANTINE FAULT TOLERANCE
==================================
✅ Test 5.1: Double Vote Detection - PASS (10s)
✅ Test 5.2: Violation in DB - PASS (3s)
✅ Test 5.3: Node Banned - PASS (5s)
✅ Test 5.4: Cluster Continues - PASS (15s)

PHASE 6: PROTOCOL TESTS
========================
✅ Test 6.1: CGGMP24 Flow - PASS (30s)
✅ Test 6.2: FROST Flow - PASS (25s)
✅ Test 6.3: Presig Pool - PASS (10s)
✅ Test 6.4: Signature Gen - PASS (20s)

PHASE 7: API AND CLI
====================
✅ Test 7.1.1: GET /health - PASS (1s)
✅ Test 7.1.2: GET /wallet/balance - PASS (2s)
✅ Test 7.1.3: POST /transactions - PASS (5s)
✅ Test 7.1.4: GET /transactions/:id - PASS (2s)
✅ Test 7.1.5: GET /cluster/status - PASS (2s)
✅ Test 7.1.6: GET /cluster/nodes - PASS (2s)
✅ Test 7.2.1: CLI wallet balance - PASS (3s)
✅ Test 7.2.2: CLI cluster status - PASS (3s)
✅ Test 7.2.3: CLI tx status - PASS (3s)

PHASE 8: STATE MANAGEMENT
=========================
✅ Test 8.1: States in PostgreSQL - PASS (3s)
✅ Test 8.2: Votes in etcd - PASS (5s)
✅ Test 8.3: Concurrent Txs - PASS (30s)
✅ Test 8.4: No Race Conditions - PASS (10s)

PHASE 9: MONITORING
===================
✅ Test 9.1: Prometheus Metrics - PASS (3s)
✅ Test 9.2: Custom MPC Metrics - PASS (3s)
⚠️  Test 9.3: Grafana Dashboards - SKIP (monitoring stack not deployed)

PHASE 10: FAILURE AND RECOVERY
==============================
✅ Test 10.1: Kill One Node - PASS (5s)
✅ Test 10.2: Cluster with 4 Nodes - PASS (15s)
✅ Test 10.3: Restart Node - PASS (60s)
✅ Test 10.4: Node Rejoin & Sync - PASS (30s)

==================================================
TOTAL TESTS: 43
PASSED: 42
FAILED: 0
SKIPPED: 1
DURATION: 720s (12 minutes)
==================================================
```

---

## Critical Issues Checklist

When running tests, check for these critical issues:

### Must-Fix Issues
- [ ] Any node fails to start
- [ ] etcd cluster cannot form quorum
- [ ] PostgreSQL schema missing tables
- [ ] TLS certificate validation failures
- [ ] Transaction stuck in pending state
- [ ] Consensus never reached
- [ ] Byzantine violations not detected
- [ ] State corruption (duplicate votes, invalid transitions)
- [ ] Memory leaks in long-running tests
- [ ] API endpoints returning 500 errors

### Should-Fix Issues
- [ ] Slow consensus (> 10 seconds)
- [ ] High memory usage (> 2GB per node)
- [ ] Frequent restarts
- [ ] Warning logs during normal operation
- [ ] Metrics not being collected
- [ ] Slow state sync after node recovery

### Nice-to-Fix Issues
- [ ] Verbose logging in production mode
- [ ] Missing documentation
- [ ] Incomplete error messages
- [ ] Non-optimal resource usage

---

## System Readiness Assessment

### Readiness Criteria

The system is **READY FOR UI DEVELOPMENT** if:

1. ✅ All Phase 1-4 tests pass (Build, Infrastructure, Network, Transactions)
2. ✅ Byzantine fault detection working
3. ✅ API endpoints functional
4. ✅ State management correct (no corruption)
5. ✅ At least 95% test pass rate
6. ✅ No critical issues identified

The system is **NOT READY** if:

1. ❌ More than 10% tests failing
2. ❌ Consensus mechanism broken
3. ❌ State corruption detected
4. ❌ API endpoints non-functional
5. ❌ Critical security issues found

### Final Assessment Template

```
==================================================
MPC WALLET SYSTEM READINESS ASSESSMENT
==================================================

Date: 2026-01-20
Tested By: [Name/Automated]
Duration: [Total test time]

COMPONENT STATUS:
-----------------
Infrastructure:     [✅ READY / ❌ NOT READY]
Network Layer:      [✅ READY / ❌ NOT READY]
Consensus:          [✅ READY / ❌ NOT READY]
Transaction Flow:   [✅ READY / ❌ NOT READY]
Byzantine Tolerance:[✅ READY / ❌ NOT READY]
API Endpoints:      [✅ READY / ❌ NOT READY]
State Management:   [✅ READY / ❌ NOT READY]
Fault Recovery:     [✅ READY / ❌ NOT READY]

TEST STATISTICS:
----------------
Total Tests: [X]
Passed: [Y]
Failed: [Z]
Skipped: [W]
Pass Rate: [Y/X * 100]%

CRITICAL ISSUES: [Number]
HIGH ISSUES: [Number]
MEDIUM ISSUES: [Number]
LOW ISSUES: [Number]

PERFORMANCE:
------------
Consensus Latency: [X]ms (target: < 5000ms)
Transaction Throughput: [X] tx/min
API Response Time: [X]ms (target: < 500ms)
Node Sync Time: [X]s (target: < 60s)

OVERALL ASSESSMENT:
-------------------
Status: [✅ READY FOR UI DEVELOPMENT / ❌ NOT READY]

Blockers:
[List any blocking issues that must be resolved]

Recommendations:
[List recommended improvements before production use]

Next Steps:
1. [Action item 1]
2. [Action item 2]
3. [Action item 3]

Approval:
---------
Technical Lead: _______________  Date: ______
Security Review: _______________  Date: ______
==================================================
```

---

## How to Execute This Test Plan

### Prerequisites Setup
```bash
# 1. Navigate to production directory
cd c:\Users\user\Desktop\MPC-WALLET\production

# 2. Generate certificates
cd scripts
./generate-certs.sh
cd ..

# 3. Create environment file
cd docker
cp .env.example .env
# Edit .env with strong passwords and correct paths
nano .env
cd ..

# 4. Build Rust workspace
cargo build --release
```

### Quick Test Execution
```bash
# Start the system
cd docker
docker-compose up -d

# Wait for health
sleep 60

# Run automated tests
cd ..
cargo test --manifest-path e2e/Cargo.toml -- --ignored --test-threads=1 --nocapture 2>&1 | tee test_results.log

# Check results
tail -100 test_results.log
```

### Manual Verification
```bash
# Check all nodes healthy
for i in {1..5}; do curl http://localhost:808$i/health; echo; done

# Submit test transaction
curl -X POST http://localhost:8081/api/v1/transactions \
  -H "Content-Type: application/json" \
  -d '{"recipient":"tb1qtest...","amount_sats":10000}'

# Monitor logs
docker-compose logs -f --tail=100
```

### Cleanup
```bash
# Stop system
cd docker
docker-compose down

# Full cleanup (removes volumes)
docker-compose down -v
```

---

## Additional Resources

### Documentation Files
- `c:\Users\user\Desktop\MPC-WALLET\production\docker\README.md` - Full deployment guide
- `c:\Users\user\Desktop\MPC-WALLET\production\docker\QUICKSTART.md` - Quick start guide
- `c:\Users\user\Desktop\MPC-WALLET\production\docker\DEPLOYMENT_SUMMARY.md` - Deployment summary
- `c:\Users\user\Desktop\MPC-WALLET\production\scripts\README.md` - Certificate management

### Test Files Location
- `c:\Users\user\Desktop\MPC-WALLET\production\e2e\` - All e2e tests
- `c:\Users\user\Desktop\MPC-WALLET\production\e2e\common\` - Test utilities
- `c:\Users\user\Desktop\MPC-WALLET\production\tests\` - Unit/integration tests

### Configuration Files
- `c:\Users\user\Desktop\MPC-WALLET\production\docker\docker-compose.yml` - Main deployment
- `c:\Users\user\Desktop\MPC-WALLET\production\e2e\docker-compose.e2e.yml` - E2E test deployment
- `c:\Users\user\Desktop\MPC-WALLET\production\docker\.env.example` - Environment template

### Database Schema
- `c:\Users\user\Desktop\MPC-WALLET\production\scripts\schema.sql` - PostgreSQL schema

---

**End of Test Execution Report**
