# Quick Start Guide - Windows

## Windows'ta Kurulum

### Seçenek 1: Docker ile Kullan (EN KOLAY - ÖNERİLEN) ✅

Docker kullanırsan **hiçbir şey kurman gerekmiyor**! Her şey container içinde hazır.

#### Adım 1: Sistemi Başlat

```powershell
# PowerShell'i Aç (Admin değil, normal yeterli)
cd C:\Users\user\Desktop\p2p-comm

# Tüm servisleri başlat (5 node + etcd + PostgreSQL)
docker-compose up -d

# Servislerin durumunu kontrol et
docker-compose ps

# Logları izle
docker-compose logs -f
```

#### Adım 2: Test Senaryolarını Çalıştır

```powershell
# Test scriptini çalıştır
.\scripts\test-scenario.ps1
```

Bu 3 senaryoyu test eder:
1. **Başarılı Consensus**: 4 node aynı değeri oylar → threshold'a ulaşılır
2. **Byzantine Detection**: 1 node farklı oylar → tespit edilir ve banlanır
3. **Double Voting**: Aynı node 2 kez farklı oylar → reddedilir

#### Adım 3: Manuel Test

Kendin vote gönder:

```powershell
# Node 1'den vote gönder
docker-compose exec node1 /app/threshold-voting-system vote --tx-id tx_manual_001 --value 42

# Node 2'den vote gönder
docker-compose exec node2 /app/threshold-voting-system vote --tx-id tx_manual_001 --value 42

# Node 3'ten vote gönder
docker-compose exec node3 /app/threshold-voting-system vote --tx-id tx_manual_001 --value 42

# Node 4'ten vote gönder (threshold'a ulaşır!)
docker-compose exec node4 /app/threshold-voting-system vote --tx-id tx_manual_001 --value 42
```

Consensus'ı kontrol et:
```powershell
docker-compose logs | Select-String "threshold reached"
```

#### Adım 4: Sistemi Durdur

```powershell
# Servisleri durdur
docker-compose down

# Tüm verileri sil (temiz başlangıç)
docker-compose down -v
```

---

### Seçenek 2: Local Build (İsterseniz)

Local build yapmak isterseniz protobuf kurmanız gerekir.

#### Adım 1: Protobuf Kur

**Otomatik Kurulum (Önerilen):**

```powershell
# PowerShell'i **Administrator olarak** aç
cd C:\Users\user\Desktop\p2p-comm

# Kurulum scriptini çalıştır
.\scripts\setup-protobuf.ps1
```

Script şunları yapar:
1. Protobuf compiler'ı GitHub'dan indirir
2. `C:\protobuf` klasörüne çıkarır
3. Otomatik olarak PATH'e ekler

**Kurulum sonrası**:
- PowerShell penceresini **KAPAT**
- **YENİ** bir PowerShell aç
- Test et:
  ```powershell
  protoc --version
  # Çıktı: libprotoc 28.3 gibi olmalı
  ```

**Manuel Kurulum:**

Eğer script çalışmazsa:

1. İndir: https://github.com/protocolbuffers/protobuf/releases
   - `protoc-28.3-win64.zip` dosyasını indir
2. Çıkar: `C:\protobuf` klasörüne
3. PATH'e ekle:
   ```powershell
   # PowerShell Admin olarak
   [Environment]::SetEnvironmentVariable(
       "Path", 
       $env:Path + ";C:\protobuf\bin", 
       "Machine"
   )
   ```
4. Yeni terminal aç ve test et: `protoc --version`

#### Adım 2: Rust Build

```powershell
# Rust kurulu olmalı (rustup.rs)
cd C:\Users\user\Desktop\p2p-comm

# Build et
cargo build --release

# Binary burda olacak
.\target\release\threshold-voting-system.exe
```

#### Adım 3: Infrastructure Başlat

```powershell
# Sadece etcd ve PostgreSQL başlat
docker-compose up -d etcd1 etcd2 etcd3 postgres
```

#### Adım 4: Node Çalıştır

```powershell
# Environment variables set et
$env:NODE_ID="node_local"
$env:PEER_ID="peer_local"
$env:LISTEN_ADDR="/ip4/0.0.0.0/tcp/9999"
$env:ETCD_ENDPOINTS="http://localhost:2379"
$env:POSTGRES_URL="postgresql://threshold:threshold_pass@localhost:5432/threshold_voting"
$env:TOTAL_NODES="5"
$env:THRESHOLD="4"
$env:VOTE_TIMEOUT_SECS="300"

# Node'u çalıştır
.\target\release\threshold-voting-system.exe run
```

#### Adım 5: Vote Gönder

Başka bir PowerShell penceresi aç:

```powershell
# Aynı environment variables
$env:NODE_ID="node_local"
$env:PEER_ID="peer_local"
$env:ETCD_ENDPOINTS="http://localhost:2379"
$env:POSTGRES_URL="postgresql://threshold:threshold_pass@localhost:5432/threshold_voting"

# Vote gönder
.\target\release\threshold-voting-system.exe vote --tx-id test_001 --value 42
```

---

## Sistem Mimarisi

```
┌─────────────────────────────────────────────────────────┐
│                  Voting Nodes (N=5)                     │
│  ┌──────┐  ┌──────┐  ┌──────┐  ┌──────┐  ┌──────┐    │
│  │Node 1│  │Node 2│  │Node 3│  │Node 4│  │Node 5│    │
│  └───┬──┘  └───┬──┘  └───┬──┘  └───┬──┘  └───┬──┘    │
│      │         │         │         │         │         │
│      └─────────┴─────────┴─────────┴─────────┘         │
│                       │                                 │
│              libp2p P2P Network                        │
│           (Noise Protocol + GossipSub)                 │
└─────────────────────────────────────────────────────────┘
                        │
        ┌───────────────┼───────────────┐
        │               │               │
        ▼               ▼               ▼
  ┌──────────┐   ┌──────────┐   ┌──────────┐
  │ etcd-1   │   │ etcd-2   │   │ etcd-3   │
  │ (Raft)   │   │ (Raft)   │   │ (Raft)   │
  └──────────┘   └──────────┘   └──────────┘
        │               │               │
        └───────────────┼───────────────┘
                        │
                        ▼
                ┌──────────────┐
                │  PostgreSQL  │
                │  (Audit Log) │
                └──────────────┘
```

---

## Sık Karşılaşılan Sorunlar

### Docker Desktop çalışmıyor

```powershell
# Docker Desktop'ı başlat
# Taskbar'da whale icon'u göreceksin
# "Docker Desktop is running" yazana kadar bekle
```

### Port 2379 kullanılıyor

```powershell
# Çakışan servisleri durdur
docker-compose down

# Portları kontrol et
netstat -ano | findstr "2379"
```

### PowerShell execution policy hatası

```powershell
# Script çalıştırma izni ver (Admin PowerShell):
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser

# Veya tek seferlik:
powershell -ExecutionPolicy Bypass -File .\scripts\test-scenario.ps1
```

### protoc bulunamadı hatası

```
error: Could not find `protoc`
```

**Çözüm**: 
1. `.\scripts\setup-protobuf.ps1` çalıştır (Admin PowerShell)
2. PowerShell'i kapat ve yeniden aç
3. `protoc --version` ile test et

---

## Faydalı Komutlar

```powershell
# Tüm container'ları gör
docker-compose ps

# Belirli service'in loglarını izle
docker-compose logs -f node1

# Tüm logları izle
docker-compose logs -f

# Container'a bash ile bağlan
docker-compose exec node1 bash

# Service'i yeniden başlat
docker-compose restart node1

# Tek service başlat
docker-compose up -d node1

# Build'i zorla
docker-compose build --no-cache

# Tüm container'ları ve volume'ları sil
docker-compose down -v

# Docker disk kullanımını temizle
docker system prune -a
```

---

## Dosya Yapısı

```
C:\Users\user\Desktop\p2p-comm\
│
├── scripts\
│   ├── setup-protobuf.ps1      ← Protobuf otomatik kurulum (Windows)
│   └── test-scenario.ps1       ← Otomatik test senaryoları (Windows)
│
├── docker-compose.yml          ← Docker deployment config
├── README.md                   ← Tam dokümantasyon
├── QUICKSTART-WINDOWS.md       ← Bu dosya
└── IMPLEMENTATION_STATUS.md    ← Implementasyon durumu
```

---

## Sıradaki Adımlar

1. **Docker ile Test Et**: `docker-compose up -d` → `.\scripts\test-scenario.ps1`
2. **Mimariyi İncele**: `guide.tex` veya `implementation_guide.pdf`
3. **Kodu Keşfet**: `crates\` klasöründeki modular yapı
4. **Kendi Testlerini Yaz**: `test-scenario.ps1`'i düzenle
5. **Monitoring Ekle**: Prometheus + Grafana (gelecek)

---

## Destek

Sorun yaşarsan:
- [README.md](README.md) - Detaylı dokümantasyon
- [IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md) - Durum raporu
- [guide.tex](guide.tex) - Mimari dokümantasyon

## Lisans

Bu bir prototip implementasyon, eğitim ve araştırma amaçlıdır.
