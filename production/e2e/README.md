# End-to-End (E2E) Tests for Production MPC Wallet

Comprehensive E2E tests for the 5-node distributed threshold wallet system testing the complete multi-node cluster in realistic deployment scenarios.

## Overview

This test suite validates:
- **5-node MPC cluster** with 4-of-5 threshold
- **QUIC+mTLS** networking between nodes
- **Byzantine fault tolerance** and detection
- **etcd cluster** (3 nodes) for distributed coordination
- **PostgreSQL** for audit logs and transaction storage
- **Complete transaction lifecycle** from proposal to signing

## Test Categories

### 1. Cluster Setup (`cluster_setup.rs`)
Basic cluster orchestration and connectivity tests:
- Cluster startup and shutdown
- etcd cluster connectivity
- PostgreSQL connectivity
- Inter-node connectivity
- Health endpoint verification
- Rapid cluster restart (idempotency)

```bash
cargo test --package e2e-tests --test cluster_setup -- --ignored --nocapture
```

### 2. Transaction Lifecycle (`transaction_lifecycle.rs`)
Full transaction flow from submission to consensus:
- End-to-end transaction processing
- Vote propagation across all nodes
- Consensus reaching (threshold)
- Transaction state transitions
- Signature generation and verification
- Transaction visibility across nodes
- Concurrent transaction handling
- Audit log verification

```bash
cargo test --package e2e-tests --test transaction_lifecycle -- --ignored --nocapture
```

### 3. Byzantine Scenarios (`byzantine_scenarios.rs`)
Byzantine fault tolerance and detection:
- Double vote detection
- Invalid signature detection
- Byzantine violation recording
- Node banning mechanism
- Cluster recovery after Byzantine attack
- Minority vote attack detection
- Malicious node isolation

```bash
cargo test --package e2e-tests --test byzantine_scenarios -- --ignored --nocapture
```

### 4. Fault Tolerance (`fault_tolerance.rs`)
Node failure and recovery scenarios:
- Node failure during voting
- Node recovery after restart
- Multiple simultaneous node failures
- Sequential node failures
- State synchronization after crash
- Rapid node restart tolerance
- Threshold enforcement (below 4 nodes)

```bash
cargo test --package e2e-tests --test fault_tolerance -- --ignored --nocapture
```

### 5. Concurrency (`concurrency.rs`)
Concurrent operations and race conditions:
- Concurrent transaction submission (10+ transactions)
- Vote conflict detection under load
- Transaction ordering verification
- Mixed load across different nodes
- Sustained throughput testing
- Concurrent reads and writes
- Presignature pool management under load

```bash
cargo test --package e2e-tests --test concurrency -- --ignored --nocapture
```

### 6. Network Partition (`network_partition.rs`)
Network partition scenarios (split-brain):
- Majority partition continues operation
- Minority partition cannot progress
- Network partition healing and resync
- Split-brain scenario (3 vs 2 nodes)
- Cascading partition failures
- Data consistency after healing

```bash
cargo test --package e2e-tests --test network_partition -- --ignored --nocapture
```

### 7. Certificate Rotation (`certificate_rotation.rs`)
mTLS certificate management:
- Basic mTLS cluster operation
- Node restart with same certificate
- All nodes communicate via mTLS
- Certificate validation enforcement
- mTLS performance under load
- QUIC connection reestablishment
- Certificate-based node identity

```bash
cargo test --package e2e-tests --test certificate_rotation -- --ignored --nocapture
```

### 8. Performance Benchmarks (`benchmarks.rs`)
Performance metrics and benchmarks:
- Transaction throughput (tx/sec)
- Vote propagation latency (p50, p95, p99)
- Consensus completion time
- API response times (GET, LIST)
- Concurrent load handling
- Sustained throughput measurement

```bash
cargo test --package e2e-tests --test benchmarks -- --ignored --nocapture
```

## Prerequisites

### Required Software
- **Docker** and **Docker Compose**
- **Rust** toolchain (1.75+)
- **PostgreSQL client** tools (optional, for manual inspection)
- **etcdctl** (optional, for manual inspection)

### Certificate Generation
Before running E2E tests, generate test certificates:

```bash
cd production/certs
./generate_certs.sh
```

Or use the provided test certificates in `production/certs/`.

### Environment Setup
Set the certificate path:

```bash
export E2E_CERTS_PATH=/absolute/path/to/production/certs
```

## Running Tests

### Run All E2E Tests
```bash
# Run all tests (takes 30-60 minutes)
cargo test --package e2e-tests -- --ignored --nocapture

# Run with debug logging
RUST_LOG=debug cargo test --package e2e-tests -- --ignored --nocapture
```

### Run Specific Test Category
```bash
# Run only cluster setup tests
cargo test --package e2e-tests --test cluster_setup -- --ignored --nocapture

# Run only transaction lifecycle tests
cargo test --package e2e-tests --test transaction_lifecycle -- --ignored --nocapture

# Run only Byzantine tests
cargo test --package e2e-tests --test byzantine_scenarios -- --ignored --nocapture
```

### Run Single Test
```bash
# Run a specific test by name
cargo test --package e2e-tests --test cluster_setup test_cluster_startup_and_shutdown -- --ignored --nocapture
```

### Parallel vs Sequential Execution
By default, tests run sequentially to avoid Docker conflicts. For parallel execution:

```bash
# Run up to 4 tests in parallel (use with caution)
cargo test --package e2e-tests -- --ignored --nocapture --test-threads=4
```

## Test Architecture

### Cluster Orchestration
Each test:
1. **Starts** a fresh Docker Compose cluster with unique project name
2. **Waits** for all services (etcd, PostgreSQL, 5 MPC nodes) to be healthy
3. **Executes** the test scenario
4. **Cleans up** by stopping and removing all containers

### Infrastructure Stack
- **etcd cluster**: 3 nodes on ports 2379-2381
- **PostgreSQL**: Single instance on port 5432
- **MPC nodes**: 5 nodes on ports 8081-8085 (HTTP), 9001-9005 (QUIC)

### Test Utilities

#### `common/api_client.rs`
HTTP client for node APIs:
- `send_transaction()`
- `get_transaction()`
- `list_transactions()`
- `get_wallet_balance()`
- `health_check()`

#### `common/docker_compose.rs`
Docker orchestration:
- `up()` - Start services
- `down()` - Stop and clean up
- `kill()` - Simulate crash
- `restart()` - Restart service
- `pause()`/`unpause()` - Simulate freeze
- `disconnect_network()`/`connect_network()` - Network partition

#### `common/cluster.rs`
Cluster management:
- `Cluster::start()` - Start full cluster
- `Cluster::stop()` - Clean shutdown
- `wait_for_health()` - Health polling
- `PostgresClient` - Database queries
- `EtcdClient` - etcd operations

#### `common/helpers.rs`
Test utilities:
- `retry_with_backoff()` - Retry logic
- `wait_until()` - Condition waiting
- `generate_test_address()` - Bitcoin address generation
- `calculate_percentile()` - Statistics

## Performance Targets

Based on the `benchmarks.rs` test suite:

| Metric | Target | P95 Target |
|--------|--------|------------|
| Transaction Throughput | >5 tx/sec | - |
| API Response Time (GET) | <100ms | <200ms |
| Vote Propagation | <1s | <2s |
| Consensus Completion | <3s | <5s |
| Concurrent Load | 20+ requests | - |

## Debugging Failed Tests

### View Logs
```bash
# View logs for specific service
docker-compose -f production/e2e/docker-compose.e2e.yml -p e2e-test logs node-1

# Follow logs in real-time
docker-compose -f production/e2e/docker-compose.e2e.yml -p e2e-test logs -f

# View all service logs
docker-compose -f production/e2e/docker-compose.e2e.yml -p e2e-test logs
```

### Inspect Database
```bash
# Connect to PostgreSQL
docker exec -it e2e-postgres psql -U mpc -d mpc_wallet

# Query transactions
SELECT * FROM transactions;

# Query Byzantine violations
SELECT * FROM byzantine_violations;

# Query votes
SELECT * FROM votes;
```

### Inspect etcd
```bash
# List all keys
docker exec -it e2e-etcd-1 etcdctl get --prefix /mpc

# Get specific key
docker exec -it e2e-etcd-1 etcdctl get /mpc/transactions/txid123
```

### Manual Cleanup
If tests fail to clean up:

```bash
# Stop all E2E containers
docker ps -a | grep e2e- | awk '{print $1}' | xargs docker stop
docker ps -a | grep e2e- | awk '{print $1}' | xargs docker rm

# Remove E2E networks
docker network ls | grep e2e- | awk '{print $1}' | xargs docker network rm

# Remove E2E volumes
docker volume ls | grep e2e- | awk '{print $2}' | xargs docker volume rm
```

## CI/CD Integration

### GitHub Actions Example
```yaml
name: E2E Tests

on: [push, pull_request]

jobs:
  e2e:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Generate certificates
        run: cd production/certs && ./generate_certs.sh

      - name: Run E2E tests
        run: |
          cd production
          cargo test --package e2e-tests --test cluster_setup -- --ignored
          cargo test --package e2e-tests --test transaction_lifecycle -- --ignored
        env:
          E2E_CERTS_PATH: ${{ github.workspace }}/production/certs
```

## Test Coverage

Current coverage includes:
- ✅ Cluster orchestration and health
- ✅ Transaction submission and retrieval
- ✅ Vote propagation and consensus
- ✅ Node failures and recovery
- ✅ Network partitions
- ✅ Byzantine fault detection
- ✅ Concurrent operations
- ✅ mTLS/QUIC connectivity
- ✅ Performance benchmarking

## Known Limitations

1. **Certificate Rotation**: Tests verify certificates work but don't test live rotation (requires application support)
2. **Presignature Pool**: Tests don't directly verify presignature generation (requires protocol-level instrumentation)
3. **Bitcoin Broadcasting**: Tests mock Bitcoin network (testnet addresses used but not broadcast)
4. **Long-running Tests**: Some tests take several minutes to complete

## Contributing

When adding new E2E tests:

1. **Use the `#[ignore]` attribute** for expensive tests
2. **Create unique test IDs** with `helpers::test_id()`
3. **Clean up resources** in all code paths
4. **Add clear logging** for debugging
5. **Document expected behavior** in test comments
6. **Use assertion helpers** from `common/assertions.rs`

## Troubleshooting

### Port Already in Use
```bash
# Check what's using port 8081
lsof -i :8081

# Kill the process
kill -9 <PID>
```

### Docker Out of Resources
```bash
# Clean up Docker system
docker system prune -af --volumes

# Increase Docker resources (Docker Desktop)
# Settings → Resources → Increase memory to 8GB+
```

### Tests Hanging
- Check Docker Compose logs for stuck services
- Verify health checks are passing
- Increase timeout values in `common/cluster.rs`

### Flaky Tests
- Network partitioning tests may be timing-sensitive
- Increase wait times in `helpers::wait_until()`
- Run tests sequentially with `--test-threads=1`

## License

MIT License - See LICENSE file for details
