# MPC Wallet Integration Tests

Comprehensive integration tests for the production MPC wallet system.

## Test Structure

```
tests/
├── common/
│   ├── mod.rs              # Common test utilities
│   ├── fixtures.rs         # Test data fixtures
│   ├── mock_services.rs    # Mock services (Bitcoin, crypto)
│   └── test_containers.rs  # Testcontainers setup (PostgreSQL, etcd)
├── storage_integration.rs      # Storage layer tests
├── consensus_integration.rs    # Consensus and Byzantine detection tests
├── bitcoin_integration.rs      # Bitcoin transaction building tests
├── api_integration.rs          # API serialization/validation tests
├── protocols_integration.rs    # MPC protocol tests
└── network_integration.rs      # Network and messaging tests
```

## Test Categories

### 1. Storage Integration Tests (`storage_integration.rs`)

Tests PostgreSQL and etcd integration:
- Connection and schema validation
- Vote recording and retrieval
- Transaction state transitions
- Byzantine violation recording
- Distributed locking (etcd)
- Concurrent access handling
- Vote counting and cleanup

**Run:**
```bash
cargo test --test storage_integration
```

### 2. Consensus Integration Tests (`consensus_integration.rs`)

Tests Byzantine fault detection and consensus:
- Valid vote acceptance
- Double-vote detection
- Invalid signature detection
- Threshold consensus (4-of-5)
- Minority vote detection
- Idempotent vote submission
- Concurrent vote processing
- Edge cases (tie votes, exactly threshold)

**Run:**
```bash
cargo test --test consensus_integration
```

### 3. Bitcoin Integration Tests (`bitcoin_integration.rs`)

Tests Bitcoin transaction building:
- Transaction builder (P2WPKH, P2TR)
- OP_RETURN metadata (up to 80 bytes)
- Fee estimation and calculation
- UTXO selection (greedy algorithm)
- Change calculation and dust limit
- Multiple outputs
- Transaction finalization with signatures
- Address validation

**Run:**
```bash
cargo test --test bitcoin_integration
```

### 4. API Integration Tests (`api_integration.rs`)

Tests API types and serialization:
- Vote serialization/deserialization
- Transaction JSON encoding
- Byzantine violation messages
- Network message formats
- Error response formats
- Health check responses
- Cluster status
- Concurrent serialization

**Run:**
```bash
cargo test --test api_integration
```

### 5. Protocol Integration Tests (`protocols_integration.rs`)

Tests MPC protocol flows:
- Presignature pool management
- DKG session coordination
- Signing round coordination
- Protocol message structures
- Session locking
- State transitions
- Mock cryptographic operations

**Run:**
```bash
cargo test --test protocols_integration
```

### 6. Network Integration Tests (`network_integration.rs`)

Tests network layer:
- QUIC connection lifecycle
- Message encoding/decoding (JSON, bincode)
- Peer discovery and management
- Heartbeat tracking
- Large message transmission
- Concurrent connections
- Message deduplication
- Network metrics

**Run:**
```bash
cargo test --test network_integration
```

## Prerequisites

### Docker Required

The tests use **testcontainers-rs** to spin up PostgreSQL and etcd containers. You must have Docker installed and running:

```bash
# Check Docker is running
docker ps

# If not running, start Docker Desktop or Docker daemon
```

### Environment Setup

No environment variables are required. The tests use testcontainers which automatically:
- Pulls necessary images (postgres:16-alpine, etcd:v3.5.11)
- Starts containers with random ports
- Provides connection strings
- Cleans up containers after tests

## Running Tests

### Run All Integration Tests

```bash
cd production
cargo test --tests
```

### Run Specific Test Suite

```bash
cargo test --test storage_integration
cargo test --test consensus_integration
cargo test --test bitcoin_integration
cargo test --test api_integration
cargo test --test protocols_integration
cargo test --test network_integration
```

### Run Specific Test

```bash
cargo test --test storage_integration test_vote_recording_and_retrieval
```

### Run with Output

```bash
cargo test --test consensus_integration -- --nocapture
```

### Run in Release Mode (faster)

```bash
cargo test --release --tests
```

## Test Execution Notes

### Timing

- **Storage tests**: ~10-30 seconds (depends on container startup)
- **Consensus tests**: ~15-45 seconds (many concurrent scenarios)
- **Bitcoin tests**: ~5-10 seconds (no external dependencies)
- **API tests**: ~2-5 seconds (pure serialization)
- **Protocol tests**: ~5-15 seconds (lightweight mocking)
- **Network tests**: ~5-10 seconds (lightweight mocking)

### Parallelization

Tests within each file run sequentially by default to avoid container conflicts. You can run different test files in parallel:

```bash
cargo test --tests -- --test-threads=4
```

### Slow Tests

Some tests are marked with `#[ignore]` and require manual opt-in:

```bash
cargo test -- --ignored
```

## Test Coverage Goals

| Component | Target Coverage | Current Status |
|-----------|----------------|----------------|
| Storage   | 90%+           | ✅ Comprehensive |
| Consensus | 95%+           | ✅ Comprehensive |
| Bitcoin   | 85%+           | ✅ Comprehensive |
| API       | 80%+           | ✅ Comprehensive |
| Protocols | 70%+           | ✅ Comprehensive |
| Network   | 75%+           | ✅ Comprehensive |

## Common Test Utilities

### `TestContext`

Provides all infrastructure (PostgreSQL, etcd, Bitcoin mock):

```rust
let ctx = TestContext::new().await;
let postgres_url = ctx.postgres_url();
let etcd_endpoints = ctx.etcd_endpoints();
let bitcoin_url = ctx.bitcoin_url();
```

### Fixtures

Pre-built test data:

```rust
let vote = sample_vote(node_id, tx_id, value);
let tx = sample_transaction(txid);
let violation = sample_byzantine_violation(node_id, tx_id, violation_type);
let utxos = sample_utxos();
```

### Mock Services

```rust
// Mock Bitcoin server
let bitcoin_mock = BitcoinMockServer::new().await;
bitcoin_mock.mock_address_balance("tb1q...", 100_000).await;

// Mock crypto operations
let sig = MockCrypto::mock_ecdsa_signature();
let pubkey = MockCrypto::mock_compressed_pubkey();

// Mock QUIC connection
let conn = MockQuicConnection::new("peer-1".to_string());

// Mock presignature pool
let pool = MockPresignaturePool::new(10);
pool.consume().await.unwrap();
```

## Debugging Tests

### Enable Logging

```bash
RUST_LOG=debug cargo test --test consensus_integration -- --nocapture
```

### Run Single Test with Backtrace

```bash
RUST_BACKTRACE=1 cargo test --test storage_integration test_vote_recording_and_retrieval
```

### Inspect Container Logs

Testcontainers keeps containers running during test execution. Check logs:

```bash
docker logs <container_id>
```

## Continuous Integration

These tests are designed to run in CI environments:

```yaml
# .github/workflows/test.yml
- name: Run integration tests
  run: cargo test --tests
  env:
    RUST_LOG: info
```

## Troubleshooting

### "Docker not found"

Ensure Docker is installed and running:
```bash
docker --version
docker ps
```

### "Port already in use"

Testcontainers uses random ports, but if you see port conflicts, stop other containers:
```bash
docker stop $(docker ps -q)
```

### "Connection refused to PostgreSQL/etcd"

Wait for containers to fully start. Tests include 2-second delays, but slow systems may need more time.

### "Test timeout"

Increase timeout in individual tests or run with:
```bash
cargo test --test storage_integration -- --test-threads=1
```

## Contributing

When adding new tests:

1. **Use testcontainers** for external dependencies
2. **Create deterministic test data** (no random IDs unless testing randomness)
3. **Clean up resources** in test teardown
4. **Document what each test validates** in comments
5. **Use fixtures** from `common/fixtures.rs`
6. **Test both happy and error paths**
7. **Test concurrent scenarios** where relevant
8. **Follow naming convention**: `test_<component>_<scenario>`

## License

MIT
