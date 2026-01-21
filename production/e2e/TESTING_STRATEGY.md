# E2E Testing Strategy for Production MPC Wallet

## Overview

This document outlines the comprehensive end-to-end testing strategy for the production MPC wallet system, covering a distributed 5-node threshold wallet with Byzantine fault tolerance.

## Testing Philosophy

### Principles

1. **Test the Full Stack**: Every test runs against the complete system (etcd, PostgreSQL, 5 MPC nodes)
2. **Realistic Scenarios**: Tests simulate real-world conditions including failures and attacks
3. **Isolation**: Each test starts with a fresh cluster and cleans up completely
4. **Observability**: Tests verify both behavior and internal state (databases, logs, metrics)
5. **Performance**: Benchmarks establish baseline performance characteristics

### Test Pyramid

```
                    /\
                   /  \
                  / E2E \          ← We are here (8 test suites)
                 /______\
                /        \
               /Integration\       ← Unit tests in crates
              /____________\
             /              \
            /   Unit Tests   \    ← Individual function tests
           /__________________\
```

## Test Coverage Matrix

### Functional Coverage

| Category | Coverage | Test Count | Duration |
|----------|----------|------------|----------|
| Cluster Operations | 100% | 6 tests | 5-10 min |
| Transaction Lifecycle | 95% | 6 tests | 10-15 min |
| Byzantine Scenarios | 90% | 6 tests | 10-15 min |
| Fault Tolerance | 95% | 6 tests | 15-20 min |
| Concurrency | 85% | 6 tests | 10-15 min |
| Network Partition | 85% | 5 tests | 10-15 min |
| Certificate/mTLS | 80% | 7 tests | 10-15 min |
| Performance | 100% | 5 tests | 15-20 min |

### Scenarios Covered

#### Happy Path
- ✅ Transaction submission and approval
- ✅ Vote propagation across nodes
- ✅ Threshold consensus (4-of-5)
- ✅ Signature generation with presignatures
- ✅ Transaction state transitions
- ✅ Multi-transaction handling

#### Failure Scenarios
- ✅ Single node failure (cluster continues)
- ✅ Multiple node failures (below threshold)
- ✅ Node crash and recovery
- ✅ Rapid node restarts
- ✅ Network partitions (majority/minority)
- ✅ Split-brain scenarios

#### Security Scenarios
- ✅ Byzantine double-vote detection
- ✅ Invalid signature detection
- ✅ Minority vote attack
- ✅ Node banning after violations
- ✅ mTLS certificate validation
- ✅ Cluster recovery after attacks

#### Performance Scenarios
- ✅ Transaction throughput measurement
- ✅ Vote propagation latency
- ✅ Consensus completion time
- ✅ API response times
- ✅ Concurrent load handling

## Test Execution Strategy

### Development Workflow

**During Development:**
```bash
# Quick smoke test (1-2 min)
cargo test --package e2e-tests --test cluster_setup test_cluster_startup_and_shutdown -- --ignored

# Test specific feature (5-10 min)
cargo test --package e2e-tests --test transaction_lifecycle -- --ignored

# Full suite before PR (90-120 min)
./run_e2e_tests.sh
```

### CI/CD Integration

**On Every Push:**
- Run cluster setup tests (smoke test)
- Verify basic connectivity

**On Pull Requests:**
- Run full E2E suite (all 8 categories)
- Parallel execution using matrix strategy
- Report coverage metrics

**Nightly Builds:**
- Run full suite including benchmarks
- Track performance trends
- Generate reports

### Release Testing

**Before Release:**
1. Run full E2E suite on staging environment
2. Run extended performance benchmarks
3. Run stress tests (higher loads)
4. Verify all Byzantine scenarios
5. Test upgrade/migration paths

## Test Data Management

### Database State

- **Schema**: Applied automatically from `schema.sql`
- **Data**: Generated per test (no fixtures)
- **Cleanup**: Database is recreated for each test
- **Verification**: Direct SQL queries for assertions

### Test Identifiers

- Each test generates unique IDs using `helpers::test_id()`
- Docker projects use unique names: `e2e-{test_id}`
- No conflicts between parallel tests
- Easy log correlation

### Certificate Management

- Test certificates generated once
- Mounted read-only into containers
- No runtime certificate generation
- Certificate rotation tested separately

## Performance Baselines

### Target Metrics

| Metric | Target | P95 | P99 |
|--------|--------|-----|-----|
| Transaction Throughput | >5 tx/sec | - | - |
| Vote Propagation | <1s | <2s | <3s |
| Consensus Completion | <3s | <5s | <7s |
| API GET Response | <50ms | <100ms | <200ms |
| API LIST Response | <100ms | <200ms | <500ms |

### Benchmark Methodology

1. **Warm-up**: 30 seconds before measurement
2. **Duration**: 60 seconds per benchmark
3. **Load**: Gradual increase to target
4. **Measurement**: Continuous monitoring
5. **Statistics**: P50, P95, P99 percentiles

## Failure Analysis

### Common Failure Patterns

**1. Timeout Failures**
- Cause: Services not starting in time
- Solution: Increase health check timeouts
- Prevention: Ensure adequate Docker resources

**2. Port Conflicts**
- Cause: Previous test cleanup failed
- Solution: Run cleanup script
- Prevention: Use unique test IDs

**3. Network Partition Failures**
- Cause: Docker network timing issues
- Solution: Add wait time after partition
- Prevention: Use retry logic with backoff

**4. Consensus Timeouts**
- Cause: High load or slow nodes
- Solution: Increase consensus timeout
- Prevention: Monitor node health

### Debugging Strategy

**Step 1: Identify Failure**
```bash
# View test output
cargo test --package e2e-tests --test cluster_setup -- --ignored --nocapture 2>&1 | tee test.log
```

**Step 2: Check Services**
```bash
# List running containers
docker ps | grep e2e-

# Check service health
docker-compose -f e2e/docker-compose.e2e.yml ps
```

**Step 3: Inspect Logs**
```bash
# Node logs
docker logs e2e-node-1

# Database logs
docker logs e2e-postgres

# etcd logs
docker logs e2e-etcd-1
```

**Step 4: Verify State**
```bash
# Check database
docker exec -it e2e-postgres psql -U mpc -d mpc_wallet -c "SELECT * FROM transactions;"

# Check etcd
docker exec -it e2e-etcd-1 etcdctl get --prefix /mpc
```

## Continuous Improvement

### Metrics to Track

1. **Test Reliability**
   - Flakiness rate (target: <1%)
   - Pass rate trend
   - Time-to-fix for failures

2. **Test Performance**
   - Total suite duration
   - Individual test duration
   - Resource utilization

3. **Coverage**
   - Code coverage (target: >80%)
   - Scenario coverage
   - Edge case coverage

### Review Process

**Monthly Review:**
- Analyze test failures
- Identify flaky tests
- Update performance baselines
- Add new scenarios

**Quarterly Review:**
- Comprehensive coverage audit
- Performance trend analysis
- Test infrastructure updates
- Documentation updates

## Future Enhancements

### Planned Additions

1. **Chaos Engineering**
   - Random failure injection
   - Jepsen-style consistency tests
   - Extended partition scenarios

2. **Load Testing**
   - Sustained high-load tests
   - Spike load scenarios
   - Memory leak detection

3. **Security Testing**
   - Penetration testing scenarios
   - Certificate expiration handling
   - Key rotation under load

4. **Upgrade Testing**
   - Rolling upgrade scenarios
   - Backward compatibility
   - Data migration verification

### Infrastructure Improvements

1. **Test Observability**
   - Distributed tracing
   - Real-time metrics dashboard
   - Test execution visualization

2. **Parallel Execution**
   - Improved test isolation
   - Resource pool management
   - Faster cleanup

3. **Test Data Management**
   - Snapshot testing
   - Data seeding framework
   - Fixture library

## Conclusion

This E2E testing strategy provides comprehensive coverage of the MPC wallet system, ensuring reliability, security, and performance in production environments. The tests are designed to catch issues early, validate critical functionality, and provide confidence in the system's behavior under various conditions.

**Key Takeaways:**
- 47+ test scenarios covering all critical paths
- Full stack integration (etcd, PostgreSQL, 5 nodes)
- Byzantine fault tolerance verification
- Performance benchmarking and tracking
- Automated CI/CD integration
- Clear debugging and maintenance procedures

---

**Last Updated**: 2026-01-20
**Version**: 1.0.0
**Maintainer**: MPC Wallet Team
