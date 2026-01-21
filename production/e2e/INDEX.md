# E2E Test Suite Index

Complete index of all E2E test files, utilities, and documentation for the Production MPC Wallet system.

## Directory Structure

```
production/e2e/
├── common/                           # Shared test utilities
│   ├── mod.rs                        # Module exports
│   ├── api_client.rs                 # HTTP client for node APIs
│   ├── assertions.rs                 # Custom test assertions
│   ├── cluster.rs                    # Cluster orchestration
│   ├── docker_compose.rs             # Docker Compose wrapper
│   └── helpers.rs                    # Test helper functions
├── cluster_setup.rs                  # Basic cluster tests (6 tests)
├── transaction_lifecycle.rs          # Transaction flow tests (6 tests)
├── byzantine_scenarios.rs            # Byzantine fault tests (6 tests)
├── fault_tolerance.rs                # Node failure tests (6 tests)
├── concurrency.rs                    # Concurrent operations (6 tests)
├── network_partition.rs              # Network partition tests (5 tests)
├── certificate_rotation.rs           # mTLS/certificate tests (7 tests)
├── benchmarks.rs                     # Performance benchmarks (5 tests)
├── docker-compose.e2e.yml            # E2E Docker Compose config
├── Cargo.toml                        # E2E package manifest
├── README.md                         # Comprehensive test documentation
├── QUICKSTART.md                     # Quick start guide
├── TESTING_STRATEGY.md               # Testing strategy and philosophy
├── run_e2e_tests.sh                  # Test runner script
├── .github_workflows_e2e.yml.example # CI/CD workflow example
└── INDEX.md                          # This file
```

## Test Files

### Core Test Suites

| File | Tests | Duration | Description |
|------|-------|----------|-------------|
| `cluster_setup.rs` | 6 | 5-10 min | Cluster orchestration, connectivity, health checks |
| `transaction_lifecycle.rs` | 6 | 10-15 min | Full transaction flow from submission to consensus |
| `byzantine_scenarios.rs` | 6 | 10-15 min | Byzantine fault detection and handling |
| `fault_tolerance.rs` | 6 | 15-20 min | Node failures and recovery scenarios |
| `concurrency.rs` | 6 | 10-15 min | Concurrent transaction handling |
| `network_partition.rs` | 5 | 10-15 min | Network partition and split-brain scenarios |
| `certificate_rotation.rs` | 7 | 10-15 min | mTLS connectivity and certificate validation |
| `benchmarks.rs` | 5 | 15-20 min | Performance benchmarks and metrics |

**Total**: 47 tests, ~90-120 minutes for full suite

## Common Utilities

### `common/api_client.rs`
**Purpose**: HTTP client for interacting with MPC wallet node APIs

**Key Types**:
- `ApiClient` - Main client struct
- `SendTransactionRequest` - Transaction submission
- `TransactionResponse` - Transaction details
- `HealthResponse` - Health check response
- `WalletBalanceResponse` - Balance information

**Methods**:
- `health_check()` - Check node health
- `send_transaction()` - Submit transaction
- `get_transaction()` - Retrieve transaction
- `list_transactions()` - List all transactions
- `get_wallet_balance()` - Get wallet balance
- `get_metrics()` - Fetch Prometheus metrics

### `common/docker_compose.rs`
**Purpose**: Docker Compose orchestration wrapper

**Key Type**:
- `DockerCompose` - Main orchestration struct

**Methods**:
- `up()` - Start all services
- `down()` - Stop and cleanup
- `kill(service)` - Simulate crash
- `restart(service)` - Restart service
- `pause(service)` / `unpause(service)` - Freeze simulation
- `disconnect_network()` / `connect_network()` - Network partitioning
- `logs(service)` - Get service logs
- `exec(service, command)` - Execute command in container

### `common/cluster.rs`
**Purpose**: High-level cluster management

**Key Types**:
- `Cluster` - Main cluster handle
- `PostgresClient` - Database client
- `EtcdClient` - etcd client
- `SignatureRecord` - Vote signature record
- `ByzantineViolationRecord` - Violation record
- `AuditLogRecord` - Audit log entry

**Methods**:
- `Cluster::start()` - Start full cluster
- `Cluster::stop()` - Clean shutdown
- `PostgresClient::query_signatures()` - Get transaction signatures
- `PostgresClient::query_byzantine_violations()` - Get violations
- `PostgresClient::query_transaction_state()` - Get TX state
- `PostgresClient::query_audit_log()` - Get audit logs
- `EtcdClient::get()` / `put()` - Key-value operations
- `EtcdClient::is_node_banned()` - Check ban status
- `EtcdClient::get_vote_count()` - Get vote counts

### `common/assertions.rs`
**Purpose**: Custom test assertions

**Functions**:
- `assert_transaction_state()` - Verify TX state
- `assert_in_range()` - Numeric range check
- `assert_threshold_reached()` - Consensus threshold
- `assert_node_banned()` - Ban status check
- `assert_contains()` - String contains
- `assert_metric_exists()` - Prometheus metric existence
- `assert_metric_value()` - Metric value check
- `assert_no_errors_in_logs()` - Log error check

### `common/helpers.rs`
**Purpose**: General test utilities

**Functions**:
- `retry_with_backoff()` - Retry with exponential backoff
- `wait_until()` - Wait for condition
- `generate_test_address()` - Bitcoin testnet address
- `parse_metric_value()` - Parse Prometheus metrics
- `calculate_percentile()` - Statistical percentile
- `test_id()` - Generate unique test ID

## Documentation Files

### `README.md`
**Comprehensive documentation** covering:
- Test overview and categories
- Prerequisites and setup
- Running tests (all methods)
- Test architecture
- Performance targets
- Debugging guide
- CI/CD integration
- Troubleshooting

### `QUICKSTART.md`
**5-minute quick start** covering:
- Minimal setup steps
- Fast verification test
- Common issues and solutions
- Test category overview
- Tips for faster testing

### `TESTING_STRATEGY.md`
**Testing philosophy and strategy** covering:
- Testing principles
- Coverage matrix
- Execution strategy
- Performance baselines
- Failure analysis
- Continuous improvement
- Future enhancements

### `INDEX.md` (This File)
**Complete reference** covering:
- Directory structure
- File descriptions
- API reference
- Test inventory
- Quick navigation

## Configuration Files

### `Cargo.toml`
E2E test package manifest with dependencies:
- `tokio` - Async runtime
- `reqwest` - HTTP client
- `serde` / `serde_json` - Serialization
- `tokio-postgres` - Database client
- `etcd-client` - etcd client
- `threshold-types` - Internal types
- `futures` - Async utilities
- `rand` - Random generation

### `docker-compose.e2e.yml`
Docker Compose configuration for E2E tests:
- **3 etcd nodes** (etcd-1, etcd-2, etcd-3)
- **1 PostgreSQL** instance
- **5 MPC nodes** (node-1 through node-5)
- **Networks**: `e2e-internal` (isolated)
- **Ports**: 8081-8085 (API), 9001-9005 (QUIC), 5432 (PostgreSQL), 2379 (etcd)
- **Health checks** for all services
- **Certificate mounting** via environment variable

### `run_e2e_tests.sh`
Test runner script features:
- Prerequisites checking
- Docker cleanup
- Test category execution
- Result reporting
- Colored output
- Error handling

## Test Inventory

### 1. Cluster Setup Tests (6 tests)

| Test Name | Duration | Description |
|-----------|----------|-------------|
| `test_cluster_startup_and_shutdown` | 1-2 min | Basic cluster lifecycle |
| `test_etcd_cluster_connectivity` | 1-2 min | etcd read/write operations |
| `test_postgres_connectivity` | 1-2 min | PostgreSQL connectivity |
| `test_inter_node_connectivity` | 2-3 min | Node-to-node communication |
| `test_cluster_health_endpoints` | 1-2 min | Health check verification |
| `test_rapid_cluster_restart` | 5-10 min | Restart idempotency |

### 2. Transaction Lifecycle Tests (6 tests)

| Test Name | Duration | Description |
|-----------|----------|-------------|
| `test_full_transaction_lifecycle` | 3-5 min | End-to-end transaction flow |
| `test_transaction_visibility_across_nodes` | 2-3 min | Cross-node visibility |
| `test_concurrent_transactions` | 2-3 min | 5 concurrent submissions |
| `test_transaction_state_transitions` | 3-5 min | State machine verification |
| `test_transaction_list_endpoint` | 2-3 min | List API functionality |

### 3. Byzantine Scenarios Tests (6 tests)

| Test Name | Duration | Description |
|-----------|----------|-------------|
| `test_double_vote_detection` | 2-3 min | Double-vote detection |
| `test_byzantine_violation_recording` | 2-3 min | Violation persistence |
| `test_node_continues_after_byzantine_detection` | 3-4 min | Cluster resilience |
| `test_malicious_node_isolation` | 2-3 min | Node banning |
| `test_byzantine_violation_types` | 2-3 min | Various violation types |
| `test_cluster_recovers_from_byzantine_attack` | 3-4 min | Recovery validation |

### 4. Fault Tolerance Tests (6 tests)

| Test Name | Duration | Description |
|-----------|----------|-------------|
| `test_node_failure_during_voting` | 3-5 min | Mid-voting failure |
| `test_node_recovery_after_restart` | 3-5 min | Restart and rejoin |
| `test_multiple_node_failures` | 5-7 min | Simultaneous failures |
| `test_sequential_node_failures` | 3-5 min | Sequential failures |
| `test_node_crash_and_state_sync` | 4-6 min | State synchronization |
| `test_rapid_node_restarts` | 5-7 min | Rapid restart cycles |

### 5. Concurrency Tests (6 tests)

| Test Name | Duration | Description |
|-----------|----------|-------------|
| `test_concurrent_transaction_submission` | 3-5 min | 10 concurrent TXs |
| `test_no_vote_conflicts_under_load` | 3-4 min | Vote conflict detection |
| `test_transaction_ordering_under_concurrency` | 2-3 min | Ordering verification |
| `test_mixed_load_different_nodes` | 3-4 min | Multi-node load |
| `test_sustained_transaction_throughput` | 30-60 sec | Throughput measurement |
| `test_concurrent_reads_and_writes` | 2-3 min | Read/write concurrency |

### 6. Network Partition Tests (5 tests)

| Test Name | Duration | Description |
|-----------|----------|-------------|
| `test_network_partition_majority_continues` | 3-5 min | Majority partition |
| `test_network_partition_healing` | 4-6 min | Partition healing |
| `test_minority_partition_cannot_progress` | 3-4 min | Minority partition |
| `test_split_brain_scenario` | 4-6 min | Split-brain (3 vs 2) |
| `test_cascading_partition_failures` | 3-5 min | Cascading failures |

### 7. Certificate/mTLS Tests (7 tests)

| Test Name | Duration | Description |
|-----------|----------|-------------|
| `test_basic_cluster_with_certificates` | 2-3 min | Basic mTLS operation |
| `test_node_restart_with_same_certificate` | 3-4 min | Certificate persistence |
| `test_all_nodes_communicate_via_mtls` | 3-4 min | mTLS verification |
| `test_certificate_validation` | 2-3 min | Validation enforcement |
| `test_mtls_under_load` | 3-4 min | mTLS performance |
| `test_quic_connection_reestablishment` | 3-4 min | Connection recovery |
| `test_certificate_based_node_identity` | 2-3 min | Identity verification |

### 8. Performance Benchmarks (5 tests)

| Test Name | Duration | Description |
|-----------|----------|-------------|
| `benchmark_transaction_throughput` | 60 sec | Throughput measurement |
| `benchmark_vote_propagation_latency` | 3-5 min | Propagation latency |
| `benchmark_consensus_completion_time` | 3-5 min | Consensus timing |
| `benchmark_api_response_times` | 2-3 min | API performance |
| `benchmark_concurrent_load` | 2-3 min | Concurrent load handling |

## Quick Navigation

### I want to...

**Run tests:**
- Quick smoke test → [QUICKSTART.md](QUICKSTART.md)
- Full test suite → [README.md](README.md#running-tests)
- Specific category → [README.md](README.md#run-specific-test-category)
- Single test → [README.md](README.md#run-single-test)

**Understand testing:**
- Testing philosophy → [TESTING_STRATEGY.md](TESTING_STRATEGY.md)
- Test architecture → [README.md](README.md#test-architecture)
- Coverage matrix → [TESTING_STRATEGY.md](TESTING_STRATEGY.md#test-coverage-matrix)

**Debug issues:**
- View logs → [README.md](README.md#view-logs)
- Inspect database → [README.md](README.md#inspect-database)
- Troubleshooting → [README.md](README.md#troubleshooting)
- Common issues → [QUICKSTART.md](QUICKSTART.md#common-issues)

**Contribute:**
- Add new tests → [README.md](README.md#contributing)
- Review process → [TESTING_STRATEGY.md](TESTING_STRATEGY.md#review-process)
- Test utilities → [This file](#common-utilities)

**CI/CD:**
- GitHub Actions → [.github_workflows_e2e.yml.example](.github_workflows_e2e.yml.example)
- Integration strategy → [TESTING_STRATEGY.md](TESTING_STRATEGY.md#cicd-integration)

## Statistics

- **Total test files**: 8
- **Total tests**: 47
- **Total utilities**: 6 modules
- **Documentation files**: 4
- **Lines of test code**: ~3,500
- **Test coverage**: ~90% of critical paths
- **Full suite duration**: 90-120 minutes
- **Smoke test duration**: 1-2 minutes

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-01-20 | Initial E2E test suite |

---

**Maintainer**: MPC Wallet Team
**Last Updated**: 2026-01-20
