# MPC Wallet

[![CI](https://github.com/FAME-CRYPT/torcus-wallet/actions/workflows/ci.yml/badge.svg)](https://github.com/FAME-CRYPT/torcus-wallet/actions/workflows/ci.yml)

A threshold signature wallet system in Rust supporting **CGGMP24** (ECDSA/SegWit) and **FROST** (Schnorr/Taproot) protocols. Private keys are distributed across multiple nodes and never reconstructed.

## Features

- **Threshold Signatures**: Configurable t-of-n (default 3-of-4)
- **Bitcoin SegWit**: P2WPKH addresses with ECDSA (CGGMP24)
- **Bitcoin Taproot**: P2TR addresses with Schnorr (FROST)
- **HD Derivation**: BIP32/BIP84/BIP86 address derivation
- **Multi-Network**: Mainnet, testnet, and regtest support
- **Persistent Storage**: SQLite-based wallet and key share storage

## Quick Start

```bash
# Start services (regtest mode)
./scripts/start-regtest.sh

# Create a Taproot wallet
cargo run --bin mpc-wallet -- taproot-create --name "My Wallet"

# Mine blocks to fund wallet (regtest)
cargo run --bin mpc-wallet -- mine --wallet-id <UUID> --blocks 101

# Check balance
cargo run --bin mpc-wallet -- balance --wallet-id <UUID>

# Send Bitcoin
cargo run --bin mpc-wallet -- taproot-send --wallet-id <UUID> --to <ADDRESS> --amount 100000000

# Stop services
./scripts/stop.sh
```

## Architecture

```
┌─────────────┐           ┌─────────────────┐           ┌──────────────┐
│  wallet-cli │ ────────► │   coordinator   │ ◄───────► │  mpc-node-1  │
└─────────────┘           │   (port 3000)   │           └──────────────┘
                          │                 │           ┌──────────────┐
                          │  Orchestrates   │ ◄───────► │  mpc-node-2  │
                          │  DKG & signing  │           └──────────────┘
                          │                 │           ┌──────────────┐
                          │                 │ ◄───────► │  mpc-node-3  │
                          │                 │           └──────────────┘
                          │                 │           ┌──────────────┐
                          │                 │ ◄───────► │  mpc-node-4  │
                          └─────────────────┘           └──────────────┘
```

## Protocol Comparison

| Feature | CGGMP24 | FROST |
|---------|---------|-------|
| Signature Type | ECDSA | Schnorr |
| Bitcoin Address | P2WPKH (SegWit) | P2TR (Taproot) |
| Setup Time | 1-5 min (one-time) | Instant |
| Signing Time (2-of-3) | ~1.0 s | ~2 ms |
| Signing Time (3-of-4) | ~2.0 s | ~3 ms |

## Benchmarks

Run protocol benchmarks locally to measure signing performance:

```bash
# FROST benchmarks
cargo test --package protocols benchmark_frost --release -- --nocapture

# CGGMP24 benchmarks
cargo test --package protocols benchmark_cggmp24_2of3 --release -- --nocapture
cargo test --package protocols benchmark_cggmp24_3of4 --release -- --nocapture
```

### Benchmark Results

**FROST (Schnorr/Taproot)**
| Configuration | Key Generation | Signing | Total |
|---------------|----------------|---------|-------|
| 2-of-2 | ~2 ms | ~2 ms | ~4 ms |
| 2-of-3 | ~4 ms | ~2 ms | ~6 ms |
| 3-of-5 | ~11 ms | ~3 ms | ~14 ms |

**CGGMP24 (ECDSA/SegWit)** - with cached key shares
| Configuration | Cache Load | Signing | Total |
|---------------|------------|---------|-------|
| 2-of-3 | ~1 ms | ~1.0 s | ~1.0 s |
| 3-of-4 | ~1 ms | ~2.0 s | ~2.0 s |

*Note: CGGMP24 first run generates primes and aux_info (~30-60s per party). These are cached in `target/bench-primes-cache/` for subsequent runs.*

## CLI Commands

### Wallet Management
```bash
mpc-wallet list-wallets                          # List all wallets
mpc-wallet get-wallet --wallet-id <UUID>         # Get wallet details
mpc-wallet delete-wallet --wallet-id <UUID>      # Delete wallet
mpc-wallet balance --wallet-id <UUID>            # Check balance
mpc-wallet derive-addresses --wallet-id <UUID>   # Derive HD addresses
```

### CGGMP24 (SegWit/ECDSA)
```bash
mpc-wallet cggmp24-init                                          # Initialize (one-time)
mpc-wallet cggmp24-status                                        # Check node status
mpc-wallet cggmp24-create --name "Wallet"                        # Create wallet
mpc-wallet cggmp24-send --wallet-id <UUID> --to <ADDR> --amount <SATS>  # Send
```

### FROST (Taproot/Schnorr)
```bash
mpc-wallet taproot-create --name "Wallet"                        # Create wallet
mpc-wallet taproot-send --wallet-id <UUID> --to <ADDR> --amount <SATS>  # Send
```

### Regtest
```bash
mpc-wallet mine --wallet-id <UUID> --blocks 101  # Mine blocks
mpc-wallet faucet --wallet-id <UUID>             # Faucet info (testnet)
```

## Crates

| Crate | Description |
|-------|-------------|
| `cli` | Command-line interface |
| `coordinator` | MPC protocol orchestration and message relay |
| `node` | Key share holder, participates in signing |
| `common` | Shared types and storage |
| `chains` | Bitcoin address derivation and transactions |
| `protocols` | CGGMP24 and FROST implementations |

## Docker Services

| Service | Port | Description |
|---------|------|-------------|
| coordinator | 3000 | Orchestration server |
| mpc-node-1..4 | 3001-3004 | MPC nodes |
| bitcoind | 18443 | Bitcoin Core (regtest) |

## Security Notes

This is a demonstration implementation. For production use, consider:

- Encrypt key shares at rest
- Use TLS/mTLS for node communication
- Add authentication between coordinator and nodes
- Deploy nodes in isolated network segments
- Get a professional security audit

## References

- [CGGMP21 Paper](https://eprint.iacr.org/2021/060)
- [FROST Paper](https://eprint.iacr.org/2020/852)
- [BIP-340](https://github.com/bitcoin/bips/blob/master/bip-0340.mediawiki) - Schnorr Signatures
- [BIP-341](https://github.com/bitcoin/bips/blob/master/bip-0341.mediawiki) - Taproot

## License

MIT
