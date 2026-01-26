# MPC WALLET - KAPSAMLI TEST REHBERİ

**Versiyon:** 1.0
**Tarih:** 2026-01-23
**Durum:** Production Testing - Phase 1

---

## İÇİNDEKİLER

1. [Test Stratejisi ve Yaklaşım](#1-test-stratejisi-ve-yaklaşım)
2. [Test Ortamı Hazırlığı](#2-test-ortamı-hazırlığı)
3. [Component-Level Testing (Katman 1)](#3-component-level-testing-katman-1)
4. [Integration Testing (Katman 2)](#4-integration-testing-katman-2)
5. [Protocol Testing (Katman 3)](#5-protocol-testing-katman-3)
6. [Security Testing (Katman 4)](#6-security-testing-katman-4)
7. [Byzantine Fault Tolerance Testing (Katman 5)](#7-byzantine-fault-tolerance-testing-katman-5)
8. [Performance & Load Testing (Katman 6)](#8-performance--load-testing-katman-6)
9. [Disaster Recovery Testing (Katman 7)](#9-disaster-recovery-testing-katman-7)
10. [Automated Test Suite](#10-automated-test-suite)

---

## 1. TEST STRATEJİSİ VE YAKLAŞIM

### 1.1 Test Piramidi

```
                    ┌─────────────────┐
                    │  E2E Tests (5%) │  ← Full transaction lifecycle
                    └─────────────────┘
                ┌───────────────────────┐
                │ Integration Tests (15%)│  ← Component interactions
                └───────────────────────┘
            ┌─────────────────────────────┐
            │   Protocol Tests (30%)      │  ← DKG, TSS, Consensus
            └─────────────────────────────┘
        ┌─────────────────────────────────────┐
        │      Unit Tests (50%)                │  ← Individual functions
        └─────────────────────────────────────┘
```

### 1.2 Test Kategorileri

#### Functional Testing (Fonksiyonel)
- ✅ Unit Tests - Her fonksiyon/method ayrı ayrı
- ✅ Integration Tests - Component'ler arası etkileşim
- ✅ System Tests - Tam sistem davranışı
- ✅ Acceptance Tests - Business requirements

#### Non-Functional Testing (Fonksiyonel Olmayan)
- ⚡ Performance Tests - Throughput, latency
- 🔒 Security Tests - Penetration, vulnerability
- 💪 Load Tests - Scalability, stress
- 🛡️ Byzantine Tests - Malicious node behavior
- 🔄 Recovery Tests - Disaster scenarios

### 1.3 Test Önceliklendirme (Priority Matrix)

| Priority | Category | Components | Why Critical |
|----------|----------|------------|--------------|
| **P0** | Security | mTLS, Key Management | Fund safety |
| **P0** | Consensus | Voting, Byzantine detection | Correctness |
| **P0** | Protocols | DKG, TSS Signing | Core functionality |
| **P1** | Storage | PostgreSQL, etcd | Data integrity |
| **P1** | Network | QUIC P2P | Communication |
| **P2** | API | REST endpoints | User interface |
| **P2** | Monitoring | Health checks | Observability |
| **P3** | Performance | Optimization | User experience |

---

## 2. TEST ORTAMI HAZIRLIĞI

### 2.1 Test Cluster Kurulumu

```bash
# Docker test ortamı
cd /c/Users/user/Desktop/MPC-WALLET/production/docker

# Temiz başlangıç
docker-compose down -v
docker system prune -f

# Test için log seviyesini artır
export RUST_LOG=debug,mpc_wallet=trace

# Sistemi başlat
docker-compose up -d

# Sistem sağlık kontrolü
sleep 30
make health
```

### 2.2 Test Tooling

#### Gerekli Araçlar
```bash
# HTTP testing
sudo apt-get install -y curl jq httpie

# Database tools
sudo apt-get install -y postgresql-client

# Network tools
sudo apt-get install -y netcat-openbsd tcpdump wireshark

# Load testing
sudo apt-get install -y apache2-utils wrk siege

# Bitcoin tools
wget https://bitcoin.org/bin/bitcoin-core-25.0/bitcoin-25.0-x86_64-linux-gnu.tar.gz
tar xzf bitcoin-25.0-x86_64-linux-gnu.tar.gz
sudo cp bitcoin-25.0/bin/bitcoin-cli /usr/local/bin/
```

#### Test Scripts Directory
```bash
mkdir -p /c/Users/user/Desktop/MPC-WALLET/production/tests/{unit,integration,e2e,load,security}
```

### 2.3 Monitoring Setup

```bash
# Tüm node'ların loglarını ayrı terminallerde izle
# Terminal 1
docker-compose logs -f node-1

# Terminal 2
docker-compose logs -f node-2

# Terminal 3
docker-compose logs -f node-3

# Terminal 4
docker-compose logs -f node-4

# Terminal 5
docker-compose logs -f node-5

# Terminal 6 - Infrastructure logs
docker-compose logs -f etcd-1 postgres
```

---

## 3. COMPONENT-LEVEL TESTING (Katman 1)

### 3.1 Infrastructure Testing

#### 3.1.1 PostgreSQL Testing

**Test 1: Connection Pool**
```bash
# Test database connection
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "SELECT version();"

# Expected: PostgreSQL 16.x
```

**Test 2: Schema Integrity**
```bash
# Verify all tables exist
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT table_name
  FROM information_schema.tables
  WHERE table_schema = 'public'
  ORDER BY table_name;
"

# Expected tables:
# - transactions
# - voting_rounds
# - votes
# - audit_events
# - dkg_ceremonies
# - aux_info_ceremonies
# - key_shares
# - presignatures
```

**Test 3: Write Performance**
```bash
# Insert test transaction
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  INSERT INTO transactions (
    txid, recipient, amount_satoshis, state,
    unsigned_tx, created_at
  ) VALUES (
    'test-tx-001',
    'tb1qtest123456789abcdefghijklmnopqrstuvwxyz',
    100000,
    'pending',
    E'\\\\x0200000001',
    NOW()
  ) RETURNING id, txid, state;
"

# Measure: < 10ms for single insert
```

**Test 4: Concurrent Writes**
```bash
# Test script: tests/integration/postgres_concurrent.sh
cat > /tmp/pg_concurrent_test.sh << 'EOF'
#!/bin/bash
for i in {1..100}; do
  docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
    INSERT INTO transactions (txid, recipient, amount_satoshis, state, unsigned_tx, created_at)
    VALUES ('concurrent-tx-$i', 'tb1qtest', 50000, 'pending', E'\\\\x02', NOW());
  " &
done
wait
echo "✅ 100 concurrent inserts completed"
EOF

chmod +x /tmp/pg_concurrent_test.sh
/tmp/pg_concurrent_test.sh

# Verify count
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT COUNT(*) FROM transactions WHERE txid LIKE 'concurrent-tx-%';
"
# Expected: 100
```

**Test 5: Transaction Isolation (ACID)**
```bash
# Test ACID properties
docker exec mpc-postgres psql -U mpc -d mpc_wallet << 'EOF'
BEGIN;
  INSERT INTO transactions (txid, recipient, amount_satoshis, state, unsigned_tx, created_at)
  VALUES ('acid-test-1', 'tb1qtest', 100000, 'pending', E'\\x02', NOW());

  -- Simulate error before commit
  -- Transaction should rollback
ROLLBACK;

-- Verify no data inserted
SELECT COUNT(*) FROM transactions WHERE txid = 'acid-test-1';
-- Expected: 0
EOF
```

**Test 6: Index Performance**
```bash
# Create test data
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  INSERT INTO transactions (txid, recipient, amount_satoshis, state, unsigned_tx, created_at)
  SELECT
    'perf-tx-' || i,
    'tb1qtest',
    100000,
    CASE (i % 5)
      WHEN 0 THEN 'pending'
      WHEN 1 THEN 'voting'
      WHEN 2 THEN 'approved'
      WHEN 3 THEN 'signed'
      ELSE 'confirmed'
    END,
    E'\\\\x02',
    NOW() - (i || ' minutes')::interval
  FROM generate_series(1, 10000) AS i;
"

# Test index usage on state column
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  EXPLAIN ANALYZE
  SELECT * FROM transactions WHERE state = 'pending';
"

# Expected: Index Scan on idx_transactions_state
# Execution time: < 5ms for 10k rows
```

#### 3.1.2 etcd Cluster Testing

**Test 7: Cluster Health**
```bash
# Check all nodes healthy
docker exec mpc-etcd-1 etcdctl endpoint health --cluster

# Expected:
# http://etcd-1:2379 is healthy
# http://etcd-2:2379 is healthy
# http://etcd-3:2379 is healthy
```

**Test 8: Distributed Consensus**
```bash
# Write to node 1
docker exec mpc-etcd-1 etcdctl put /test/consensus/key1 "written-from-node-1"

# Read from node 2 (should see same value)
docker exec mpc-etcd-2 etcdctl get /test/consensus/key1

# Expected: written-from-node-1

# Read from node 3 (should see same value)
docker exec mpc-etcd-3 etcdctl get /test/consensus/key1

# Expected: written-from-node-1
```

**Test 9: Lease Management**
```bash
# Create lease (TTL: 30 seconds)
LEASE_ID=$(docker exec mpc-etcd-1 etcdctl lease grant 30 | grep -oP 'lease \K[0-9a-f]+')
echo "Created lease: $LEASE_ID"

# Put key with lease
docker exec mpc-etcd-1 etcdctl put /test/lease/key1 "expires-in-30s" --lease=$LEASE_ID

# Verify key exists
docker exec mpc-etcd-1 etcdctl get /test/lease/key1

# Wait for expiration
sleep 35

# Verify key expired
docker exec mpc-etcd-1 etcdctl get /test/lease/key1
# Expected: (empty)
```

**Test 10: Transaction Support (STM)**
```bash
# Atomic transaction
docker exec mpc-etcd-1 etcdctl txn << 'EOF'
compare:
value("/test/txn/counter") = "0"

success:
put /test/txn/counter "1"
put /test/txn/log "incremented to 1"

failure:
get /test/txn/counter
EOF

# Expected: SUCCESS
```

**Test 11: Watch Mechanism**
```bash
# Start watch in background
docker exec mpc-etcd-1 etcdctl watch /test/watch/key1 &
WATCH_PID=$!

# Write to watched key
sleep 2
docker exec mpc-etcd-1 etcdctl put /test/watch/key1 "value-changed"

# Expected: Watch should trigger and show PUT event
sleep 2
kill $WATCH_PID
```

**Test 12: etcd Performance (Read/Write Latency)**
```bash
# Write latency test
time docker exec mpc-etcd-1 etcdctl put /test/perf/write "test-value"
# Expected: < 50ms

# Read latency test
time docker exec mpc-etcd-1 etcdctl get /test/perf/write
# Expected: < 10ms

# Bulk write test
for i in {1..1000}; do
  docker exec mpc-etcd-1 etcdctl put /test/perf/bulk-$i "value-$i" &
done
wait

# Expected: All 1000 writes < 30 seconds
```

**Test 13: etcd Failure Recovery**
```bash
# Stop one etcd node
docker-compose stop etcd-3

# Cluster should still be operational (quorum: 2/3)
docker exec mpc-etcd-1 etcdctl endpoint health --cluster

# Write during node failure
docker exec mpc-etcd-1 etcdctl put /test/failure/key1 "written-during-node-3-down"

# Restart failed node
docker-compose start etcd-3
sleep 10

# Verify data replicated to recovered node
docker exec mpc-etcd-3 etcdctl get /test/failure/key1
# Expected: written-during-node-3-down
```

### 3.2 Network Layer Testing (QUIC P2P)

#### 3.2.1 QUIC Listener Tests

**Test 14: QUIC Port Binding**
```bash
# Check if all nodes listening on QUIC ports
for i in {1..5}; do
  PORT=$((9000 + i))
  echo "Testing node-$i on port $PORT..."
  docker exec mpc-node-$i netstat -tuln | grep ":$PORT"
done

# Expected: Each node listening on 0.0.0.0:900X
```

**Test 15: mTLS Certificate Validation**
```bash
# Verify certificates loaded
docker logs mpc-node-1 2>&1 | grep "Successfully loaded certificates"

# Expected: Successfully loaded certificates for node 1 (party_index 0)
```

**Test 16: Peer Discovery**
```bash
# Check if nodes discover each other
docker logs mpc-node-1 2>&1 | grep -i "peer" | tail -20

# Expected: Connection attempts/successes to other nodes
```

**Test 17: QUIC Stream Creation**
```bash
# This requires looking at logs during protocol execution
# We'll test this in DKG/Signing sections
docker-compose logs -f node-1 | grep -i "quic stream"
```

#### 3.2.2 Network Partition Tests

**Test 18: Network Split (Byzantine)**
```bash
# Create network partition: nodes 1-2 vs nodes 3-5
# Disconnect node 1 and 2 from nodes 3, 4, 5

# Using iptables (if available in container)
docker exec mpc-node-1 iptables -A OUTPUT -d mpc-node-3 -j DROP
docker exec mpc-node-1 iptables -A OUTPUT -d mpc-node-4 -j DROP
docker exec mpc-node-1 iptables -A OUTPUT -d mpc-node-5 -j DROP

# Try to initiate DKG - should fail (no quorum)
curl -X POST http://localhost:8081/api/v1/cluster/dkg/start \
  -H "Content-Type: application/json" \
  -d '{"threshold": 4, "participants": [1,2,3,4,5]}'

# Expected: Timeout or quorum failure

# Restore network
docker exec mpc-node-1 iptables -F OUTPUT
```

**Test 19: Network Latency Injection**
```bash
# Add 500ms latency to node-1 → node-2 communication
docker exec mpc-node-1 tc qdisc add dev eth0 root netem delay 500ms

# Measure DKG/signing time with high latency
# (Will test in protocol section)

# Remove latency
docker exec mpc-node-1 tc qdisc del dev eth0 root
```

### 3.3 Storage Layer Testing

#### 3.3.1 PostgresStorage Tests

**Test 20: Transaction CRUD Operations**
```bash
# Create transaction via API
curl -X POST http://localhost:8081/api/v1/tx/create \
  -H "Content-Type: application/json" \
  -d '{
    "recipient": "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx",
    "amount_satoshis": 50000
  }' | jq .

# Expected: Returns transaction with txid, state=pending

# Get transaction
TXID=$(curl -s http://localhost:8081/api/v1/tx/create -X POST \
  -H "Content-Type: application/json" \
  -d '{"recipient": "tb1qtest", "amount_satoshis": 50000}' | jq -r '.txid')

curl http://localhost:8081/api/v1/tx/$TXID | jq .

# Expected: Transaction details
```

**Test 21: Voting Round Management**
```bash
# Create voting round (via transaction creation)
# Check database directly
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT
    id, tx_id, round_number, threshold, votes_received, approved, completed
  FROM voting_rounds
  ORDER BY id DESC
  LIMIT 5;
"

# Expected: Recent voting rounds
```

**Test 22: Vote Recording**
```bash
# Submit vote via API (requires authentication in production)
# For testing, insert directly
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  INSERT INTO votes (voting_round_id, node_id, vote, voted_at)
  VALUES (1, 1, true, NOW());
"

# Verify vote count updated
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT votes_received FROM voting_rounds WHERE id = 1;
"
```

#### 3.3.2 EtcdStorage Tests

**Test 23: FSM State Management**
```bash
# Store FSM state
docker exec mpc-etcd-1 etcdctl put /mpc/fsm/tx-001/state "pending"

# Read FSM state
docker exec mpc-etcd-1 etcdctl get /mpc/fsm/tx-001/state

# Update FSM state
docker exec mpc-etcd-1 etcdctl put /mpc/fsm/tx-001/state "voting"

# Verify update
docker exec mpc-etcd-1 etcdctl get /mpc/fsm/tx-001/state
# Expected: voting
```

**Test 24: Distributed Lock**
```bash
# Acquire lock for DKG ceremony
LOCK_KEY="/mpc/locks/dkg-ceremony-001"
docker exec mpc-etcd-1 etcdctl lock $LOCK_KEY echo "Lock acquired"

# Try to acquire same lock (should block)
timeout 5s docker exec mpc-etcd-2 etcdctl lock $LOCK_KEY echo "This should timeout" || echo "✅ Lock correctly blocked"
```

**Test 25: Leader Election**
```bash
# Multiple nodes compete for leadership
for i in {1..5}; do
  docker exec mpc-etcd-$((i % 3 + 1)) etcdctl elect mpc-leader "node-$i" &
done

# One should win, others should wait
sleep 5

# Check current leader
docker exec mpc-etcd-1 etcdctl get mpc-leader
```

### 3.4 Security Layer Testing

#### 3.4.1 Certificate Management

**Test 26: CA Certificate Verification**
```bash
# Verify CA certificate
docker exec mpc-node-1 cat /certs/ca.crt | openssl x509 -text -noout

# Expected:
# - Subject: CN=MPC Wallet CA
# - Valid dates
# - RSA 4096 bit key
```

**Test 27: Node Certificate Chain**
```bash
# Verify node certificate signed by CA
docker exec mpc-node-1 sh -c '
  openssl verify -CAfile /certs/ca.crt /certs/node1.crt
'

# Expected: /certs/node1.crt: OK
```

**Test 28: Certificate Expiration Check**
```bash
# Check expiration date
for i in {1..5}; do
  echo "Node $i certificate expiration:"
  docker exec mpc-node-1 openssl x509 -in /certs/node$i.crt -noout -enddate
done

# Expected: All certificates valid for > 1 year
```

**Test 29: mTLS Handshake**
```bash
# This is tested implicitly during QUIC connection
# Check logs for successful mTLS handshake
docker logs mpc-node-1 2>&1 | grep -i "tls\|handshake\|certificate"

# Expected: No certificate validation errors
```

#### 3.4.2 Key Management

**Test 30: Key Share Storage**
```bash
# After DKG (will run in protocol section), verify key shares stored
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT
    id, ceremony_id, party_index,
    length(public_key) as pubkey_len,
    length(encrypted_share) as share_len,
    created_at
  FROM key_shares
  ORDER BY id DESC
  LIMIT 5;
"

# Expected: Key shares with non-zero lengths
```

**Test 31: Key Share Encryption**
```bash
# Verify key shares are encrypted (not plaintext)
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT encrypted_share FROM key_shares LIMIT 1;
"

# Expected: Binary data (not readable plaintext)
```

### 3.5 API Layer Testing

#### 3.5.1 Health Endpoints

**Test 32: Node Health Check**
```bash
# Check all nodes
for i in {1..5}; do
  echo "Node $i health:"
  curl -s http://localhost:808$i/health | jq .
done

# Expected response:
# {
#   "status": "healthy",
#   "node_id": "N",
#   "peers_connected": 4,
#   "etcd_connected": true,
#   "postgres_connected": true
# }
```

**Test 33: Cluster Status**
```bash
curl -s http://localhost:8081/api/v1/cluster/status | jq .

# Expected:
# {
#   "cluster_size": 5,
#   "threshold": 4,
#   "healthy_nodes": 5,
#   "nodes": [...]
# }
```

#### 3.5.2 Transaction Endpoints

**Test 34: Create Transaction**
```bash
# Create valid transaction
curl -X POST http://localhost:8081/api/v1/tx/create \
  -H "Content-Type: application/json" \
  -d '{
    "recipient": "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx",
    "amount_satoshis": 100000
  }' | jq .

# Expected: 200 OK with transaction details
```

**Test 35: Get Transaction**
```bash
# Create transaction first
TXID=$(curl -s -X POST http://localhost:8081/api/v1/tx/create \
  -H "Content-Type: application/json" \
  -d '{"recipient": "tb1qtest", "amount_satoshis": 50000}' | jq -r '.txid')

# Get transaction
curl -s http://localhost:8081/api/v1/tx/$TXID | jq .

# Expected: Transaction with current state
```

**Test 36: List Transactions**
```bash
# Get all transactions
curl -s 'http://localhost:8081/api/v1/tx/list?limit=10' | jq .

# Get by state
curl -s 'http://localhost:8081/api/v1/tx/list?state=pending' | jq .

# Expected: Array of transactions
```

#### 3.5.3 DKG Endpoints

**Test 37: Start DKG Ceremony**
```bash
curl -X POST http://localhost:8081/api/v1/cluster/dkg/start \
  -H "Content-Type: application/json" \
  -d '{
    "threshold": 4,
    "participants": [1, 2, 3, 4, 5]
  }' | jq .

# Expected:
# {
#   "ceremony_id": "uuid",
#   "status": "in_progress"
# }
```

**Test 38: Check DKG Status**
```bash
# Get DKG status
curl -s http://localhost:8081/api/v1/cluster/dkg/status | jq .

# Expected:
# {
#   "status": "completed" | "in_progress" | "failed",
#   "ceremony_id": "uuid",
#   "participants": [...],
#   "completed_at": "timestamp"
# }
```

---

## 4. INTEGRATION TESTING (Katman 2)

### 4.1 Database + Network Integration

**Test 39: Transaction State Synchronization**
```bash
# Create transaction on node 1
TX_RESPONSE=$(curl -s -X POST http://localhost:8081/api/v1/tx/create \
  -H "Content-Type: application/json" \
  -d '{"recipient": "tb1qtest", "amount_satoshis": 75000}')

TXID=$(echo $TX_RESPONSE | jq -r '.txid')

# Wait for state propagation
sleep 5

# Check same transaction on all nodes
for i in {1..5}; do
  echo "Node $i view:"
  curl -s http://localhost:808$i/api/v1/tx/$TXID | jq '.state'
done

# Expected: All nodes show same state
```

**Test 40: Voting Synchronization**
```bash
# After transaction creation, check voting rounds across nodes
for i in {1..5}; do
  echo "Node $i voting rounds:"
  docker exec mpc-node-$i cat /data/voting_status.log 2>/dev/null || echo "No voting data yet"
done

# Expected: Consistent voting state across nodes
```

### 4.2 Consensus + Storage Integration

**Test 41: Vote Aggregation**
```bash
# Create transaction that requires voting
TXID=$(curl -s -X POST http://localhost:8081/api/v1/tx/create \
  -H "Content-Type: application/json" \
  -d '{"recipient": "tb1qtest", "amount_satoshis": 100000}' | jq -r '.txid')

# Monitor voting progress
while true; do
  STATE=$(curl -s http://localhost:8081/api/v1/tx/$TXID | jq -r '.state')
  echo "Current state: $STATE"

  if [ "$STATE" = "approved" ] || [ "$STATE" = "failed" ]; then
    break
  fi

  sleep 2
done

# Check final vote count in database
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT vr.votes_received, vr.threshold, vr.approved
  FROM voting_rounds vr
  JOIN transactions t ON vr.tx_id = t.txid
  WHERE t.txid = '$TXID';
"

# Expected: votes_received >= threshold, approved = true
```

---

## 5. PROTOCOL TESTING (Katman 3)

### 5.1 DKG Protocol Testing

**Test 42: Full DKG Ceremony (CGGMP24)**
```bash
# Start DKG ceremony
DKG_RESPONSE=$(curl -s -X POST http://localhost:8081/api/v1/cluster/dkg/start \
  -H "Content-Type: application/json" \
  -d '{
    "threshold": 4,
    "participants": [1, 2, 3, 4, 5],
    "protocol": "cggmp24"
  }')

CEREMONY_ID=$(echo $DKG_RESPONSE | jq -r '.ceremony_id')
echo "DKG Ceremony ID: $CEREMONY_ID"

# Monitor DKG progress on all nodes
for i in {1..5}; do
  echo "=== Node $i DKG logs ===" &
  docker logs -f mpc-node-$i 2>&1 | grep -i "dkg" &
done

# Wait for completion (typical: 30-120 seconds)
sleep 120

# Check DKG status
curl -s http://localhost:8081/api/v1/cluster/dkg/status | jq .

# Expected:
# {
#   "status": "completed",
#   "protocol": "cggmp24",
#   "public_key": "02...",
#   "participants": [1,2,3,4,5]
# }
```

**Test 43: DKG Round 1 (Commitment)**
```bash
# Monitor Round 1 messages
docker logs mpc-node-1 2>&1 | grep "DKG Round 1"

# Expected:
# - "DKG Round 1: Generated commitment"
# - "DKG Round 1: Broadcasted commitment to N peers"
# - "DKG Round 1: Received commitments from N peers"
```

**Test 44: DKG Round 2 (Share Distribution)**
```bash
# Monitor Round 2 messages
docker logs mpc-node-1 2>&1 | grep "DKG Round 2"

# Expected:
# - "DKG Round 2: Generated key shares"
# - "DKG Round 2: Sent shares to N participants"
# - "DKG Round 2: Received shares from N participants"
```

**Test 45: DKG Round 3 (Verification)**
```bash
# Monitor Round 3 messages
docker logs mpc-node-1 2>&1 | grep "DKG Round 3"

# Expected:
# - "DKG Round 3: Verified shares from all participants"
# - "DKG Round 3: Computed public key"
# - "DKG ceremony completed successfully"
```

**Test 46: DKG Key Share Persistence**
```bash
# Verify key shares stored in database
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT
    ceremony_id,
    party_index,
    length(public_key) as pk_len,
    length(encrypted_share) as share_len,
    algorithm,
    created_at
  FROM key_shares
  WHERE ceremony_id = '$CEREMONY_ID';
"

# Expected: 5 rows (one per participant) with:
# - party_index: 0-4
# - pk_len: 33 (compressed SECP256K1 pubkey)
# - share_len: > 0 (encrypted share data)
# - algorithm: 'cggmp24'
```

**Test 47: DKG Failure - Insufficient Participants**
```bash
# Try DKG with only 3 nodes (threshold=4)
curl -X POST http://localhost:8081/api/v1/cluster/dkg/start \
  -H "Content-Type: application/json" \
  -d '{
    "threshold": 4,
    "participants": [1, 2, 3]
  }' | jq .

# Expected: Error - "Insufficient participants for threshold"
```

**Test 48: DKG Failure - Node Timeout**
```bash
# Stop node-5 during DKG
docker-compose stop node-5

# Start DKG with all 5 nodes
curl -X POST http://localhost:8081/api/v1/cluster/dkg/start \
  -H "Content-Type: application/json" \
  -d '{
    "threshold": 4,
    "participants": [1, 2, 3, 4, 5]
  }'

# Monitor for timeout
sleep 60

# Check status
curl -s http://localhost:8081/api/v1/cluster/dkg/status | jq .

# Expected: Timeout or failed status

# Restart node-5
docker-compose start node-5
```

### 5.2 Aux Info Protocol Testing

**Test 49: Aux Info Ceremony**
```bash
# After successful DKG, start aux info
AUX_RESPONSE=$(curl -s -X POST http://localhost:8081/api/v1/cluster/aux-info/start \
  -H "Content-Type: application/json" \
  -d '{
    "ceremony_id": "'$CEREMONY_ID'",
    "participants": [1, 2, 3, 4, 5]
  }')

AUX_CEREMONY_ID=$(echo $AUX_RESPONSE | jq -r '.ceremony_id')

# Monitor completion
sleep 60

# Check status
curl -s http://localhost:8081/api/v1/cluster/aux-info/status | jq .

# Expected: completed
```

**Test 50: Aux Info Data Verification**
```bash
# Verify aux info stored
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT
    ceremony_id, party_index,
    length(aux_data) as data_len,
    created_at
  FROM aux_info_ceremonies
  WHERE ceremony_id = '$AUX_CEREMONY_ID';
"

# Expected: 5 rows with aux_data
```

### 5.3 Presignature Generation Testing

**Test 51: Presignature Pool Management**
```bash
# Generate presignatures
PRESIG_RESPONSE=$(curl -s -X POST http://localhost:8081/api/v1/cluster/presig/generate \
  -H "Content-Type: application/json" \
  -d '{
    "count": 10,
    "protocol": "cggmp24"
  }')

echo $PRESIG_RESPONSE | jq .

# Monitor generation
docker logs -f mpc-node-1 | grep -i "presignature"

# Expected: 10 presignatures generated
```

**Test 52: Presignature Pool Status**
```bash
# Check pool status
curl -s http://localhost:8081/api/v1/cluster/presig/status | jq .

# Expected:
# {
#   "total": 10,
#   "available": 10,
#   "in_use": 0,
#   "protocol": "cggmp24"
# }
```

### 5.4 TSS Signing Protocol Testing

**Test 53: CGGMP24 Signing (P2WPKH)**
```bash
# Create wallet first (if not exists)
WALLET_RESPONSE=$(curl -s -X POST http://localhost:8081/api/v1/wallet/create \
  -H "Content-Type: application/json" \
  -d '{
    "network": "testnet",
    "derivation_path": "m/84h/1h/0h/0/0"
  }')

ADDRESS=$(echo $WALLET_RESPONSE | jq -r '.address')
echo "Wallet address: $ADDRESS"

# Create transaction to P2WPKH address
TX_RESPONSE=$(curl -s -X POST http://localhost:8081/api/v1/tx/create \
  -H "Content-Type: application/json" \
  -d '{
    "recipient": "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx",
    "amount_satoshis": 50000
  }')

TXID=$(echo $TX_RESPONSE | jq -r '.txid')

# Monitor transaction lifecycle
while true; do
  STATE=$(curl -s http://localhost:8081/api/v1/tx/$TXID | jq -r '.state')
  echo "$(date) - State: $STATE"

  # Check logs for signing activity
  docker logs --tail=10 mpc-node-1 2>&1 | grep -i "sign\|signature"

  if [ "$STATE" = "signed" ] || [ "$STATE" = "failed" ]; then
    break
  fi

  sleep 5
done

# Verify signature
curl -s http://localhost:8081/api/v1/tx/$TXID | jq .

# Expected:
# - state: "signed"
# - signed_tx: hex-encoded transaction with witness
```

**Test 54: FROST Signing (P2TR Taproot)**
```bash
# Create transaction to P2TR address
TX_RESPONSE=$(curl -s -X POST http://localhost:8081/api/v1/tx/create \
  -H "Content-Type: application/json" \
  -d '{
    "recipient": "tb1p5d7rjq7g6rdk2yhzks9smlaqtedr4dekq08ge8ztwac72sfr9rusxg3297",
    "amount_satoshis": 75000
  }')

TXID=$(echo $TX_RESPONSE | jq -r '.txid')

# Monitor signing with FROST protocol
docker logs -f mpc-node-1 2>&1 | grep -i "frost"

# Wait for completion
sleep 60

# Verify
curl -s http://localhost:8081/api/v1/tx/$TXID | jq '.state, .protocol'

# Expected:
# - state: "signed"
# - protocol: "frost"
```

**Test 55: Signature Verification**
```bash
# Get signed transaction
SIGNED_TX=$(curl -s http://localhost:8081/api/v1/tx/$TXID | jq -r '.signed_tx')

# Decode and verify with bitcoin-cli (if available)
echo $SIGNED_TX | bitcoin-cli -testnet decoderawtransaction /dev/stdin

# Expected: Valid transaction structure with witness
```

### 5.5 Protocol Failure Scenarios

**Test 56: Signing Timeout**
```bash
# Stop 2 nodes (below threshold)
docker-compose stop node-4 node-5

# Try to create and sign transaction
TX_RESPONSE=$(curl -s -X POST http://localhost:8081/api/v1/tx/create \
  -H "Content-Type: application/json" \
  -d '{"recipient": "tb1qtest", "amount_satoshis": 50000}')

TXID=$(echo $TX_RESPONSE | jq -r '.txid')

# Wait for timeout (60 seconds)
sleep 65

# Check state
curl -s http://localhost:8081/api/v1/tx/$TXID | jq '.state'

# Expected: "failed" (timeout)

# Restart nodes
docker-compose start node-4 node-5
```

**Test 57: Invalid Share Detection**
```bash
# This requires manual intervention (modifying share data)
# In production, Byzantine node detection should catch this

# Simulate by stopping DKG mid-ceremony
curl -X POST http://localhost:8081/api/v1/cluster/dkg/start \
  -H "Content-Type: application/json" \
  -d '{"threshold": 4, "participants": [1,2,3,4,5]}' &

# Wait 5 seconds (mid-ceremony)
sleep 5

# Stop node-3
docker-compose stop node-3

# Expected: DKG should eventually timeout and fail

docker-compose start node-3
```

---

## 6. SECURITY TESTING (Katman 4)

### 6.1 Authentication & Authorization

**Test 58: Unauthenticated Access (API)**
```bash
# Try to access admin endpoints without auth
curl -X POST http://localhost:8081/api/v1/cluster/shutdown \
  -H "Content-Type: application/json"

# Expected: 401 Unauthorized (in production)
# Note: Current version may not have auth enabled
```

**Test 59: mTLS Mutual Authentication**
```bash
# Verify QUIC connections require valid certificates
# Try to connect without certificate (should fail)

# This is tested implicitly - nodes with invalid certs can't join
```

### 6.2 Data Encryption

**Test 60: At-Rest Encryption (Key Shares)**
```bash
# Verify key shares are encrypted in database
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT
    substring(encode(encrypted_share, 'hex'), 1, 40) as share_preview,
    length(encrypted_share) as encrypted_len
  FROM key_shares
  LIMIT 1;
"

# Expected: Encrypted data (not plaintext keys)
```

**Test 61: In-Transit Encryption (QUIC/TLS)**
```bash
# Capture network traffic and verify encryption
# This requires tcpdump/wireshark

# Capture on QUIC port during signing
docker exec mpc-node-1 tcpdump -i any -n port 9001 -w /tmp/quic_capture.pcap &
TCPDUMP_PID=$!

# Trigger signing ceremony
curl -X POST http://localhost:8081/api/v1/tx/create \
  -H "Content-Type: application/json" \
  -d '{"recipient": "tb1qtest", "amount_satoshis": 50000}'

sleep 30
kill $TCPDUMP_PID

# Analyze capture - should see encrypted QUIC packets
docker cp mpc-node-1:/tmp/quic_capture.pcap /tmp/
# Open in Wireshark - verify no plaintext key material
```

### 6.3 Input Validation

**Test 62: Invalid Bitcoin Address**
```bash
# Try to create transaction with invalid address
curl -X POST http://localhost:8081/api/v1/tx/create \
  -H "Content-Type: application/json" \
  -d '{
    "recipient": "invalid_bitcoin_address",
    "amount_satoshis": 50000
  }'

# Expected: 400 Bad Request - invalid address format
```

**Test 63: Negative Amount**
```bash
curl -X POST http://localhost:8081/api/v1/tx/create \
  -H "Content-Type: application/json" \
  -d '{
    "recipient": "tb1qtest",
    "amount_satoshis": -1000
  }'

# Expected: 400 Bad Request - invalid amount
```

**Test 64: SQL Injection Attempt**
```bash
# Try SQL injection in transaction ID
curl "http://localhost:8081/api/v1/tx/' OR '1'='1"

# Expected: 400 or 404 (not SQL error)
```

### 6.4 Rate Limiting

**Test 65: API Rate Limiting**
```bash
# Flood API with requests
for i in {1..1000}; do
  curl -s -X POST http://localhost:8081/api/v1/tx/create \
    -H "Content-Type: application/json" \
    -d '{"recipient": "tb1qtest", "amount_satoshis": 1000}' &
done

# Expected: Some requests should be rate-limited (429 Too Many Requests)
# Note: May not be implemented yet
```

---

## 7. BYZANTINE FAULT TOLERANCE TESTING (Katman 5)

### 7.1 Byzantine Node Detection

**Test 66: Malicious Node - Invalid Signature**
```bash
# Simulate malicious node sending invalid signature shares
# This requires modifying node behavior (advanced)

# For now, test detection mechanism by checking logs
docker logs mpc-node-1 2>&1 | grep -i "byzantine\|invalid\|malicious"

# Expected: Detection and rejection of invalid messages
```

**Test 67: Byzantine Node - Double Voting**
```bash
# Try to vote twice on same transaction
# Insert duplicate vote manually

TXID="test-double-vote-tx"

docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  -- Create transaction
  INSERT INTO transactions (txid, recipient, amount_satoshis, state, unsigned_tx, created_at)
  VALUES ('$TXID', 'tb1qtest', 50000, 'voting', E'\\\\x02', NOW());

  -- Create voting round
  INSERT INTO voting_rounds (tx_id, round_number, total_nodes, threshold, votes_received, started_at, timeout_at)
  VALUES ('$TXID', 1, 5, 4, 0, NOW(), NOW() + INTERVAL '5 minutes')
  RETURNING id;
"

ROUND_ID=$(docker exec mpc-postgres psql -U mpc -d mpc_wallet -t -c "
  SELECT id FROM voting_rounds WHERE tx_id = '$TXID';
" | tr -d ' ')

# Try to insert duplicate vote
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  INSERT INTO votes (voting_round_id, node_id, vote, voted_at)
  VALUES ($ROUND_ID, 1, true, NOW());

  INSERT INTO votes (voting_round_id, node_id, vote, voted_at)
  VALUES ($ROUND_ID, 1, true, NOW());
"

# Expected: Second insert should fail (unique constraint)
```

**Test 68: Byzantine Node - Conflicting Messages**
```bash
# Send conflicting DKG messages
# This requires implementing Byzantine behavior

# Check Byzantine detection in logs
docker logs mpc-node-1 2>&1 | grep -i "conflict\|byzantine"
```

### 7.2 Threshold Security

**Test 69: Signature Generation Below Threshold**
```bash
# Try to generate signature with only 3 nodes (threshold=4)
docker-compose stop node-4 node-5

# Attempt signing
curl -X POST http://localhost:8081/api/v1/tx/create \
  -H "Content-Type: application/json" \
  -d '{"recipient": "tb1qtest", "amount_satoshis": 50000}'

# Wait for signing attempt
sleep 60

# Expected: Signing should fail (insufficient participants)

docker-compose start node-4 node-5
```

**Test 70: t-out-of-n Security (Any 4 of 5)**
```bash
# Generate signature with nodes 1,2,3,4 (exclude 5)
docker-compose stop node-5

# Create and sign transaction
TX_RESPONSE=$(curl -s -X POST http://localhost:8081/api/v1/tx/create \
  -H "Content-Type: application/json" \
  -d '{"recipient": "tb1qtest", "amount_satoshis": 50000}')

TXID=$(echo $TX_RESPONSE | jq -r '.txid')

# Wait for signing
sleep 60

# Verify signature succeeded with 4 nodes
curl -s http://localhost:8081/api/v1/tx/$TXID | jq '.state'

# Expected: "signed" (4 nodes sufficient)

docker-compose start node-5
```

---

## 8. PERFORMANCE & LOAD TESTING (Katman 6)

### 8.1 Throughput Testing

**Test 71: Transaction Creation Rate**
```bash
# Measure transaction creation throughput
START_TIME=$(date +%s)

for i in {1..100}; do
  curl -s -X POST http://localhost:8081/api/v1/tx/create \
    -H "Content-Type: application/json" \
    -d "{\"recipient\": \"tb1qtest\", \"amount_satoshis\": $((50000 + i))}" &
done

wait

END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

echo "Created 100 transactions in $DURATION seconds"
echo "Throughput: $((100 / DURATION)) tx/sec"

# Expected: > 10 tx/sec
```

**Test 72: Concurrent Signing Load**
```bash
# Create multiple transactions that will be signed concurrently
for i in {1..10}; do
  curl -s -X POST http://localhost:8081/api/v1/tx/create \
    -H "Content-Type: application/json" \
    -d '{"recipient": "tb1qtest", "amount_satoshis": 50000}' &
done

wait

# Monitor signing throughput
docker logs -f mpc-node-1 | grep -i "signing completed"

# Measure time to sign all 10
# Expected: Parallel processing, < 5 minutes total
```

### 8.2 Latency Testing

**Test 73: DKG Latency**
```bash
# Measure DKG ceremony time
START=$(date +%s)

curl -X POST http://localhost:8081/api/v1/cluster/dkg/start \
  -H "Content-Type: application/json" \
  -d '{"threshold": 4, "participants": [1,2,3,4,5]}'

# Wait for completion
while true; do
  STATUS=$(curl -s http://localhost:8081/api/v1/cluster/dkg/status | jq -r '.status')
  if [ "$STATUS" = "completed" ]; then
    break
  fi
  sleep 2
done

END=$(date +%s)
LATENCY=$((END - START))

echo "DKG ceremony completed in $LATENCY seconds"

# Expected: < 120 seconds for 5 nodes
```

**Test 74: Signing Latency**
```bash
# Measure end-to-end signing time
START=$(date +%s)

TXID=$(curl -s -X POST http://localhost:8081/api/v1/tx/create \
  -H "Content-Type: application/json" \
  -d '{"recipient": "tb1qtest", "amount_satoshis": 50000}' | jq -r '.txid')

# Wait for signing completion
while true; do
  STATE=$(curl -s http://localhost:8081/api/v1/tx/$TXID | jq -r '.state')
  if [ "$STATE" = "signed" ] || [ "$STATE" = "failed" ]; then
    break
  fi
  sleep 1
done

END=$(date +%s)
LATENCY=$((END - START))

echo "Transaction signed in $LATENCY seconds"

# Expected: < 30 seconds (with presignatures), < 60 seconds (without)
```

### 8.3 Resource Utilization

**Test 75: CPU Usage**
```bash
# Monitor CPU usage during load
docker stats --no-stream | grep mpc-node

# Run load test
for i in {1..50}; do
  curl -s -X POST http://localhost:8081/api/v1/tx/create \
    -H "Content-Type: application/json" \
    -d '{"recipient": "tb1qtest", "amount_satoshis": 50000}' &
done

# Monitor CPU during signing
docker stats mpc-node-1 mpc-node-2 mpc-node-3 mpc-node-4 mpc-node-5

# Expected: CPU usage spike during cryptographic operations
```

**Test 76: Memory Usage**
```bash
# Monitor memory over time
while true; do
  echo "$(date) - Memory usage:"
  docker stats --no-stream --format "table {{.Name}}\t{{.MemUsage}}" | grep mpc-node
  sleep 10
done

# Run in background, monitor for memory leaks over hours
# Expected: Stable memory usage (no leaks)
```

**Test 77: Database Connection Pool**
```bash
# Monitor PostgreSQL connections
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT
    count(*) as active_connections,
    max_conn
  FROM pg_stat_activity, (SELECT setting::int AS max_conn FROM pg_settings WHERE name = 'max_connections') AS mc
  GROUP BY max_conn;
"

# Under load, connections should not exceed pool size
# Expected: < 50 connections (10 per node * 5 nodes)
```

### 8.4 Scalability Testing

**Test 78: Network Throughput (QUIC)**
```bash
# Measure network throughput during DKG/signing
# Monitor with iftop or nethogs

docker exec mpc-node-1 apt-get update && apt-get install -y iftop

# Run DKG while monitoring
docker exec mpc-node-1 iftop -i eth0 &

curl -X POST http://localhost:8081/api/v1/cluster/dkg/start \
  -H "Content-Type: application/json" \
  -d '{"threshold": 4, "participants": [1,2,3,4,5]}'

# Expected: Peak bandwidth during message exchange
```

---

## 9. DISASTER RECOVERY TESTING (Katman 7)

### 9.1 Node Failure Recovery

**Test 79: Single Node Restart**
```bash
# Stop node-3
docker-compose stop node-3

# System should continue (threshold still met)
curl -s http://localhost:8081/api/v1/cluster/status | jq '.healthy_nodes'
# Expected: 4

# Restart node-3
docker-compose start node-3
sleep 10

# Verify rejoined cluster
curl -s http://localhost:8081/api/v1/cluster/status | jq '.healthy_nodes'
# Expected: 5
```

**Test 80: Simultaneous Node Failures (1 node, threshold safe)**
```bash
# Stop 1 node (threshold=4, still have 4 nodes)
docker-compose stop node-5

# Create and sign transaction
TXID=$(curl -s -X POST http://localhost:8081/api/v1/tx/create \
  -H "Content-Type: application/json" \
  -d '{"recipient": "tb1qtest", "amount_satoshis": 50000}' | jq -r '.txid')

# Wait for signing
sleep 60

# Verify success
curl -s http://localhost:8081/api/v1/tx/$TXID | jq '.state'
# Expected: "signed"

docker-compose start node-5
```

**Test 81: Simultaneous Node Failures (2 nodes, threshold UNSAFE)**
```bash
# Stop 2 nodes (threshold=4, only 3 nodes left)
docker-compose stop node-4 node-5

# Try to create and sign transaction
TXID=$(curl -s -X POST http://localhost:8081/api/v1/tx/create \
  -H "Content-Type: application/json" \
  -d '{"recipient": "tb1qtest", "amount_satoshis": 50000}' | jq -r '.txid')

# Wait for timeout
sleep 90

# Verify failure
curl -s http://localhost:8081/api/v1/tx/$TXID | jq '.state'
# Expected: "failed" (insufficient nodes)

docker-compose start node-4 node-5
```

### 9.2 Database Recovery

**Test 82: PostgreSQL Crash Recovery**
```bash
# Force PostgreSQL crash
docker-compose kill postgres

# Restart immediately
docker-compose start postgres
sleep 15

# Verify data integrity
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT COUNT(*) FROM transactions;
"

# Expected: All transactions still present (WAL recovery)
```

**Test 83: etcd Cluster Quorum Loss**
```bash
# Stop 2 etcd nodes (lose quorum)
docker-compose stop etcd-2 etcd-3

# System should detect etcd unavailability
curl -s http://localhost:8081/health | jq '.etcd_connected'
# Expected: false

# Restart etcd nodes
docker-compose start etcd-2 etcd-3
sleep 10

# Verify recovery
curl -s http://localhost:8081/health | jq '.etcd_connected'
# Expected: true
```

### 9.3 Data Backup & Restore

**Test 84: PostgreSQL Backup**
```bash
# Create backup
docker exec mpc-postgres pg_dump -U mpc -d mpc_wallet > /tmp/mpc_backup_$(date +%Y%m%d_%H%M%S).sql

# Verify backup size
ls -lh /tmp/mpc_backup_*.sql

# Expected: Non-zero size backup file
```

**Test 85: PostgreSQL Restore**
```bash
# Get latest backup
BACKUP=$(ls -t /tmp/mpc_backup_*.sql | head -1)

# Create test database
docker exec mpc-postgres psql -U mpc -c "CREATE DATABASE mpc_wallet_restore;"

# Restore
cat $BACKUP | docker exec -i mpc-postgres psql -U mpc -d mpc_wallet_restore

# Verify data
docker exec mpc-postgres psql -U mpc -d mpc_wallet_restore -c "
  SELECT COUNT(*) FROM transactions;
"

# Clean up
docker exec mpc-postgres psql -U mpc -c "DROP DATABASE mpc_wallet_restore;"
```

**Test 86: etcd Snapshot & Restore**
```bash
# Create etcd snapshot
docker exec mpc-etcd-1 etcdctl snapshot save /tmp/etcd_backup.db

# Copy snapshot out
docker cp mpc-etcd-1:/tmp/etcd_backup.db /tmp/

# Simulate data loss (write then delete)
docker exec mpc-etcd-1 etcdctl put /test/restore/key "value-to-be-lost"
docker exec mpc-etcd-1 etcdctl del /test/restore/key

# Restore snapshot (requires cluster restart - advanced)
# Expected: Data restored from snapshot
```

---

## 10. AUTOMATED TEST SUITE

### 10.1 Test Automation Scripts

**Script 1: Full Integration Test**
```bash
cat > /tmp/full_integration_test.sh << 'SCRIPT_END'
#!/bin/bash
set -e

echo "========================================="
echo "MPC WALLET - FULL INTEGRATION TEST SUITE"
echo "========================================="
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

PASSED=0
FAILED=0

# Test function
run_test() {
  TEST_NAME="$1"
  TEST_CMD="$2"

  echo -n "Running: $TEST_NAME ... "

  if eval "$TEST_CMD" > /tmp/test_output.log 2>&1; then
    echo -e "${GREEN}PASSED${NC}"
    ((PASSED++))
  else
    echo -e "${RED}FAILED${NC}"
    echo "Error output:"
    cat /tmp/test_output.log
    ((FAILED++))
  fi
}

# Infrastructure tests
echo "=== INFRASTRUCTURE TESTS ==="
run_test "PostgreSQL connection" "docker exec mpc-postgres psql -U mpc -d mpc_wallet -c 'SELECT 1;'"
run_test "etcd cluster health" "docker exec mpc-etcd-1 etcdctl endpoint health --cluster"

# API tests
echo ""
echo "=== API TESTS ==="
run_test "Node 1 health" "curl -sf http://localhost:8081/health"
run_test "Node 2 health" "curl -sf http://localhost:8082/health"
run_test "Cluster status" "curl -sf http://localhost:8081/api/v1/cluster/status"

# Transaction tests
echo ""
echo "=== TRANSACTION TESTS ==="
run_test "Create transaction" "curl -sf -X POST http://localhost:8081/api/v1/tx/create -H 'Content-Type: application/json' -d '{\"recipient\": \"tb1qtest\", \"amount_satoshis\": 50000}'"

# DKG tests
echo ""
echo "=== PROTOCOL TESTS ==="
run_test "DKG ceremony" "curl -sf -X POST http://localhost:8081/api/v1/cluster/dkg/start -H 'Content-Type: application/json' -d '{\"threshold\": 4, \"participants\": [1,2,3,4,5]}'"

# Summary
echo ""
echo "========================================="
echo "TEST SUMMARY"
echo "========================================="
echo -e "Passed: ${GREEN}$PASSED${NC}"
echo -e "Failed: ${RED}$FAILED${NC}"
echo "Total: $((PASSED + FAILED))"

if [ $FAILED -eq 0 ]; then
  echo -e "${GREEN}ALL TESTS PASSED!${NC}"
  exit 0
else
  echo -e "${RED}SOME TESTS FAILED${NC}"
  exit 1
fi
SCRIPT_END

chmod +x /tmp/full_integration_test.sh
```

**Script 2: Continuous Monitoring**
```bash
cat > /tmp/continuous_monitor.sh << 'SCRIPT_END'
#!/bin/bash

while true; do
  clear
  echo "=== MPC WALLET - SYSTEM MONITOR ==="
  echo "Time: $(date)"
  echo ""

  echo "--- Node Health ---"
  for i in {1..5}; do
    STATUS=$(curl -s http://localhost:808$i/health 2>/dev/null | jq -r '.status // "unreachable"')
    echo "Node $i: $STATUS"
  done

  echo ""
  echo "--- Recent Transactions ---"
  docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
    SELECT txid, state, created_at
    FROM transactions
    ORDER BY created_at DESC
    LIMIT 5;
  " 2>/dev/null

  echo ""
  echo "--- Container Status ---"
  docker-compose ps

  sleep 10
done
SCRIPT_END

chmod +x /tmp/continuous_monitor.sh
```

**Script 3: Load Test Runner**
```bash
cat > /tmp/load_test.sh << 'SCRIPT_END'
#!/bin/bash

CONCURRENCY=${1:-10}
TOTAL_REQUESTS=${2:-100}

echo "Running load test: $TOTAL_REQUESTS requests, $CONCURRENCY concurrent"

START_TIME=$(date +%s)

for ((i=1; i<=TOTAL_REQUESTS; i++)); do
  curl -s -X POST http://localhost:8081/api/v1/tx/create \
    -H "Content-Type: application/json" \
    -d "{\"recipient\": \"tb1qtest\", \"amount_satoshis\": $((50000 + i))}" > /dev/null &

  # Limit concurrency
  if [ $((i % CONCURRENCY)) -eq 0 ]; then
    wait
  fi
done

wait

END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

echo "Completed $TOTAL_REQUESTS requests in $DURATION seconds"
echo "Throughput: $((TOTAL_REQUESTS / DURATION)) requests/sec"
SCRIPT_END

chmod +x /tmp/load_test.sh
```

### 10.2 Test Execution Plan

#### Phase 1: Smoke Tests (5 minutes)
```bash
# Run basic smoke tests
/tmp/full_integration_test.sh
```

#### Phase 2: Component Tests (30 minutes)
```bash
# Run all component tests from sections 3.1 - 3.5
# Execute tests 1-38 sequentially
```

#### Phase 3: Integration Tests (1 hour)
```bash
# Run integration tests from section 4
# Execute tests 39-41
```

#### Phase 4: Protocol Tests (2 hours)
```bash
# Run full protocol suite from section 5
# Execute tests 42-57
```

#### Phase 5: Security Tests (1 hour)
```bash
# Run security tests from section 6
# Execute tests 58-65
```

#### Phase 6: Byzantine Tests (2 hours)
```bash
# Run Byzantine fault tolerance tests from section 7
# Execute tests 66-70
```

#### Phase 7: Performance Tests (3 hours)
```bash
# Run performance and load tests from section 8
# Execute tests 71-78
```

#### Phase 8: Recovery Tests (1 hour)
```bash
# Run disaster recovery tests from section 9
# Execute tests 79-86
```

---

## APPENDIX A: Test Data Cleanup

**Cleanup Script**
```bash
cat > /tmp/cleanup_test_data.sh << 'SCRIPT_END'
#!/bin/bash

echo "Cleaning up test data..."

# PostgreSQL cleanup
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  DELETE FROM votes WHERE voting_round_id IN (
    SELECT id FROM voting_rounds WHERE tx_id LIKE 'test-%' OR tx_id LIKE 'concurrent-%'
  );

  DELETE FROM voting_rounds WHERE tx_id LIKE 'test-%' OR tx_id LIKE 'concurrent-%';

  DELETE FROM transactions WHERE txid LIKE 'test-%' OR txid LIKE 'concurrent-%';

  DELETE FROM audit_events WHERE tx_id LIKE 'test-%' OR tx_id LIKE 'concurrent-%';
"

# etcd cleanup
docker exec mpc-etcd-1 etcdctl del --prefix /test/

echo "✅ Cleanup completed"
SCRIPT_END

chmod +x /tmp/cleanup_test_data.sh
```

---

## APPENDIX B: Expected Test Results Summary

| Test Category | Total Tests | P0 (Critical) | Estimated Time |
|---------------|-------------|---------------|----------------|
| Infrastructure | 13 | 13 | 30 min |
| Network | 6 | 5 | 45 min |
| Storage | 6 | 6 | 30 min |
| Security | 5 | 4 | 1 hour |
| API | 7 | 3 | 30 min |
| Integration | 3 | 3 | 1 hour |
| Protocols | 16 | 16 | 3 hours |
| Security | 8 | 8 | 2 hours |
| Byzantine | 5 | 5 | 2 hours |
| Performance | 8 | 2 | 3 hours |
| Recovery | 8 | 6 | 2 hours |
| **TOTAL** | **86** | **71** | **~16 hours** |

---

## NEXT STEPS

1. **Execute smoke tests** - Verify basic system functionality
2. **Run component tests** - Test each layer independently
3. **Integration testing** - Verify component interactions
4. **Protocol validation** - Full DKG and signing ceremonies
5. **Security audit** - Penetration testing and vulnerability scan
6. **Performance benchmarking** - Establish baseline metrics
7. **Chaos engineering** - Byzantine and failure injection
8. **Production readiness** - Final checklist before mainnet

**Ready to begin testing?** Start with:
```bash
cd /c/Users/user/Desktop/MPC-WALLET/production/docker
make health  # Verify system is running
/tmp/full_integration_test.sh  # Run smoke tests
```
