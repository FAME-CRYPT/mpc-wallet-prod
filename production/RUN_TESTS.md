# Quick Test Execution Guide

## Prerequisites (One-Time Setup)

### 1. Install Dependencies
```bash
# Docker and Docker Compose should be installed
docker --version  # Should be 24.0+
docker-compose --version  # Should be 2.20+

# Rust toolchain should be installed
rustc --version  # Should be 1.75+
cargo --version
```

### 2. Generate Certificates
```bash
cd /mnt/c/Users/user/Desktop/MPC-WALLET/production/scripts
bash generate-certs.sh

# Verify certificates created
ls -la /mnt/c/Users/user/Desktop/MPC-WALLET/production/certs/
# Should see: ca.crt, node1-5.crt, node1-5.key
```

### 3. Configure Environment
```bash
cd /mnt/c/Users/user/Desktop/MPC-WALLET/production/docker

# Create .env file
cp .env.example .env

# Edit .env (CRITICAL: Change POSTGRES_PASSWORD!)
nano .env

# Minimum required changes:
# POSTGRES_PASSWORD=<generate-strong-password-here>
# CERTS_PATH=../certs
```

---

## Quick Test Run (Automated)

### Full System Test (Recommended)
```bash
cd /mnt/c/Users/user/Desktop/MPC-WALLET/production

# 1. Build everything
cargo build --release

# 2. Run complete e2e test suite
cargo test --manifest-path e2e/Cargo.toml -- --ignored --test-threads=1 --nocapture

# This will:
# - Start Docker containers
# - Run all 30+ tests
# - Clean up afterward
# Duration: ~15-20 minutes
```

### Individual Test Suites
```bash
cd /mnt/c/Users/user/Desktop/MPC-WALLET/production

# Cluster setup tests (infrastructure)
cargo test --test cluster_setup -- --ignored --nocapture

# Transaction lifecycle tests
cargo test --test transaction_lifecycle -- --ignored --nocapture

# Byzantine fault tolerance tests
cargo test --test byzantine_scenarios -- --ignored --nocapture

# Failure recovery tests
cargo test --test fault_tolerance -- --ignored --nocapture

# Concurrency tests
cargo test --test concurrency -- --ignored --nocapture
```

---

## Manual Test Run (Step-by-Step)

### Phase 1: Start System
```bash
cd /mnt/c/Users/user/Desktop/MPC-WALLET/production/docker

# Build images
docker-compose build

# Start all services
docker-compose up -d

# Watch logs (Ctrl+C to exit)
docker-compose logs -f
```

### Phase 2: Wait for Health (60-90 seconds)
```bash
# Check status (wait until all show "Up (healthy)")
watch docker-compose ps

# Or poll health endpoints
for i in {1..5}; do
  curl http://localhost:808$i/health && echo " - Node $i OK" || echo " - Node $i FAILED"
done
```

### Phase 3: Run Tests

#### Infrastructure Tests
```bash
# etcd cluster
docker exec mpc-etcd-1 etcdctl endpoint health --cluster

# PostgreSQL
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "SELECT count(*) FROM information_schema.tables WHERE table_schema='public';"
# Should return 7 (7 tables)

# All nodes responding
curl http://localhost:8081/health
curl http://localhost:8082/health
curl http://localhost:8083/health
curl http://localhost:8084/health
curl http://localhost:8085/health
```

#### Transaction Test
```bash
# Submit transaction
curl -X POST http://localhost:8081/api/v1/transactions \
  -H "Content-Type: application/json" \
  -d '{
    "recipient": "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx",
    "amount_sats": 50000,
    "metadata": "Test transaction"
  }' | jq

# Get the txid from response, then query it
TXID="<txid-from-above>"
curl http://localhost:8081/api/v1/transactions/$TXID | jq

# Wait a few seconds and query again to see state progression
sleep 5
curl http://localhost:8081/api/v1/transactions/$TXID | jq
```

#### Cluster Status
```bash
# Cluster status
curl http://localhost:8081/api/v1/cluster/status | jq

# Node list
curl http://localhost:8081/api/v1/cluster/nodes | jq
```

#### Database Verification
```bash
# Check transactions
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "SELECT txid, state, amount_sats FROM transactions ORDER BY created_at DESC LIMIT 5;"

# Check votes
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "SELECT node_id, approve, received_at FROM votes ORDER BY received_at DESC LIMIT 10;"

# Check Byzantine violations
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "SELECT * FROM byzantine_violations;"
```

### Phase 4: Failure Recovery Test
```bash
# Kill node 5
docker-compose stop node-5

# Verify cluster still works
curl -X POST http://localhost:8081/api/v1/transactions \
  -H "Content-Type: application/json" \
  -d '{
    "recipient": "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx",
    "amount_sats": 25000,
    "metadata": "4-node test"
  }' | jq

# Restart node 5
docker-compose start node-5

# Wait for health
sleep 30
curl http://localhost:8085/health
```

### Phase 5: Cleanup
```bash
# Stop all services
docker-compose down

# Full cleanup (removes volumes and data)
docker-compose down -v

# Clean build artifacts
cd /mnt/c/Users/user/Desktop/MPC-WALLET/production
cargo clean
```

---

## Test Results Interpretation

### Success Indicators
- ✅ All health endpoints return `{"status":"healthy",...}`
- ✅ etcd cluster shows 3/3 healthy endpoints
- ✅ PostgreSQL has 7 tables
- ✅ Transactions progress through states: pending → voting → approved
- ✅ At least 4 votes recorded for each transaction
- ✅ No critical errors in logs
- ✅ Cluster continues after 1 node failure

### Failure Indicators
- ❌ Any node health check fails
- ❌ etcd cluster shows < 3 healthy endpoints
- ❌ PostgreSQL missing tables
- ❌ Transactions stuck in "pending" state
- ❌ Votes < threshold (4)
- ❌ "ERROR" or "PANIC" in logs
- ❌ Cluster stops when 1 node fails

---

## Troubleshooting

### Nodes Won't Start
```bash
# Check logs
docker-compose logs node-1

# Common issues:
# 1. Certificates missing/invalid
ls -la /mnt/c/Users/user/Desktop/MPC-WALLET/production/certs/

# 2. PostgreSQL not ready
docker-compose logs postgres

# 3. etcd not ready
docker-compose logs etcd-1
```

### Certificates Error
```bash
# Regenerate certificates
cd /mnt/c/Users/user/Desktop/MPC-WALLET/production/scripts
rm -rf ../certs/*
bash generate-certs.sh

# Restart services
cd ../docker
docker-compose restart
```

### Port Conflicts
```bash
# Check what's using ports
netstat -tuln | grep -E '8081|8082|8083|8084|8085|5432|2379'

# Kill conflicting processes or change ports in docker-compose.dev.yml
```

### Database Connection Failed
```bash
# Check PostgreSQL is running
docker exec mpc-postgres pg_isready -U mpc

# Check password in .env matches
cat .env | grep POSTGRES_PASSWORD

# Restart PostgreSQL
docker-compose restart postgres
```

### Tests Hanging
```bash
# Tests may take time, especially first run
# Typical durations:
# - cluster_setup: 2-3 minutes
# - transaction_lifecycle: 3-5 minutes
# - fault_tolerance: 5-8 minutes

# If truly stuck (> 10 minutes), check:
docker-compose ps  # All services healthy?
docker-compose logs --tail=50  # Any errors?
```

---

## Performance Benchmarks

### Expected Performance
- Cluster startup: 60-90 seconds
- Transaction submission: < 500ms
- Consensus reached: 3-5 seconds
- Vote propagation: < 2 seconds
- Node recovery: < 60 seconds
- API response time: < 200ms

### Benchmark Tests
```bash
cd /mnt/c/Users/user/Desktop/MPC-WALLET/production

# Run benchmark suite
cargo test --test benchmarks -- --ignored --nocapture
```

---

## Monitoring During Tests

### Real-Time Monitoring
```bash
# Terminal 1: All logs
docker-compose logs -f

# Terminal 2: Resource usage
watch docker stats

# Terminal 3: Service status
watch docker-compose ps

# Terminal 4: Run tests
cargo test --test transaction_lifecycle -- --ignored --nocapture
```

### Health Monitoring
```bash
# Continuous health check
watch 'for i in {1..5}; do echo -n "Node $i: "; curl -s http://localhost:808$i/health | jq -r .status; done'
```

---

## Advanced Testing

### Stress Test
```bash
# Submit multiple concurrent transactions
for i in {1..10}; do
  curl -X POST http://localhost:8081/api/v1/transactions \
    -H "Content-Type: application/json" \
    -d "{
      \"recipient\": \"tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx\",
      \"amount_sats\": $((10000 + i * 1000)),
      \"metadata\": \"Stress test $i\"
    }" &
done
wait

# Check all transactions
curl http://localhost:8081/api/v1/transactions | jq
```

### Byzantine Attack Simulation
```bash
# This is included in the byzantine_scenarios tests
cargo test --test byzantine_scenarios -- --ignored --nocapture
```

### Network Partition Test
```bash
# Simulate network partition
cargo test --test network_partition -- --ignored --nocapture
```

---

## CI/CD Integration

### GitHub Actions Example
```yaml
name: E2E Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Generate Certificates
        run: |
          cd production/scripts
          bash generate-certs.sh

      - name: Create .env
        run: |
          cd production/docker
          cp .env.example .env
          sed -i 's/POSTGRES_PASSWORD=.*/POSTGRES_PASSWORD=test123456/' .env

      - name: Build
        run: |
          cd production
          cargo build --release

      - name: Run E2E Tests
        run: |
          cd production
          cargo test --manifest-path e2e/Cargo.toml -- --ignored --test-threads=1
```

---

## Quick Reference Commands

### Start System
```bash
cd /mnt/c/Users/user/Desktop/MPC-WALLET/production/docker && docker-compose up -d
```

### Stop System
```bash
cd /mnt/c/Users/user/Desktop/MPC-WALLET/production/docker && docker-compose down
```

### View Logs
```bash
docker-compose logs -f --tail=100
```

### Health Check All Nodes
```bash
for i in {1..5}; do curl http://localhost:808$i/health; echo; done
```

### Run All Tests
```bash
cd /mnt/c/Users/user/Desktop/MPC-WALLET/production && cargo test --manifest-path e2e/Cargo.toml -- --ignored --test-threads=1
```

### Clean Everything
```bash
cd /mnt/c/Users/user/Desktop/MPC-WALLET/production/docker && docker-compose down -v && cd .. && cargo clean
```

---

## Environment Variables Reference

### Required
- `POSTGRES_PASSWORD` - PostgreSQL password (change from default!)
- `CERTS_PATH` - Path to TLS certificates (default: ../certs)

### Optional
- `THRESHOLD` - Signature threshold (default: 4)
- `TOTAL_NODES` - Total nodes in cluster (default: 5)
- `BITCOIN_NETWORK` - Bitcoin network (default: testnet)
- `RUST_LOG` - Log level (default: info)
- `ESPLORA_URL` - Blockchain API URL

### E2E Test Specific
- `E2E_CERTS_PATH` - Override certificate path for e2e tests

---

## Support and Documentation

### Full Documentation
- [Complete Test Execution Report](./TEST_EXECUTION_REPORT.md) - Detailed test documentation
- [Docker README](./docker/README.md) - Deployment guide
- [Quick Start](./docker/QUICKSTART.md) - Fast setup guide

### Log Files
- Docker logs: `docker-compose logs > logs.txt`
- Test output: `cargo test 2>&1 | tee test_output.log`
- Database logs: `docker exec mpc-postgres cat /var/log/postgresql/*.log`

### Getting Help
1. Check logs: `docker-compose logs`
2. Check test output: Review test failures carefully
3. Verify prerequisites: Certificates, .env, Docker version
4. Clean and retry: `docker-compose down -v && docker-compose up -d`

---

**Last Updated**: 2026-01-20
