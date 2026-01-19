# 🔗 P2P Direct Messaging Test Planı

## 📋 AMAÇ

Request-Response protokolünün çalıştığını test etmek ve binary vs JSON performansını kıyaslamak.

---

## 🧪 TEST SENARYOLARI

### **Test 1: Basic P2P Connectivity**

**Amaç:** İki node arasında direct message gönderebilme

```rust
// Node 1'den Node 2'ye mesaj gönder
DirectRequest::CustomMessage {
    message: "Hello from Node 1!".to_string()
}

// Beklenen Response:
DirectResponse::CustomMessage {
    message: "Received: Hello from Node 1!".to_string()
}
```

**Success Criteria:**
- ✅ Message delivered
- ✅ Response received
- ✅ Latency < 10ms (local Docker)

---

### **Test 2: Vote Status Query (P2P)**

**Amaç:** Bir node'dan diğerine vote durumu sorgulamak

```rust
// Node 2'den Node 1'e vote status sorgusu
DirectRequest::GetVoteStatus {
    tx_id: "tx_001".to_string()
}

// Beklenen Response:
DirectResponse::VoteStatus {
    tx_id: "tx_001",
    status: VoteStatus::InProgress,
    vote_count: 3,
    threshold: 4,
}
```

**Success Criteria:**
- ✅ Correct status returned
- ✅ etcd query successful
- ✅ Response time < 15ms

---

### **Test 3: Public Key Exchange**

**Amaç:** Node'lar arası public key paylaşımı

```rust
DirectRequest::GetPublicKey

// Response:
DirectResponse::PublicKey {
    public_key: vec![...], // Ed25519 public key
}
```

**Success Criteria:**
- ✅ Valid public key returned
- ✅ Key matches node's actual public key

---

### **Test 4: Reputation Query**

**Amaç:** Başka bir node'un reputation score'unu öğrenme

```rust
DirectRequest::GetReputation {
    node_id: "node_3".to_string()
}

// Response:
DirectResponse::Reputation {
    node_id: "node_3",
    score: 95,
}
```

**Success Criteria:**
- ✅ Reputation retrieved from PostgreSQL
- ✅ Accurate score

---

## 🚀 PERFORMANS BENCHMARKları

### **Latency Test (Round-Trip Time)**

```
Iterations: 1000
Message Size: ~100 bytes

Expected Results:
  Min:  1-2 ms
  Avg:  3-5 ms
  Max:  10-15 ms
  P99:  < 10 ms
```

### **Throughput Test**

```
Duration: 10 seconds
Concurrent requests: 10

Expected Throughput:
  JSON:    200-300 req/sec
  Binary:  400-600 req/sec

Speedup: ~2x with bincode
```

### **Serialization Overhead**

```
Message: DirectRequest::GetVoteStatus

JSON Serialization:
  Size:     ~80 bytes
  Time:     ~15 μs

Binary Serialization:
  Size:     ~50 bytes (38% smaller)
  Time:     ~5 μs (3x faster)
```

---

## 🔧 IMPLEMENTATION CHECKLIST

### **Phase 1: Basic Messaging** ✅ (Already Done)
- [x] DirectRequest enum defined
- [x] DirectResponse enum defined
- [x] DirectMessageCodec implemented
- [x] Request-Response behaviour added to ThresholdBehavior

### **Phase 2: Node Integration** (TODO)
- [ ] Add send_request() method to P2PNode
- [ ] Add request handler in event loop
- [ ] Implement GetVoteStatus handler
- [ ] Implement GetPublicKey handler
- [ ] Implement GetReputation handler

### **Phase 3: CLI Commands** (TODO)
- [ ] `send` command for direct messaging
- [ ] `query` command for vote status
- [ ] `peer-info` command for public key
- [ ] Performance measurement in CLI

### **Phase 4: Benchmarking** (TODO)
- [ ] Latency measurement (RTT)
- [ ] Throughput measurement
- [ ] Serialization comparison (JSON vs Binary)
- [ ] Network utilization metrics

---

## 📊 EXPECTED VS ACTUAL RESULTS

### **Latency Comparison:**

| Message Type | JSON | Binary | Improvement |
|--------------|------|--------|-------------|
| Ping/Pong | 3ms | 2ms | 1.5x |
| Vote Status | 8ms | 5ms | 1.6x |
| Public Key | 4ms | 3ms | 1.3x |
| Reputation | 10ms | 7ms | 1.4x |

### **Size Comparison:**

| Message Type | JSON | Binary | Reduction |
|--------------|------|--------|-----------|
| Custom Message | 85 bytes | 52 bytes | 38% |
| Vote Status | 120 bytes | 75 bytes | 38% |
| Public Key | 150 bytes | 100 bytes | 33% |

---

## 🎯 SUCCESS CRITERIA (Overall)

### **Functionality:**
- ✅ All 4 request types work
- ✅ Responses are correct
- ✅ No packet loss
- ✅ Handles errors gracefully

### **Performance:**
- ✅ Latency < 10ms (p95)
- ✅ Binary is 2x faster than JSON
- ✅ Binary is 30-40% smaller
- ✅ Throughput > 300 req/sec

### **Reliability:**
- ✅ Works with Noise encryption
- ✅ No connection drops
- ✅ Handles concurrent requests
- ✅ Byzantine-resilient (signed messages)

---

## 🔍 DEBUGGING CHECKLIST

If P2P messaging fails:

1. **Check Peer Connection:**
   ```powershell
   docker logs threshold-node1 2>&1 | grep "Connection established"
   ```

2. **Check Request-Response Protocol:**
   ```powershell
   docker logs threshold-node1 2>&1 | grep "request_response"
   ```

3. **Verify Peer IDs:**
   ```powershell
   docker logs threshold-node1 2>&1 | grep "Local peer id"
   ```

4. **Network Connectivity:**
   ```powershell
   docker exec threshold-node1 ping -c 3 threshold-node2
   ```

5. **Protocol Registration:**
   - Check that `/threshold-voting/direct-message/1.0.0` is registered
   - Verify ProtocolSupport::Full is set

---

## 💡 NEXT STEPS

1. **Complete CLI Integration**
   - Implement send command
   - Add response handling
   - Display latency stats

2. **Add Benchmarking**
   - Implement benchmark subcommand
   - Measure latency distribution
   - Compare JSON vs Binary

3. **P2P Stress Test**
   - 1000 concurrent requests
   - Multiple nodes
   - Byzantine attack simulation

4. **Performance Optimization**
   - Connection pooling
   - Request batching
   - Response caching

---

## ✅ FINAL DELIVERABLE

Modern CLI ile kullanıcı şunu yapabilmeli:

```bash
# Peer'e mesaj gönder
threshold-voting-system send --peer-id 12D3KooW... --message "Hello"

# Vote durumunu sorgula
threshold-voting-system query-vote --peer-id 12D3KooW... --tx-id tx_001

# P2P latency test
threshold-voting-system benchmark-p2p --peer-id 12D3KooW... --iterations 1000

# Çıktı:
# 📊 P2P Latency Benchmark
# ─────────────────────────────
#   Peer:      12D3KooW...
#   Iterations: 1000
#   Min:       1.2 ms
#   Avg:       3.5 ms
#   Max:       8.9 ms
#   P95:       6.2 ms
#   P99:       7.8 ms
```

Sistem SOTA (state-of-the-art) seviyesinde olacak! 🚀
