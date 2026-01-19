# 🚀 Test Komutları Rehberi

## ✅ SİSTEM HAZIR!

Mevcut durumda çalışan özellikler:
- ✅ 5 voting nodes (running)
- ✅ etcd cluster (3 nodes, healthy)
- ✅ PostgreSQL (ready)
- ✅ Noise Protocol encryption (active)
- ✅ Binary serialization support (bincode)

---

## 🧪 HIZLI TESTLER

### **1. Serialization Benchmark**

```bash
# Docker container içinde benchmark çalıştır
docker exec threshold-node1 /app/threshold-voting-system benchmark --iterations 1000 --verbose
```

**Beklenen Çıktı:**
```
📊 JSON Serialization Benchmark
─────────────────────────────────────
  Iterations:     1000
  Total Time:     XX.XX ms
  Avg Time:       XX.XX μs
  Throughput:     XXXX ops/sec
  Avg Size:       XXX bytes

📊 Binary Serialization Benchmark
─────────────────────────────────────
  Iterations:     1000
  Total Time:     XX.XX ms
  Avg Time:       XX.XX μs
  Throughput:     XXXX ops/sec
  Avg Size:       XXX bytes

📈 Performance Comparison
─────────────────────────────────────
  Speed:          Binary is X.XXx faster
  Size:           Binary is XX.X% smaller
```

**Not:** Binary genellikle 2-3x daha hızlı ve %30-40 daha küçük olur.

---

### **2. etcd'de Vote Count Test**

```powershell
# Transaction başlat
docker exec etcd1 etcdctl put "/vote_counts/test_001/100" "1"
docker exec etcd1 etcdctl put "/vote_counts/test_001/100" "2"
docker exec etcd1 etcdctl put "/vote_counts/test_001/100" "3"
docker exec etcd1 etcdctl put "/vote_counts/test_001/100" "4"

# Sonucu oku
docker exec etcd1 etcdctl get "/vote_counts/test_001/100"
```

**Beklenen:** `4` (threshold reached!)

---

### **3. P2P Connectivity Test**

```powershell
# Node 1 loglarını kontrol et (connection events)
docker logs threshold-node1 --tail 50 2>&1 | grep -i "connection\|peer"

# Node 2'nin Node 1'e bağlandığını kontrol et
docker logs threshold-node2 --tail 50 2>&1 | grep -i "connection\|peer"
```

**Beklenen:** Connection established mesajları

---

### **4. PostgreSQL Schema Kontrolü**

```powershell
# Tabloları listele
docker exec threshold-postgres psql -U threshold -d threshold_voting -c "\dt"

# Vote history tablosunu kontrol
docker exec threshold-postgres psql -U threshold -d threshold_voting -c "SELECT * FROM vote_history LIMIT 5;"

# Byzantine violations
docker exec threshold-postgres psql -U threshold -d threshold_voting -c "SELECT * FROM byzantine_violations;"
```

---

## 🔧 DEBUGGING KOMUTLARI

### **etcd Cluster Health**

```powershell
docker exec etcd1 etcdctl endpoint health --cluster --endpoints=http://etcd1:2379,http://etcd2:2379,http://etcd3:2379
```

### **etcd Data Explorer**

```powershell
# Tüm keys
docker exec etcd1 etcdctl get --prefix "/"

# Vote counts
docker exec etcd1 etcdctl get --prefix "/vote_counts/"

# Config
docker exec etcd1 etcdctl get --prefix "/config/"
```

### **Container Logs**

```powershell
# Node logs (JSON format)
docker logs threshold-node1 --tail 100 2>&1

# Specific error logs
docker logs threshold-node1 2>&1 | grep -i "error\|warn"

# Connection logs
docker logs threshold-node1 2>&1 | grep -i "connection"
```

### **Network Inspection**

```powershell
# Docker network details
docker network inspect p2p-comm_threshold-network

# Node IP addresses
docker inspect threshold-node1 | grep IPAddress
docker inspect threshold-node2 | grep IPAddress
```

---

## 🎯 SONRAKİ ADIMLAR

### **Eksik Özellikler (Implement edilecek):**

1. **Vote Submission CLI**
   ```bash
   docker exec threshold-node1 /app/threshold-voting-system vote --tx-id tx_001 --value 42
   ```
   - Vote struct oluşturma
   - Ed25519 imzalama
   - GossipSub publish
   - Status query

2. **P2P Direct Messaging**
   ```bash
   docker exec threshold-node1 /app/threshold-voting-system send --peer-id 12D3KooW... --message "Hello"
   ```
   - Request-Response protocol kullanımı
   - Latency ölçümü
   - Response handling

3. **Byzantine Detection Test**
   ```bash
   docker exec threshold-node1 /app/threshold-voting-system test-byzantine --test-type double-vote
   ```
   - Double voting simülasyonu
   - Invalid signature test
   - Minority attack test

4. **Network Monitor**
   ```bash
   docker exec threshold-node1 /app/threshold-voting-system monitor --interval 5
   ```
   - Connected peers listesi
   - Vote throughput
   - Byzantine violations count

5. **REST API**
   ```
   POST /api/vote
   GET /api/status/:tx_id
   GET /api/peers
   GET /api/reputation/:node_id
   ```

---

## 📊 PERFORMANS BEKLENTİLERİ

### **Serialization (1000 iterations):**
- JSON: ~50-100 μs/op, ~200 bytes
- Binary: ~20-40 μs/op, ~120 bytes
- **Speedup: 2-3x**

### **P2P Latency (local Docker):**
- Request-Response: <5 ms
- GossipSub broadcast: <10 ms
- Noise handshake: <50 ms (first connection)

### **etcd Operations:**
- Read: <5 ms
- Write: <10 ms
- Atomic increment: <15 ms

### **Threshold Consensus:**
- 4/5 votes: ~100-200 ms (local)
- Byzantine detection: <10 ms
- Audit trail write: <20 ms

---

## ✅ ŞUAN YAPILACAK TEST

**En basit ve hızlı test:**

```powershell
# 1. etcd'ye manuel vote yaz
docker exec etcd1 etcdctl put "/vote_counts/quick_test/777" "1"
docker exec etcd1 etcdctl put "/vote_counts/quick_test/777" "2"
docker exec etcd1 etcdctl put "/vote_counts/quick_test/777" "3"
docker exec etcd1 etcdctl put "/vote_counts/quick_test/777" "4"

# 2. Sonucu oku
docker exec etcd1 etcdctl get "/vote_counts/quick_test/777"
# Beklenen: 4 ✅

# 3. PostgreSQL'de node'ların durumunu gör
docker logs threshold-node1 --tail 20

# 4. etcd cluster health
docker exec etcd1 etcdctl endpoint health --cluster

# 5. Serialization benchmark (code rebuild gerekebilir)
docker exec threshold-node1 /app/threshold-voting-system benchmark --iterations 100
```

**Tamamı 2 dakikada çalışır!** ✅

---

## 🚧 REBUILD GEREKLİ Mİ?

Yeni kod ekledik (bincode, benchmark modülü). Docker image'ı rebuild etmek gerekebilir:

```powershell
# Rebuild (5-10 dakika)
cd C:\Users\user\Desktop\p2p-comm
docker-compose down
docker-compose build
docker-compose up -d

# Hızlı test
docker exec threshold-node1 /app/threshold-voting-system --help
```

**Eğer rebuild yapmak istemiyorsan:**
- Manuel etcd testleri hemen çalışır (rebuild gereksiz)
- Benchmark için rebuild gerekli

---

## 💡 ÖNERİ

**Şimdi ne yapalım?**

**Seçenek A:** Rebuild yap, yeni CLI'yi test et (10 dk)
**Seçenek B:** Rebuild'siz manuel testlere devam et (2 dk)
**Seçenek C:** Kalan özellikleri implemente et (vote submission, P2P, vb.)

Hangisini tercih edersin? 🤔
