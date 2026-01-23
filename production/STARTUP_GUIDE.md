# MPC WALLET - SİSTEM BAŞLATMA REHBERİ

## Ön Hazırlık Kontrolü

### 1. Gerekli Araçlar
```bash
# Docker ve Docker Compose versiyonları
docker --version          # >= 20.10.0
docker-compose --version  # >= 1.29.0 veya docker compose v2

# Rust toolchain (local build için)
rustc --version           # >= 1.75.0
cargo --version
```

### 2. Dosya Kontrolü
```bash
cd /c/Users/user/Desktop/MPC-WALLET/production/docker

# Gerekli dosyalar
ls -la .env                           # ✅ Var
ls -la docker-compose.yml             # ✅ Var
ls -la Dockerfile.node                # ✅ Var
ls -la ../certs/ca.crt                # ✅ Var
ls -la init-db/01_schema.sql          # ✅ Var
```

## Adım 1: Docker Ortamını Temizle (İlk Kez İçin)

```bash
cd /c/Users/user/Desktop/MPC-WALLET/production/docker

# Eski container'ları temizle (varsa)
docker-compose down -v

# Docker sistem temizliği (opsiyonel)
docker system prune -f
```

## Adım 2: Infrastructure Servislerini Başlat (etcd + PostgreSQL)

```bash
# Sadece infrastructure servislerini başlat
docker-compose up -d etcd-1 etcd-2 etcd-3 postgres

# Servislerin hazır olmasını bekle (30 saniye)
sleep 30

# Sağlık kontrolü
docker-compose ps
docker exec mpc-etcd-1 etcdctl endpoint health --cluster
docker exec mpc-postgres pg_isready -U mpc -d mpc_wallet
```

**Beklenen Çıktı:**
```
etcd-1: healthy
etcd-2: healthy
etcd-3: healthy
postgres: accepting connections
```

## Adım 3: Rust Projelerini Build Et

**ÖNEMLİ:** Docker içinde build olacak, ama önce syntax hatalarını kontrol edelim:

```bash
cd /c/Users/user/Desktop/MPC-WALLET/production

# Workspace'i kontrol et
cargo check --workspace

# Eğer hata varsa düzelt, yoksa devam et
```

## Adım 4: Docker Image'ları Build Et

```bash
cd /c/Users/user/Desktop/MPC-WALLET/production/docker

# Node image'ını build et (5-10 dakika sürebilir)
docker-compose build node-1

# İlk node build olduktan sonra diğerleri aynı layer'ları kullanır
docker-compose build
```

**Beklenen Çıktı:**
```
Successfully built <image-id>
Successfully tagged mpc-wallet_node-1:latest
...
```

## Adım 5: MPC Node'ları Başlat

```bash
# Tüm node'ları başlat
docker-compose up -d node-1 node-2 node-3 node-4 node-5

# Logları izle (ayrı bir terminal'de)
docker-compose logs -f node-1
```

**Başarılı Başlatma İşaretleri:**
```
✅ "Server listening on 0.0.0.0:8080"
✅ "QUIC transport initialized on 0.0.0.0:9000"
✅ "Connected to etcd cluster"
✅ "PostgreSQL connection pool initialized"
✅ "Peer discovery started"
```

## Adım 6: Cluster Sağlık Kontrolü

```bash
# Makefile ile otomatik kontrol
make health

# Veya manuel kontrol
docker-compose ps

# Her node'un API'sini test et
for i in {1..5}; do
  echo "Testing node $i..."
  curl http://localhost:808$i/health
done
```

**Beklenen Çıktı (Her Node İçin):**
```json
{
  "status": "healthy",
  "node_id": "1",
  "peers_connected": 4,
  "etcd_connected": true,
  "postgres_connected": true
}
```

## Adım 7: Cluster Status Kontrolü

```bash
# Node 1'den cluster durumunu sorgula
curl http://localhost:8081/api/v1/cluster/status

# Beklenilen çıktı:
{
  "cluster_size": 5,
  "threshold": 4,
  "healthy_nodes": 5,
  "nodes": [
    {"id": "1", "status": "healthy", "last_seen": "2026-01-23T..."},
    {"id": "2", "status": "healthy", "last_seen": "2026-01-23T..."},
    ...
  ]
}
```

## Problemler ve Çözümleri

### Problem 1: Rust Build Hatası

```bash
# Hata: "failed to compile ..."
# Çözüm: Local'de kontrol et
cd /c/Users/user/Desktop/MPC-WALLET/production
cargo check --workspace
cargo build --workspace --all-features

# Hataları düzelt ve tekrar build et
```

### Problem 2: etcd Cluster Oluşmuyor

```bash
# etcd loglarını kontrol et
docker logs mpc-etcd-1

# etcd cluster'ı sıfırla
docker-compose down -v
docker volume rm mpc-wallet_etcd-1-data mpc-wallet_etcd-2-data mpc-wallet_etcd-3-data
docker-compose up -d etcd-1 etcd-2 etcd-3
```

### Problem 3: PostgreSQL Bağlantı Hatası

```bash
# PostgreSQL loglarını kontrol et
docker logs mpc-postgres

# Şifre hatası varsa .env'i kontrol et
cat .env | grep POSTGRES_PASSWORD

# Database'i manuel kontrol et
docker exec -it mpc-postgres psql -U mpc -d mpc_wallet
\dt  # Tabloları listele
\q   # Çık
```

### Problem 4: Node Birbirini Bulamıyor

```bash
# Network kontrolü
docker network ls
docker network inspect mpc-wallet_mpc-internal

# QUIC portlarını kontrol et
docker-compose ps

# Firewall kontrolü (Windows)
# Windows Defender Firewall'da Docker için izin ver
```

### Problem 5: Certificate Hatası

```bash
# Sertifikaları kontrol et
cd /c/Users/user/Desktop/MPC-WALLET/production/certs
openssl x509 -in ca.crt -text -noout
openssl x509 -in node1.crt -text -noout

# Sertifikaları yeniden oluştur
cd ../scripts
./generate-certs.sh
```

## Component Bazlı Test Senaryoları

### Test 1: etcd Cluster

```bash
# etcd'ye veri yaz
docker exec mpc-etcd-1 etcdctl put /test/key1 "test-value"

# etcd'den veri oku (başka node'dan)
docker exec mpc-etcd-2 etcdctl get /test/key1

# Beklenen: test-value
```

### Test 2: PostgreSQL

```bash
# Test tablosu oluştur
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  CREATE TABLE IF NOT EXISTS test (
    id SERIAL PRIMARY KEY,
    data TEXT
  );
"

# Veri ekle
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  INSERT INTO test (data) VALUES ('test-data');
"

# Veri sorgula
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT * FROM test;
"
```

### Test 3: Node API Endpoints

```bash
# Health endpoint
curl http://localhost:8081/health

# Cluster status
curl http://localhost:8081/api/v1/cluster/status

# Node info
curl http://localhost:8081/api/v1/node/info
```

### Test 4: QUIC P2P Communication

```bash
# Node loglarını kontrol et
docker logs mpc-node-1 | grep "peer"
docker logs mpc-node-2 | grep "peer"

# Beklenen: "Connected to peer" mesajları
```

### Test 5: DKG (Distributed Key Generation)

```bash
# DKG protokolünü başlat (Node 1'den)
curl -X POST http://localhost:8081/api/v1/cluster/dkg/start \
  -H "Content-Type: application/json" \
  -d '{
    "threshold": 4,
    "participants": [1, 2, 3, 4, 5]
  }'

# DKG durumunu kontrol et
curl http://localhost:8081/api/v1/cluster/dkg/status

# Logları izle
docker-compose logs -f node-1 node-2 node-3 node-4 node-5 | grep DKG
```

### Test 6: Cüzdan Oluşturma

```bash
# Bitcoin testnet cüzdanı oluştur
curl -X POST http://localhost:8081/api/v1/wallet/create \
  -H "Content-Type: application/json" \
  -d '{
    "network": "testnet",
    "derivation_path": "m/84h/1h/0h"
  }'

# Beklenen çıktı:
{
  "wallet_id": "...",
  "address": "tb1q...",
  "public_key": "...",
  "created_at": "2026-01-23T..."
}
```

### Test 7: Transaction Signing

```bash
# Ham Bitcoin transaction oluştur (örnek)
curl -X POST http://localhost:8081/api/v1/tx/sign \
  -H "Content-Type: application/json" \
  -d '{
    "wallet_id": "...",
    "tx_hex": "0200000001...",
    "input_index": 0
  }'

# TSS signing ceremony loglarını izle
docker-compose logs -f node-1 node-2 node-3 node-4 | grep "TSS-Sign"
```

## Monitoring ve Debugging

### Real-time Logs

```bash
# Tüm servislerin logları
docker-compose logs -f

# Sadece node'lar
docker-compose logs -f node-1 node-2 node-3 node-4 node-5

# Sadece infrastructure
docker-compose logs -f etcd-1 postgres

# Belirli bir node
docker-compose logs -f node-1
```

### Container İçine Giriş

```bash
# Node container'ına gir
docker exec -it mpc-node-1 /bin/sh

# PostgreSQL shell
docker exec -it mpc-postgres psql -U mpc -d mpc_wallet

# etcd shell
docker exec -it mpc-etcd-1 /bin/sh
```

### Resource Monitoring

```bash
# Container resource kullanımı
docker stats

# Disk kullanımı
docker system df

# Volume'ları listele
docker volume ls
```

## Sistemin Durdurulması

### Graceful Shutdown

```bash
cd /c/Users/user/Desktop/MPC-WALLET/production/docker

# Tüm servisleri durdur (data'yı koru)
docker-compose down

# Restart (data korunur)
docker-compose up -d
```

### Hard Reset (Tüm Data Silinir)

```bash
# TÜM DATA SİLİNİR - DİKKAT!
docker-compose down -v

# Yeniden başlat
docker-compose up -d
```

## Next Steps - Component Test Planı

Sistem ayağa kalktıktan sonra aşağıdaki sırayla test edelim:

1. ✅ Infrastructure (etcd + PostgreSQL)
2. ✅ Network & P2P (QUIC communication)
3. ✅ Consensus (Byzantine detection)
4. ✅ Storage (Database operations)
5. ✅ Security (TLS/mTLS)
6. ✅ Protocols (DKG, TSS-Sign)
7. ✅ Bitcoin (Wallet creation, TX signing)
8. ✅ Orchestrator (Workflow coordination)
9. ✅ API (REST endpoints)
10. ✅ CLI (Command-line interface)

Her component için detaylı test senaryoları ayrı dokümanda hazırlanacak.
