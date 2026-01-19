# 📝 Proje İsimlerini Değiştirme Talimatları

## 🎯 Amaç

Proje isimlerini daha mantıklı hale getirmek:
- `mtls-sharedmem` → `p2p-comm` (P2P communication - libp2p)
- `mtls-with-mtls` → `mtls-comm` (mTLS communication - pure mTLS)

## ⚠️ Önemli: Docker Containerları Durdur

```bash
# Tüm çalışan containerları durdur
docker ps --format "{{.Names}}" | grep -E "(threshold|mtls)-" | xargs docker stop
```

## 📂 Adım 1: Folder İsimlerini Değiştir (MANUEL)

**Windows Explorer'da şunları yap:**

1. `MPC-WALLET\mtls-sharedmem` klasörüne sağ tıkla → **Rename** → `p2p-comm` yaz
2. `MPC-WALLET\mtls-with-mtls` klasörüne sağ tıkla → **Rename** → `mtls-comm` yaz

**NOT**: Eğer "The action can't be completed because the file is open" hatası alırsan:
- VS Code'u kapat
- Tüm terminal/cmd pencerelerini kapat
- Docker Desktop'ı durdur
- Tekrar dene

## 🔄 Adım 2: Referansları Güncelle (OTOMATİK)

Folder isimlerini değiştirdikten sonra:

```bash
cd c:\Users\user\Desktop\MPC-WALLET
bash rename_projects.sh
```

Bu script şunları yapacak:
- ✅ Tüm Rust dosyalarında isimleri değiştirir
- ✅ Tüm bash script'lerde isimleri değiştirir
- ✅ Tüm Python script'lerde isimleri değiştirir
- ✅ Tüm markdown dosyalarda isimleri değiştirir

## 🔨 Adım 3: Projeleri Yeniden Derle

```bash
# p2p-comm'u derle
cd p2p-comm
cargo build --release

# mtls-comm'u derle
cd ../mtls-comm
cargo build --release

# benchmark-suite'i derle
cd ../benchmark-suite
cargo build --release
```

## ✅ Adım 4: Doğrula

```bash
# Binary'lerin var olduğunu kontrol et
ls p2p-comm/target/release/threshold-voting-system.exe
ls mtls-comm/target/release/threshold-voting.exe

# Benchmark'i test et
cd benchmark-suite
bash run_simple_benchmark.sh
```

## 📋 Değiştirilecek Dosyalar (Otomatik)

Script şu dosyaları güncelleyecek:

### Benchmark Suite
- `benchmark-suite/src/lib.rs`
- `benchmark-suite/src/main.rs`
- `benchmark-suite/src/integration_bench.rs`
- `benchmark-suite/benches/network_throughput.rs`
- `benchmark-suite/run_simple_benchmark.sh`
- `benchmark-suite/run_benchmarks.sh`
- `benchmark-suite/analyze_results.py`
- `benchmark-suite/README.md`
- `benchmark-suite/BENCHMARK_REPORT.md`
- `benchmark-suite/BENCHMARK_SUMMARY.md`

### Root Docs
- `BENCHMARK_README.md`
- `BENCHMARK_INSTRUCTIONS.md`
- `BENCHMARK_COMPARISON.md`
- `INTEGRATION-PLAN.md`
- `PROJELER-OZET.md`

### Project Docs
- `p2p-comm/README.md`
- `p2p-comm/QUICK_REFERENCE.md`
- `p2p-comm/TEST_COMMANDS.md`
- `p2p-comm/CURRENT_IMPLEMENTATION_STATUS.md`
- `p2p-comm/SECURITY_TESTING.md`
- `p2p-comm/QUICKSTART-WINDOWS.md`
- `p2p-comm/src/benchmark.rs`
- `mtls-comm/README.md`
- `mtls-comm/Cargo.toml`
- `mtls-comm/src/benchmark.rs`

## 🐛 Sorun Giderme

### "Permission denied" hatası
→ Tüm terminal/IDE'leri kapat, Docker'ı durdur

### "sed: command not found"
→ Git Bash veya WSL kullan

### Cargo build hatası
→ `cargo clean` yap, sonra tekrar `cargo build --release`

## 📊 Değişiklik Özeti

| Eski İsim | Yeni İsim | Açıklama |
|-----------|-----------|----------|
| `mtls-sharedmem` | `p2p-comm` | libp2p + Noise Protocol + GossipSub |
| `mtls-with-mtls` | `mtls-comm` | Pure mTLS 1.3 + rustls |
| `MtlsSharedmem` (enum) | `P2pComm` | Rust enum variant |
| `MtlsWithMtls` (enum) | `MtlsComm` | Rust enum variant |

---

**Hazırlayan**: Claude Code Agent
**Tarih**: 19 Ocak 2026
