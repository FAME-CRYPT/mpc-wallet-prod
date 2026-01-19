# Benchmark Karşılaştırması: p2p-comm vs mtls-comm

Bu dokümanda iki farklı Byzantine Fault Tolerant threshold voting sistemi karşılaştırılmaktadır:

1. **p2p-comm**: libp2p (GossipSub + Kademlia + Noise Protocol)
2. **mtls-comm**: mTLS (TLS 1.3 + X.509 certificates)

## 🎯 Test Edilen Metrikler

Her iki sistemde de aynı benchmarklar çalıştırılmıştır:

### 1. Serialization Performance
- **p2p-comm**: JSON vs Binary serileştirme
- **mtls-comm**: JSON serileştirme (sadelik için)
- Metrikler: throughput (ops/sec), latency (μs), size (bytes)

### 2. Cryptographic Operations
- Ed25519 key generation
- Ed25519 signature generation
- Ed25519 signature verification
- Metrikler: P50, P95, P99 latency

### 3. Vote Processing Throughput
- Concurrent vote processing (10 workers)
- Simulated signature verification
- Metrikler: ops/sec, avg latency

### 4. Connection Establishment
- **p2p-comm**: libp2p (TCP + Noise XX + multiplexer)
- **mtls-comm**: mTLS (TCP + TLS 1.3 + cert validation)
- Metrikler: connection setup time (P50, P95, P99)

### 5. Message Propagation
- **p2p-comm**: GossipSub broadcast (D=6 peers)
- **mtls-comm**: Direct mesh broadcast (5 peers)
- Metrikler: propagation latency

### 6. Byzantine Detection Overhead
- Double-vote check
- Signature verification
- Threshold validation
- Metrikler: per-vote overhead

### 7. Storage Performance
- etcd Compare-And-Swap operations
- PostgreSQL vote insertions
- Metrikler: latency (min/avg/max/P95/P99)

### 8. Certificate Validation (mTLS only)
- X.509 certificate parsing
- Signature chain verification
- Expiry & CRL checks
- Metrikler: validation time

### 9. End-to-End Latency
- Complete vote flow: create → sign → broadcast → validate → store
- Metrikler: total latency

---

## 📊 Beklenen Performans Farkları

### Connection Establishment

| Metric | p2p-comm (libp2p) | mtls-comm (mTLS) | Winner |
|--------|-------------------------|----------------------|---------|
| TCP Handshake | ~1ms | ~1ms | Tie |
| Encryption Handshake | ~7ms (Noise XX, 3-RTT) | ~4ms (TLS 1.3, 1-RTT) | ✅ mTLS |
| Protocol Negotiation | ~1ms (multiplexer) | ~2ms (cert validation) | libp2p |
| **Total** | **~9ms** | **~7ms** | ✅ **mTLS** |

**Avantaj**: mTLS, TLS 1.3'ün 1-RTT özelliği sayesinde daha hızlı bağlantı kurar.

### Message Propagation

| Metric | p2p-comm (GossipSub) | mtls-comm (Mesh) | Winner |
|--------|---------------------------|----------------------|---------|
| Broadcast Mechanism | Gossip (indirect) | Direct send | ✅ Mesh |
| Peer Count | D=6 (with gossip overhead) | 5 (direct connections) | Tie |
| Avg Latency | ~5ms | ~2ms | ✅ **Mesh** |

**Avantaj**: Mesh broadcast, direct TCP bağlantıları üzerinden çalıştığı için gossip overhead'i yok.

### Serialization

| Format | p2p-comm | mtls-comm | Winner |
|--------|---------------|---------------|---------|
| JSON | Supported | Supported | Tie |
| Binary (Bincode) | Supported | Not used | libp2p |
| JSON Size | ~450 bytes | ~450 bytes | Tie |
| Binary Size | ~320 bytes | N/A | libp2p |
| JSON Throughput | ~100k ops/sec | ~100k ops/sec | Tie |
| Binary Throughput | ~250k ops/sec | N/A | libp2p |

**Avantaj**: libp2p, binary serialization ile 2.5x daha yüksek throughput sağlar. Ancak mTLS'te JSON kullanımı debugging için daha kolay.

### Byzantine Detection

| Metric | p2p-comm | mtls-comm | Winner |
|--------|---------------|---------------|---------|
| Signature Verification | ~50μs | ~50μs | Tie |
| Double-vote Check | ~1μs | ~1μs | Tie |
| Threshold Check | ~1μs | ~1μs | Tie |
| **Total** | **~52μs** | **~52μs** | **Tie** |

**Not**: Byzantine detection logic her iki sistemde de tamamen aynı (etcd + PostgreSQL + Ed25519).

### End-to-End Latency

| Phase | p2p-comm | mtls-comm | Fark |
|-------|---------------|---------------|------|
| Vote Creation | ~50μs | ~50μs | - |
| Serialization | ~10μs (binary) | ~15μs (JSON) | +5μs |
| Broadcast | ~5ms | ~2ms | -3ms |
| Byzantine Check | ~52μs | ~52μs | - |
| Storage | ~5ms | ~5ms | - |
| **Total** | **~10.1ms** | **~7.1ms** | **-3ms** ✅ |

**Sonuç**: mTLS sistemi, **~30% daha düşük end-to-end latency** sağlar (mesh broadcast sayesinde).

---

## 🔐 Güvenlik Karşılaştırması

### Authentication

| Feature | p2p-comm | mtls-comm | Winner |
|---------|---------------|---------------|---------|
| Encryption | Noise Protocol XX | TLS 1.3 | Industry standard: mTLS |
| Mutual Auth | ✅ Built-in | ✅ Built-in | Tie |
| Forward Secrecy | ✅ Yes | ✅ Yes | Tie |
| Formal Verification | ✅ ProVerif proven | ⚠️ Partially (TLS 1.3) | libp2p |
| Certificate Mgmt | ❌ Not needed | ✅ PKI required | Depends |

### Infrastructure

| Feature | p2p-comm | mtls-comm | Winner |
|---------|---------------|---------------|---------|
| Audit Tools | ❌ Limited | ✅ Extensive (Wireshark, openssl) | ✅ mTLS |
| Compliance | ❌ Rare | ✅ PCI DSS, HIPAA, SOC2 | ✅ mTLS |
| Key Distribution | ✅ Peer-based | ⚠️ CA-based | Depends |
| Revocation | ❌ Manual | ✅ CRL/OCSP | ✅ mTLS |

---

## 📈 Ölçeklenebilirlik

### Peer Discovery

| Feature | p2p-comm | mtls-comm | Winner |
|---------|---------------|---------------|---------|
| Discovery | Kademlia DHT | Static bootstrap | libp2p |
| Dynamic Join | ✅ Automatic | ⚠️ Manual config | libp2p |
| Network Size | 1000s of nodes | 10-50 nodes | libp2p |

**Not**: libp2p, büyük P2P ağları için tasarlanmıştır. mTLS, küçük-orta ölçekli enterprise sistemler için idealdir.

### Storage Performance

| Metric | Both Systems | Notes |
|--------|--------------|-------|
| etcd CAS | ~3ms | Raft consensus overhead |
| PostgreSQL Insert | ~2ms | Single-region latency |
| Byzantine Audit | ✅ Identical | Same storage layer |

**Tie**: Her iki sistem de aynı storage backend'ini kullanır.

---

## 🏆 Özet Karşılaştırma Tablosu

| Kriter | p2p-comm (libp2p) | mtls-comm (mTLS) | Kazanan |
|--------|-------------------------|----------------------|---------|
| **Connection Setup** | ~9ms | ~7ms | ✅ mTLS |
| **Message Propagation** | ~5ms | ~2ms | ✅ mTLS |
| **Serialization Throughput** | 250k ops/sec (binary) | 100k ops/sec (JSON) | ✅ libp2p |
| **Byzantine Detection** | ~52μs | ~52μs | Tie |
| **End-to-End Latency** | ~10.1ms | ~7.1ms | ✅ mTLS |
| **Peer Discovery** | Automatic (DHT) | Manual (config) | ✅ libp2p |
| **Audit & Compliance** | Limited | Extensive | ✅ mTLS |
| **Network Size** | 1000+ nodes | 10-50 nodes | ✅ libp2p |
| **Security Verification** | Formal (ProVerif) | Industry standard | ✅ libp2p |
| **Operational Complexity** | Medium | Low (standard tools) | ✅ mTLS |

---

## 🎯 Kullanım Senaryoları

### p2p-comm (libp2p) Daha İyi

1. **Büyük P2P ağları** (100+ node)
2. **Dynamic node membership** (nodes frequently join/leave)
3. **Public/permissionless networks**
4. **Formal verification requirement**
5. **Maximum throughput** (binary serialization)

### mtls-comm (mTLS) Daha İyi

1. **Enterprise/private networks** (5-50 nodes)
2. **Compliance requirements** (PCI DSS, HIPAA)
3. **Standard audit tools** (Wireshark, openssl)
4. **Lower latency** (mesh broadcast)
5. **Easier operations** (standard TLS tooling)

---

## 📋 Benchmark Çalıştırma Talimatları

### p2p-comm

```bash
cd p2p-comm

# Tek benchmark (serileştirme)
cargo run --release -- benchmark --iterations 10000 --verbose

# Tüm benchmarklar
cargo run --release -- benchmark-all --iterations 10000 --verbose
```

**Çıktı**:
- 1. Serialization (JSON vs Binary)
- 2. Cryptography (Ed25519)
- 3. Vote Throughput
- 4. libp2p Connection
- 5. GossipSub Propagation
- 6. Byzantine Detection
- 7. Storage (etcd + PostgreSQL)
- 8. End-to-End Latency

### mtls-comm

```bash
cd mtls-comm

# Tek benchmark (serileştirme)
cargo run --release -- benchmark --iterations 10000 --verbose

# Tüm benchmarklar
cargo run --release -- benchmark-all --iterations 10000 --verbose
```

**Çıktı**:
- 1. Serialization (JSON)
- 2. Cryptography (Ed25519)
- 3. Vote Throughput
- 4. mTLS Connection
- 5. Mesh Broadcast
- 6. Byzantine Detection
- 7. Storage (etcd + PostgreSQL)
- 8. Certificate Validation (X.509)
- 9. End-to-End Latency

---

## 📊 Benchmark Çıktı Formatı

Her benchmark şu metrikleri raporlar:

```
📊 [Benchmark Name]
─────────────────────────────────────
  Iterations:     1000
  Min:            X.XXX ms
  Max:            X.XXX ms
  Average:        X.XXX ms
  P50 (median):   X.XXX ms
  P95:            X.XXX ms
  P99:            X.XXX ms
```

---

## 🔬 Simüle Edilen vs Gerçek Değerler

**Önemli Not**: Bu benchmarklar simülasyon içerir:

| Benchmark | Simüle mi? | Neden? |
|-----------|-----------|--------|
| Serialization | ❌ Real | Gerçek serde_json/bincode kullanılır |
| Cryptography | ❌ Real | Gerçek Ed25519 kullanılır |
| Connection | ✅ Simulated | Gerçek network gerektirir |
| Propagation | ✅ Simulated | Çalışan node cluster gerektirir |
| Byzantine | ✅ Simulated | Gerçek consensus gerektirir |
| Storage | ✅ Simulated | Çalışan etcd+PostgreSQL gerektirir |

**Gerçek değerler için**:
1. Docker Compose ile tam cluster çalıştırın:
   ```bash
   docker-compose up -d
   ```
2. Benchmarkları cluster'a karşı çalıştırın

---

## 🚀 Sonraki Adımlar

### Performance Optimization

1. **p2p-comm için**:
   - Binary serialization kullanımını artır
   - GossipSub parameters optimizasyonu (D, D_low, D_high)
   - Kademlia routing table size tuning

2. **mtls-comm için**:
   - Session resumption kullan (TLS 1.3)
   - Certificate caching
   - Connection pooling

### Real-World Testing

1. Gerçek 5-node cluster deployment
2. Network latency injection (tc/netem)
3. Byzantine fault injection tests
4. Long-running stability tests (24h+)

---

## 📝 Notlar

- Her iki sistem de aynı **Byzantine consensus logic** kullanır
- Storage layer (etcd + PostgreSQL) tamamen aynı
- Ed25519 cryptography identical
- Fark yalnızca **networking layer**'dadır

**Karar Faktörleri**:
- Network size → libp2p
- Compliance → mTLS
- Latency → mTLS
- Throughput → libp2p
- Formal verification → libp2p
- Operational simplicity → mTLS

---

## 📚 Referanslar

- [libp2p Specifications](https://github.com/libp2p/specs)
- [TLS 1.3 RFC 8446](https://www.rfc-editor.org/rfc/rfc8446)
- [Noise Protocol Framework](https://noiseprotocol.org/)
- [GossipSub Specification](https://github.com/libp2p/specs/tree/master/pubsub/gossipsub)
- [rustls Documentation](https://docs.rs/rustls/)
- [etcd Performance](https://etcd.io/docs/latest/op-guide/performance/)

---

**Son Güncelleme**: 2026-01-19
**Benchmark Versiyonu**: v1.0
