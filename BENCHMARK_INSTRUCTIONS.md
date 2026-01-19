# Benchmark Çalıştırma Talimatları

Bu dokümanda her iki sistemde de benchmarkları nasıl çalıştıracağınız detaylı olarak açıklanmaktadır.

---

## 📋 İçindekiler

1. [Hızlı Başlangıç](#hızlı-başlangıç)
2. [p2p-comm Benchmarkları](#p2p-comm-benchmarkları)
3. [mtls-comm Benchmarkları](#mtls-comm-benchmarkları)
4. [Gerçek Cluster Testleri](#gerçek-cluster-testleri)
5. [Sonuçları Analiz Etme](#sonuçları-analiz-etme)
6. [Troubleshooting](#troubleshooting)

---

## 🚀 Hızlı Başlangıç

### Ön Gereksinimler

```bash
# Rust toolchain (1.70+)
rustc --version

# Docker & Docker Compose (opsiyonel, gerçek testler için)
docker --version
docker-compose --version
```

### Basit Test (Simülasyon)

Her iki repo için de:

```bash
# 1. Repository'ye git
cd p2p-comm   # veya mtls-comm

# 2. Build
cargo build --release

# 3. Tüm benchmarkları çalıştır
cargo run --release -- benchmark-all --iterations 1000 --verbose
```

**Süre**: ~30 saniye (simüle edilmiş testler)

---

## 🔧 p2p-comm Benchmarkları

### 1. Serialization Benchmark

```bash
cd p2p-comm

# Basit test (1000 iterasyon)
cargo run --release -- benchmark --iterations 1000

# Detaylı test (10000 iterasyon)
cargo run --release -- benchmark --iterations 10000 --verbose

# Çok detaylı test (100000 iterasyon)
cargo run --release -- benchmark --iterations 100000 --verbose
```

**Çıktı Örneği**:
```
🚀 Serialization Performance Benchmark
═════════════════════════════════════════
Testing 1000 iterations per format

📊 JSON Serialization Benchmark
─────────────────────────────────────
  Iterations:     1000
  Total Time:     45.23 ms
  Avg Time:       45.23 μs
  Throughput:     22105 ops/sec
  Avg Size:       452 bytes

  Bandwidth:      9.99 MB/sec

📊 Binary Serialization Benchmark
─────────────────────────────────────
  Iterations:     1000
  Total Time:     18.76 ms
  Avg Time:       18.76 μs
  Throughput:     53297 ops/sec
  Avg Size:       318 bytes

  Bandwidth:      16.95 MB/sec

📈 Performance Comparison
─────────────────────────────────────
  Speed:          Binary is 2.41x faster
  Size:           Binary is 29.6% smaller
  Throughput:     Binary is 2.41x higher

✅ Benchmark Complete!
```

### 2. Tüm Benchmarklar

```bash
# Hızlı test (1000 iterasyon)
cargo run --release -- benchmark-all --iterations 1000

# Orta test (5000 iterasyon)
cargo run --release -- benchmark-all --iterations 5000 --verbose

# Kapsamlı test (10000 iterasyon)
RUST_LOG=info cargo run --release -- benchmark-all --iterations 10000 --verbose
```

**Çalıştırılacak Testler**:
1. ✅ Serialization (JSON vs Binary)
2. ✅ Cryptography (Ed25519)
3. ✅ Vote Processing Throughput
4. ✅ libp2p Connection Establishment
5. ✅ GossipSub Message Propagation
6. ✅ Byzantine Detection Overhead
7. ✅ Storage (etcd + PostgreSQL)
8. ✅ End-to-End Vote Latency

**Toplam Süre**: ~45 saniye (1000 iterasyon)

### 3. Sadece Crypto Benchmark

```bash
# Ed25519 performance test
cargo test --release benchmark_crypto -- --nocapture --ignored
```

### 4. Logging ile Çalıştırma

```bash
# Debug logging
RUST_LOG=debug cargo run --release -- benchmark-all --iterations 1000

# Trace logging (çok detaylı)
RUST_LOG=trace cargo run --release -- benchmark-all --iterations 100
```

---

## 🔐 mtls-comm Benchmarkları

### 1. Serialization Benchmark

```bash
cd mtls-comm

# Basit test (1000 iterasyon)
cargo run --release -- benchmark --iterations 1000

# Detaylı test (10000 iterasyon)
cargo run --release -- benchmark --iterations 10000 --verbose
```

**Çıktı Örneği**:
```
🚀 Serialization Performance Benchmark
═════════════════════════════════════════
Testing 1000 iterations (JSON format)

📊 JSON Serialization Benchmark
─────────────────────────────────────
  Iterations:     1000
  Total Time:     47.89 ms
  Avg Time:       47.89 μs
  Throughput:     20882 ops/sec
  Avg Size:       458 bytes

  Bandwidth:      9.56 MB/sec

✅ Benchmark Complete!
```

### 2. Tüm Benchmarklar

```bash
# Hızlı test
cargo run --release -- benchmark-all --iterations 1000

# Kapsamlı test
RUST_LOG=info cargo run --release -- benchmark-all --iterations 10000 --verbose
```

**Çalıştırılacak Testler**:
1. ✅ Serialization (JSON)
2. ✅ Cryptography (Ed25519)
3. ✅ Vote Processing Throughput
4. ✅ mTLS Connection Establishment
5. ✅ Mesh Broadcast Propagation
6. ✅ Byzantine Detection Overhead
7. ✅ Storage (etcd + PostgreSQL)
8. ✅ Certificate Validation (X.509)
9. ✅ End-to-End Vote Latency

**Toplam Süre**: ~50 saniye (1000 iterasyon)

### 3. Certificate Benchmark

```bash
# X.509 validation performance
cargo run --release -- benchmark-all --iterations 5000 | grep -A 8 "Certificate Validation"
```

---

## 🐳 Gerçek Cluster Testleri

### p2p-comm ile Gerçek Test

#### 1. Infrastructure Başlat

```bash
cd p2p-comm

# etcd + PostgreSQL başlat
docker-compose up -d etcd-1 etcd-2 etcd-3 postgres

# Servislerin hazır olmasını bekle
sleep 10

# Health check
docker exec mtls-etcd-1 etcdctl endpoint health
docker exec mtls-postgres pg_isready -U mpc
```

#### 2. Node Başlat

```bash
# Terminal 1: Node-1
NODE_ID=node_1 \
PEER_ID=peer_1 \
LISTEN_ADDR=0.0.0.0:9000 \
RUST_LOG=info \
cargo run --release -- run

# Terminal 2: Node-2
NODE_ID=node_2 \
PEER_ID=peer_2 \
LISTEN_ADDR=0.0.0.0:9001 \
BOOTSTRAP_PEERS=/ip4/127.0.0.1/tcp/9000 \
RUST_LOG=info \
cargo run --release -- run
```

#### 3. Gerçek Vote Gönder

```bash
# Terminal 3: Vote submit
cargo run --release -- vote --tx-id "real_test_001" --value 42

# Vote status kontrol
cargo run --release -- status --tx-id "real_test_001"
```

#### 4. Performance Ölç

```bash
# 100 vote gönder ve latency ölç
for i in {1..100}; do
    time cargo run --release -- vote --tx-id "perf_test_$i" --value 42
done
```

### mtls-comm ile Gerçek Test

#### 1. Certificates Oluştur

```bash
cd mtls-comm

# Certificate generation script
chmod +x scripts/generate-certs.sh
./scripts/generate-certs.sh

# Verify certificates
ls -la certs/
openssl x509 -in certs/ca.crt -noout -text
openssl verify -CAfile certs/ca.crt certs/node1.crt
```

#### 2. Infrastructure Başlat

```bash
# etcd + PostgreSQL
docker-compose up -d etcd-1 etcd-2 etcd-3 postgres

# Health check
docker exec mtls-etcd-1 etcdctl endpoint health
```

#### 3. mTLS Nodes Başlat

```bash
# Terminal 1: Node-1
NODE_ID=1 \
CA_CERT_PATH=certs/ca.crt \
NODE_CERT_PATH=certs/node1.crt \
NODE_KEY_PATH=certs/node1.key \
LISTEN_ADDR=0.0.0.0:9000 \
RUST_LOG=info \
cargo run --release -- run

# Terminal 2: Node-2
NODE_ID=2 \
CA_CERT_PATH=certs/ca.crt \
NODE_CERT_PATH=certs/node2.crt \
NODE_KEY_PATH=certs/node2.key \
LISTEN_ADDR=0.0.0.0:9001 \
BOOTSTRAP_PEERS=127.0.0.1:9000 \
RUST_LOG=info \
cargo run --release -- run
```

#### 4. mTLS Connection Test

```bash
# Test TLS connection with openssl
openssl s_client -connect localhost:9000 \
    -cert certs/node2.crt \
    -key certs/node2.key \
    -CAfile certs/ca.crt

# Should see: "Verify return code: 0 (ok)"
```

#### 5. Performance Test

```bash
# 100 votes with timing
for i in {1..100}; do
    time cargo run --release -- vote --tx-id "mTLS_perf_$i" --value 42
done
```

---

## 📊 Sonuçları Analiz Etme

### Benchmark Çıktısını Kaydetme

```bash
# p2p-comm
cargo run --release -- benchmark-all --iterations 10000 --verbose \
    > results_libp2p.txt 2>&1

# mtls-comm
cargo run --release -- benchmark-all --iterations 10000 --verbose \
    > results_mtls.txt 2>&1
```

### Karşılaştırmalı Analiz

```bash
# Serialization karşılaştırması
echo "=== SERIALIZATION COMPARISON ===" > comparison.txt
grep -A 5 "JSON Serialization" results_libp2p.txt >> comparison.txt
grep -A 5 "Binary Serialization" results_libp2p.txt >> comparison.txt
grep -A 5 "JSON Serialization" results_mtls.txt >> comparison.txt

# Crypto karşılaştırması
echo -e "\n=== CRYPTO COMPARISON ===" >> comparison.txt
grep -A 8 "Ed25519" results_libp2p.txt >> comparison.txt
grep -A 8 "Ed25519" results_mtls.txt >> comparison.txt

# End-to-End karşılaştırması
echo -e "\n=== E2E LATENCY COMPARISON ===" >> comparison.txt
grep -A 8 "End-to-End" results_libp2p.txt >> comparison.txt
grep -A 8 "End-to-End" results_mtls.txt >> comparison.txt

cat comparison.txt
```

### CSV Export (Analysis için)

```bash
# Script: parse_benchmarks.sh
cat > parse_benchmarks.sh << 'EOF'
#!/bin/bash

echo "Test,System,Iterations,Avg_ms,P50_ms,P95_ms,P99_ms"

# Parse libp2p results
grep -A 6 "Ed25519 Key Generation" results_libp2p.txt | \
    awk '/Average:/ {avg=$2} /P50/ {p50=$3} /P95/ {p95=$2} /P99/ {p99=$2}
         END {print "Ed25519_KeyGen,libp2p,1000,"avg","p50","p95","p99}'

# Parse mTLS results
grep -A 6 "Ed25519 Key Generation" results_mtls.txt | \
    awk '/Average:/ {avg=$2} /P50/ {p50=$3} /P95/ {p95=$2} /P99/ {p99=$2}
         END {print "Ed25519_KeyGen,mTLS,1000,"avg","p50","p95","p99}'

# ... diğer benchmarklar için tekrarla
EOF

chmod +x parse_benchmarks.sh
./parse_benchmarks.sh > benchmark_results.csv
```

### Grafik Oluşturma (gnuplot)

```bash
# gnuplot script
cat > plot_benchmarks.gnu << 'EOF'
set terminal png size 800,600
set output 'benchmark_comparison.png'
set title "End-to-End Latency Comparison"
set xlabel "System"
set ylabel "Latency (ms)"
set style data histograms
set style fill solid border -1
set boxwidth 0.9
set grid ytics

plot 'benchmark_data.txt' using 2:xtic(1) title 'Average' linecolor rgb 'blue', \
     '' using 3 title 'P95' linecolor rgb 'red', \
     '' using 4 title 'P99' linecolor rgb 'orange'
EOF

# Veri dosyası oluştur
cat > benchmark_data.txt << 'EOF'
libp2p 10.1 12.5 15.2
mTLS 7.1 9.3 11.8
EOF

gnuplot plot_benchmarks.gnu
```

---

## 🔬 İleri Seviye Testler

### 1. Network Latency Injection

```bash
# Linux tc (traffic control) ile network latency ekle
sudo tc qdisc add dev lo root netem delay 10ms

# Benchmarkları tekrar çalıştır
cargo run --release -- benchmark-all --iterations 1000

# Temizle
sudo tc qdisc del dev lo root
```

### 2. CPU Profiling

```bash
# Flamegraph ile profiling
cargo install flamegraph

# p2p-comm profiling
cargo flamegraph --bin threshold-voting -- benchmark-all --iterations 10000

# mtls-comm profiling
cargo flamegraph --bin threshold-voting -- benchmark-all --iterations 10000
```

### 3. Memory Profiling

```bash
# Valgrind ile memory leak check
cargo build --release
valgrind --leak-check=full \
    target/release/threshold-voting benchmark-all --iterations 1000
```

### 4. Stress Test

```bash
# 10 concurrent benchmark runs
for i in {1..10}; do
    cargo run --release -- benchmark-all --iterations 5000 &
done
wait

# CPU & Memory monitoring
htop
```

---

## 🐛 Troubleshooting

### Build Hatası

```bash
# Clean build
cargo clean
cargo build --release

# Update dependencies
cargo update
```

### Runtime Hatası: "Connection refused"

```bash
# etcd çalışıyor mu?
docker ps | grep etcd

# PostgreSQL çalışıyor mu?
docker ps | grep postgres

# Port kullanımda mı?
lsof -i :9000
netstat -tulpn | grep 9000
```

### Benchmark Çok Yavaş

```bash
# Release mode kullandığınızdan emin olun
cargo run --release -- benchmark-all

# Debug mode çok yavaş:
# ❌ cargo run -- benchmark-all        (10x daha yavaş)
# ✅ cargo run --release -- benchmark-all
```

### Certificate Hatası (mTLS)

```bash
# Certificate expiry check
openssl x509 -in certs/node1.crt -noout -dates

# Certificate chain verify
openssl verify -CAfile certs/ca.crt certs/node1.crt

# Regenerate certificates
rm -rf certs/
./scripts/generate-certs.sh
```

### etcd Connection Error

```bash
# etcd cluster health
docker exec mtls-etcd-1 etcdctl endpoint health
docker exec mtls-etcd-1 etcdctl member list

# Restart etcd
docker-compose restart etcd-1 etcd-2 etcd-3

# Check logs
docker logs mtls-etcd-1
```

### PostgreSQL Connection Error

```bash
# Check PostgreSQL
docker exec -it mtls-postgres psql -U mpc -d mpc_wallet -c "SELECT 1;"

# Reset database
docker-compose down -v
docker-compose up -d postgres

# Re-run schema
docker exec -it mtls-postgres psql -U mpc -d mpc_wallet < schema.sql
```

---

## 📝 Benchmark Checklist

Benchmark çalıştırmadan önce kontrol edin:

- [ ] Rust 1.70+ kurulu
- [ ] Release mode kullanılıyor (`--release`)
- [ ] Iterasyon sayısı belirlendi (min 1000)
- [ ] Infrastructure hazır (gerçek testler için)
- [ ] Loglar temizlendi (`cargo clean`)
- [ ] Yeterli disk alanı var (>1GB)
- [ ] Başka ağır işlem çalışmıyor

---

## 🎯 Benchmark Stratejisi

### Hızlı Validasyon (5 dakika)

```bash
# Her iki repo için de
cargo run --release -- benchmark --iterations 1000
```

### Orta Seviye Test (30 dakika)

```bash
# Simülasyon testleri
cargo run --release -- benchmark-all --iterations 5000 --verbose

# Sonuçları kaydet
cargo run --release -- benchmark-all --iterations 5000 --verbose \
    > results_$(date +%Y%m%d_%H%M%S).txt
```

### Kapsamlı Test (2-3 saat)

```bash
# 1. Infrastructure başlat
docker-compose up -d

# 2. Gerçek cluster testleri
./run_cluster_tests.sh

# 3. Yüksek iterasyon benchmarkları
cargo run --release -- benchmark-all --iterations 50000 --verbose

# 4. Stress testler
./run_stress_tests.sh
```

---

## 📚 Ek Kaynaklar

- [BENCHMARK_COMPARISON.md](./BENCHMARK_COMPARISON.md) - Detaylı karşılaştırma
- [p2p-comm/README.md](./p2p-comm/README.md) - libp2p implementation
- [mtls-comm/README.md](./mtls-comm/README.md) - mTLS implementation

---

**Son Güncelleme**: 2026-01-19
