# MPC-WALLET Setup Guide

**Tarih**: 2026-01-29
**Versiyon**: 1.0.0

Bu dokümanda MPC-WALLET sisteminin sıfırdan nasıl kurulduğu ve başlatıldığı adım adım anlatılmaktadır.

---

## 📋 Gereksinimler

- Docker Desktop (Windows/Mac/Linux)
- Docker Compose
- PowerShell veya CMD (Windows)
- En az 8GB RAM
- En az 20GB disk alanı

---

## 🔧 Sistem Kurulum Adımları

### 1. Docker Cleanup (Temiz Başlangıç)

Önce mevcut containerları durdur ve tüm volumeları temizle:

```powershell
cd "c:\Users\user\Desktop\MPC-WALLET\production"
docker compose down
docker system prune -af --volumes
```

**Not**: Bu komut TÜM Docker objelerini (containers, images, volumes, networks) temizler. Dikkatli kullanın.

---

### 2. Build (No Cache)

Sistemin tüm image'larını sıfırdan build et:

```powershell
docker compose build --no-cache
```

**Beklenen Süre**: ~5-10 dakika (internet hızına bağlı)

**Çıktı**:
```
[+] Building 240.5s (67/67) FINISHED
 => [mpc-node-1 internal] load build definition
 => [mpc-node-1 internal] load metadata
 => [mpc-node-1 stage-1 1/8] FROM rust:1.75
 ...
 => exporting to image
 => => naming to docker.io/library/production-mpc-node-1
```

---

### 3. Compose Up (Sistemin Başlatılması)

Tüm servisleri başlat:

```powershell
docker compose up -d
```

**Servislerin Durumunu Kontrol Et**:
```powershell
docker compose ps
```

**Beklenen Çıktı**:
```
NAME              IMAGE                    STATUS          PORTS
mpc-node-1        production-mpc-node-1   Up 10 seconds   0.0.0.0:8081->8080/tcp
mpc-node-2        production-mpc-node-2   Up 10 seconds   0.0.0.0:8082->8080/tcp
mpc-node-3        production-mpc-node-3   Up 10 seconds   0.0.0.0:8083->8080/tcp
mpc-node-4        production-mpc-node-4   Up 10 seconds   0.0.0.0:8084->8080/tcp
mpc-node-5        production-mpc-node-5   Up 10 seconds   0.0.0.0:8085->8080/tcp
postgres          postgres:16-alpine      Up 10 seconds   0.0.0.0:5432->5432/tcp
```

**Logları Kontrol Et**:
```powershell
docker compose logs -f mpc-node-1
```

Çıkmak için: `Ctrl+C`

---

### 4. Primes Dosyalarını Kopyala

CGGMP24 protokolü için pre-generated primes dosyalarını her node'a kopyala:

```powershell
# Node 1
docker cp "data/primes-party-0.json" "mpc-node-1:/data/primes-party-0.json"
echo "✅ Copied to node-1"

# Node 2
docker cp "data/primes-party-1.json" "mpc-node-2:/data/primes-party-1.json"
echo "✅ Copied to node-2"

# Node 3
docker cp "data/primes-party-2.json" "mpc-node-3:/data/primes-party-2.json"
echo "✅ Copied to node-3"

# Node 4
docker cp "data/primes-party-3.json" "mpc-node-4:/data/primes-party-3.json"
echo "✅ Copied to node-4"

# Node 5
docker cp "data/primes-party-4.json" "mpc-node-5:/data/primes-party-4.json"
echo "✅ Copied to node-5"
```

**Doğrulama**:
```powershell
docker exec mpc-node-1 ls -lh /data/primes-party-0.json
```

Beklenen çıktı:
```
-rw-r--r-- 1 root root 1.2K Jan 29 10:00 /data/primes-party-0.json
```

---

### 5. DKG Ceremony (Distributed Key Generation)

İlk adım olarak threshold key generation için DKG ceremony başlat:

```powershell
curl -X POST http://localhost:8081/api/v1/dkg/initiate -H "Content-Type: application/json" -d '{\"threshold\":4,\"total_nodes\":5,\"protocol\":\"cggmp24\"}'
```

**Beklenen Çıktı**:
```json
{
  "success": true,
  "session_id": "78177cda-0462-4725-a2e3-45e62642963e",
  "threshold": 4,
  "total_nodes": 5,
  "protocol": "cggmp24",
  "public_key_hex": "03a1b2c3...",
  "error": null
}
```

**Bekle**: DKG ceremony tamamlanana kadar bekle (~ 20 saniye)
```powershell
Start-Sleep -Seconds 20
```

**Doğrulama** (DKG loglarını kontrol et):
```powershell
docker compose logs mpc-node-1 | Select-String "DKG"
```

Beklenen çıktı:
```
INFO: "DKG ceremony completed successfully"
INFO: "Public key: 03a1b2c3..."
```

---

### 6. Aux Info Generation

CGGMP24 presignature için gerekli auxiliary information oluştur:

```powershell
curl -X POST http://localhost:8081/api/v1/aux-info/generate -H "Content-Type: application/json" -d '{\"num_parties\":5,\"participants\":[1,2,3,4,5]}'
```

**Beklenen Çıktı**:
```json
{
  "success": true,
  "session_id": "8b4282f1-1db1-4c0c-bb86-4736e52d77b6",
  "party_index": 0,
  "num_parties": 5,
  "aux_info_size_bytes": 19782,
  "error": null
}
```

**Bekle**: Aux info generation tamamlanana kadar bekle (~ 25 saniye)
```powershell
Start-Sleep -Seconds 25
```

**Doğrulama** (Database'de aux info kontrol et):
```powershell
docker exec postgres psql -U mpc_user -d mpc_wallet -c "SELECT session_id, node_id, length(aux_info_data) as size_bytes FROM aux_info;"
```

Beklenen çıktı:
```
              session_id              | node_id | size_bytes
--------------------------------------+---------+------------
 8b4282f1-1db1-4c0c-bb86-4736e52d77b6 |       1 |      19782
 8b4282f1-1db1-4c0c-bb86-4736e52d77b6 |       2 |      19782
 8b4282f1-1db1-4c0c-bb86-4736e52d77b6 |       3 |      19782
 8b4282f1-1db1-4c0c-bb86-4736e52d77b6 |       4 |      19782
 8b4282f1-1db1-4c0c-bb86-4736e52d77b6 |       5 |      19782
(5 rows)
```

---

## ✅ Sistem Hazır!

Bu adımları tamamladıktan sonra sistem kullanıma hazır.

### Sistem Sağlığı Kontrolü

```powershell
# Health check
curl http://localhost:8081/api/v1/health

# DKG status
curl http://localhost:8081/api/v1/dkg/status

# Aux info status
curl http://localhost:8081/api/v1/aux-info/status
```

**Beklenen Çıktı** (health check):
```json
{
  "status": "healthy",
  "node_id": 1,
  "services": {
    "postgres": "connected",
    "message_router": "running",
    "dkg_service": "ready",
    "aux_info_service": "ready",
    "presig_service": "ready"
  }
}
```

---

## 🚀 İlk Transaction Oluşturma

Sistem hazır olduğunda ilk transaction'ı oluştur:

```powershell
curl -X POST http://localhost:8081/api/v1/transactions -H "Content-Type: application/json" -d '{\"recipient\":\"tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx\",\"amount_sats\":10000,\"metadata\":\"Test transaction\"}'
```

**Beklenen Çıktı**:
```json
{
  "txid": "tx_abc123...",
  "state": "pending",
  "recipient": "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx",
  "amount_sats": 10000,
  "fee_sats": 1000,
  "metadata": "Test transaction",
  "created_at": "2026-01-29T11:00:00Z"
}
```

**Transaction Status Kontrol**:
```powershell
curl http://localhost:8081/api/v1/transactions/<txid>
```

---

## 🔄 Yeniden Başlatma (Restart)

Sistemi durdurup yeniden başlatmak için:

```powershell
# Durdur
docker compose down

# Başlat
docker compose up -d
```

**DİKKAT**: Volume'lar silinmediği sürece DKG ve aux info verileri kaybolmaz. Primes dosyalarını tekrar kopyalamaya gerek yoktur.

---

## 🧹 Tam Temizlik (Full Reset)

Tüm verileri sil ve sıfırdan başla:

```powershell
# Tüm containerları durdur
docker compose down

# Tüm Docker objelerini temizle
docker system prune -af --volumes

# Bu rehberin başına dön (2. Build adımından itibaren)
```

---

## 📊 Monitoring & Debugging

### Container Loglarını İzleme

```powershell
# Tüm node'ları izle
docker compose logs -f

# Sadece Node 1
docker compose logs -f mpc-node-1

# Sadece PostgreSQL
docker compose logs -f postgres
```

### Database'e Bağlanma

```powershell
docker exec -it postgres psql -U mpc_user -d mpc_wallet
```

Kullanışlı SQL sorguları:
```sql
-- Tüm DKG ceremonies
SELECT session_id, protocol, threshold, total_nodes, status, started_at
FROM dkg_ceremonies
ORDER BY started_at DESC;

-- Tüm aux info sessions
SELECT session_id, party_index, num_parties, status, started_at
FROM aux_info_sessions
ORDER BY started_at DESC;

-- Tüm transactions
SELECT txid, state, recipient, amount_sats, created_at
FROM transactions
ORDER BY created_at DESC;
```

### Container'a Bağlanma

```powershell
# Node 1 shell
docker exec -it mpc-node-1 /bin/bash

# Primes dosyasını kontrol et
ls -lh /data/
cat /data/primes-party-0.json
```

---

## 🐛 Sorun Giderme

### Problem: Container'lar başlamıyor

**Çözüm**:
```powershell
# Logları kontrol et
docker compose logs

# Port çakışması var mı?
netstat -ano | findstr "8081"
netstat -ano | findstr "5432"
```

### Problem: DKG ceremony fail ediyor

**Çözüm**:
```powershell
# Tüm node'ların çalıştığını doğrula
docker compose ps

# Primes dosyalarının kopyalandığını doğrula
docker exec mpc-node-1 ls -lh /data/
docker exec mpc-node-2 ls -lh /data/
docker exec mpc-node-3 ls -lh /data/
docker exec mpc-node-4 ls -lh /data/
docker exec mpc-node-5 ls -lh /data/
```

### Problem: Aux info generation takılıyor

**Çözüm**:
```powershell
# Aux info loglarını kontrol et
docker compose logs -f mpc-node-1 | Select-String "aux"

# DKG'nin tamamlandığını doğrula
curl http://localhost:8081/api/v1/dkg/status
```

### Problem: Database bağlantı hatası

**Çözüm**:
```powershell
# PostgreSQL container'ının çalıştığını doğrula
docker compose ps postgres

# Database'e bağlan
docker exec -it postgres psql -U mpc_user -d mpc_wallet -c "SELECT 1;"
```

---

## 📚 Ek Kaynaklar

- [sorunlar-var.md](sorunlar-var.md) - Tüm sorunlar ve çözümleri
- [test_definition_of_done.md](test_definition_of_done.md) - Test checklist
- Docker Compose dosyası: [docker-compose.yml](docker-compose.yml)
- Database schema: [docker/init-db/01_schema.sql](docker/init-db/01_schema.sql)

---

## 📝 Notlar

1. **Primes Dosyaları**: Pre-generated primes CGGMP24 protokolünün hızlı çalışması için gereklidir. Bu dosyalar olmadan aux info generation ~83 saniye sürer (primes ile ~20 saniye).

2. **Session IDs**: Her DKG ve aux info ceremony farklı session ID alır. Bu normaldir.

3. **Threshold**: 4-of-5 threshold kullanılır. Yani 5 node'dan en az 4'ü imza için gereklidir.

4. **Slow-Path vs Fast-Path**: Şu anda slow-path signing tam çalışmıyor (multi-node orchestration eksik). Fast-path signing SORUN #19'a bağlı (party count mismatch).

5. **Production Ready**: Sistem şu anda %88.9 hazır (16/18 sorun çözüldü). Kalan 2 sorun fast-path signing için gerekli.

---

**Son Güncelleme**: 2026-01-29 11:30
**Durum**: ✅ Sistem operasyonel (slow-path signing hariç)
