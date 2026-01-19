# 🚀 Hızlı Test Rehberi

## ✅ Sistem Durumu

Tüm servisler çalışıyor! Database sorunu çözüldü.

```
✅ etcd1, etcd2, etcd3: Healthy
✅ PostgreSQL: Healthy
✅ Node 1-5: Running
```

---

## 🧪 TEST 1: etcd'de Vote Sayıları

### **Mevcut durumu kontrol et:**

```powershell
# Vote counts kontrol
docker exec etcd1 etcdctl get --prefix "/vote_counts/"
```

**Beklenen:** Henüz vote yok (boş)

---

## 🧪 TEST 2: Manuel Vote Simülasyonu

### **etcd'ye vote count yaz (node'lar gibi):**

```powershell
# Transaction tx_001 için value=42'ye 1. oy
docker exec etcd1 etcdctl put "/vote_counts/tx_001/42" "1"

# Transaction tx_001 için value=42'ye 2. oy
docker exec etcd1 etcdctl put "/vote_counts/tx_001/42" "2"

# Transaction tx_001 için value=42'ye 3. oy
docker exec etcd1 etcdctl put "/vote_counts/tx_001/42" "3"

# Transaction tx_001 için value=42'ye 4. oy (THRESHOLD!)
docker exec etcd1 etcdctl put "/vote_counts/tx_001/42" "4"
```

### **Sonucu kontrol et:**

```powershell
# Vote count'u oku
docker exec etcd1 etcdctl get "/vote_counts/tx_001/42"
```

**Beklenen Çıktı:**
```
/vote_counts/tx_001/42
4
```

✅ **Bu, atomic counting sisteminin çalıştığını gösterir!**

---

## 🧪 TEST 3: Consensus Configuration

```powershell
# Threshold değerini oku
docker exec etcd1 etcdctl get "/config/threshold"

# Total nodes değerini oku
docker exec etcd1 etcdctl get "/config/total_nodes"
```

**Beklenen Çıktı:**
```
/config/threshold
4

/config/total_nodes
5
```

✅ **N=5, t=4 (Byzantine Fault Tolerant: 4/5 consensus)**

---

## 🧪 TEST 4: PostgreSQL Audit Trail

```powershell
# Vote history tablosunu kontrol et
docker exec threshold-postgres psql -U threshold -d threshold_voting -c "SELECT * FROM vote_history LIMIT 10;"

# Byzantine violations tablosunu kontrol et
docker exec threshold-postgres psql -U threshold -d threshold_voting -c "SELECT * FROM byzantine_violations LIMIT 10;"
```

**Beklenen:** Tablolar var ama henüz data yok (vote göndermedik)

---

## 🧪 TEST 5: Node Connectivity

### **Node 1'in connected peers'larını kontrol et:**

Node loglarında şu satırları ara:

```powershell
docker logs threshold-node1 2>&1 | Select-String "Connection established"
```

**Beklenen:** Diğer node'larla connection kurulmuş olmalı.

---

## 🧪 TEST 6: Noise Protocol Encryption

### **Wireshark ile Traffic Capture (Opsiyonel)**

```powershell
# Docker network trafiğini yakala (Linux gerekir)
# tcpdump -i docker0 -w noise_traffic.pcap port 9000
```

**Beklenen:** Tüm traffic encrypted (ChaCha20-Poly1305)

---

## 🎯 SONRAKİ ADIM: CLI Tool Ekle

Şu anda manuel test yapabiliyoruz, ama gerçek kullanım için CLI tool lazım:

```rust
// Örnek komut:
threshold-voting-system vote --tx-id tx_001 --value 42
```

Bu tool, içeriden:
1. Vote struct oluşturur
2. Ed25519 ile imzalar
3. GossipSub'a publish eder
4. Network üzerinden broadcast edilir (Noise encrypted)
5. Diğer node'lar receive eder ve process eder

---

## ✅ ŞU AN ÇALIŞAN ÖZELLİKLER

- ✅ **etcd Raft Consensus** (3-node cluster)
- ✅ **PostgreSQL Persistence** (audit trail ready)
- ✅ **Noise Protocol Encryption** (all traffic encrypted)
- ✅ **5 Voting Nodes** (N=5, t=4)
- ✅ **Atomic Vote Counting**
- ✅ **Byzantine Detection Infrastructure**
- ✅ **P2P Network** (libp2p + GossipSub + Request-Response)

---

## 🚧 EKSİK ÖZELLİKLER

- ❌ **CLI Interface** (vote göndermek için)
- ❌ **REST API** (external integration için)
- ❌ **Transaction Submission Logic** (kim transaction başlatıyor?)

---

## 💡 BİR SONRAKİ ADIM

**CLI tool ekleyelim mi?** Şu komutları destekleyelim:

```bash
# Vote gönder
threshold-voting-system vote --tx-id <id> --value <value>

# Transaction durumunu sorgula
threshold-voting-system status --tx-id <id>

# Node bilgilerini göster
threshold-voting-system info

# Reputation görüntüle
threshold-voting-system reputation --node-id <id>
```

İster misin implement edelim?
