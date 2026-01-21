# E2E Tests Quick Start Guide

Get started with running E2E tests in under 5 minutes.

## Prerequisites

1. **Docker Desktop** (or Docker Engine + Docker Compose)
2. **Rust** (1.75+)
3. **Certificates** (generate or use existing)

## Setup (First Time Only)

### Step 1: Generate Certificates

```bash
cd production/certs
./generate_certs.sh
```

Or if you already have certificates, set the path:

```bash
export E2E_CERTS_PATH=/path/to/your/certs
```

### Step 2: Build the Docker Image

```bash
cd production
docker-compose -f e2e/docker-compose.e2e.yml build
```

This will take a few minutes the first time.

## Running Tests

### Option 1: Use the Test Runner Script (Recommended)

```bash
cd production/e2e
./run_e2e_tests.sh
```

Run specific test categories:

```bash
./run_e2e_tests.sh cluster_setup transaction_lifecycle
```

### Option 2: Use Cargo Directly

Run all tests:

```bash
cd production
cargo test --package e2e-tests -- --ignored --nocapture
```

Run a specific test category:

```bash
cargo test --package e2e-tests --test cluster_setup -- --ignored --nocapture
```

Run a single test:

```bash
cargo test --package e2e-tests --test cluster_setup test_cluster_startup_and_shutdown -- --ignored --nocapture
```

## Quick Test Verification

To verify your setup works, run the fastest test:

```bash
cargo test --package e2e-tests --test cluster_setup test_cluster_startup_and_shutdown -- --ignored --nocapture
```

This test should complete in 1-2 minutes. If it passes, your setup is correct!

## Expected Output

Successful test output looks like:

```
running 1 test
[INFO] Starting cluster for test: test-abc123
[INFO] Waiting for services to be healthy...
[INFO] etcd-1 is healthy
[INFO] etcd-2 is healthy
[INFO] etcd-3 is healthy
[INFO] PostgreSQL is healthy
[INFO] Node 1 is healthy
[INFO] Node 2 is healthy
[INFO] Node 3 is healthy
[INFO] Node 4 is healthy
[INFO] Node 5 is healthy
[INFO] All services are healthy
Node 1 is healthy: HealthResponse { status: "healthy", timestamp: ..., version: "0.1.0" }
Node 2 is healthy: HealthResponse { status: "healthy", timestamp: ..., version: "0.1.0" }
...
[INFO] Stopping cluster: test-abc123
[INFO] Cluster stopped successfully
Test passed: Cluster startup and shutdown
test test_cluster_startup_and_shutdown ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 5 filtered out
```

## Common Issues

### Issue: Port Already in Use

```bash
# Kill processes using the ports
lsof -i :8081 | grep LISTEN | awk '{print $2}' | xargs kill -9

# Or use the cleanup script
cd production/e2e
./run_e2e_tests.sh --cleanup
```

### Issue: Docker Permission Denied

```bash
# Add your user to docker group (Linux)
sudo usermod -aG docker $USER
newgrp docker
```

### Issue: Containers Not Starting

```bash
# Clean up Docker system
docker system prune -af --volumes

# Check Docker resources
docker info | grep -i memory
# Ensure you have at least 8GB available
```

### Issue: Test Hangs

- Check if Docker containers are running:
  ```bash
  docker ps | grep e2e-
  ```

- View logs:
  ```bash
  docker-compose -f production/e2e/docker-compose.e2e.yml logs
  ```

- Force cleanup:
  ```bash
  cd production/e2e
  ./run_e2e_tests.sh --cleanup
  ```

## Test Categories Overview

| Category | Duration | Description |
|----------|----------|-------------|
| `cluster_setup` | 5-10 min | Basic cluster health and connectivity |
| `transaction_lifecycle` | 10-15 min | Full transaction flow |
| `byzantine_scenarios` | 10-15 min | Byzantine fault detection |
| `fault_tolerance` | 15-20 min | Node failures and recovery |
| `concurrency` | 10-15 min | Concurrent operations |
| `network_partition` | 10-15 min | Network partition scenarios |
| `certificate_rotation` | 10-15 min | mTLS and certificate tests |
| `benchmarks` | 15-20 min | Performance benchmarks |

**Total time for all tests**: ~90-120 minutes

## Tips for Faster Testing

1. **Run specific tests** instead of the full suite during development:
   ```bash
   cargo test --package e2e-tests --test cluster_setup -- --ignored
   ```

2. **Use Docker layer caching** - avoid rebuilding images:
   ```bash
   # Only rebuild when source changes
   docker-compose -f e2e/docker-compose.e2e.yml build --no-cache node-1
   ```

3. **Run tests sequentially** to avoid resource conflicts:
   ```bash
   cargo test --package e2e-tests -- --ignored --test-threads=1
   ```

4. **Keep Docker Desktop running** with adequate resources:
   - Memory: 8GB minimum, 16GB recommended
   - CPUs: 4 cores minimum, 8 cores recommended

## Next Steps

1. **Explore test code**: Check `production/e2e/` for test implementations
2. **Read README**: See `production/e2e/README.md` for detailed documentation
3. **Add custom tests**: Use existing tests as templates
4. **Check metrics**: Run benchmarks to establish performance baselines

## Getting Help

- **View logs**: `docker-compose -f e2e/docker-compose.e2e.yml logs -f`
- **Inspect database**: `docker exec -it e2e-postgres psql -U mpc -d mpc_wallet`
- **Check etcd**: `docker exec -it e2e-etcd-1 etcdctl get --prefix /mpc`
- **Debug with verbose logging**: `RUST_LOG=debug cargo test ...`

## Cleanup

Always clean up after tests:

```bash
cd production/e2e
./run_e2e_tests.sh --cleanup
```

Or manually:

```bash
docker ps -a | grep e2e- | awk '{print $1}' | xargs docker stop
docker ps -a | grep e2e- | awk '{print $1}' | xargs docker rm
docker network ls | grep e2e- | awk '{print $1}' | xargs docker network rm
```

---

Happy testing! 🚀
