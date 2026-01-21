# MPC Wallet Monitoring Stack

Complete Prometheus and Grafana monitoring solution for the MPC Wallet production system.

## Overview

This monitoring stack provides comprehensive observability for a distributed threshold signature wallet system with:

- **5 MPC nodes** running REST APIs with threshold signatures
- **Byzantine consensus** with voting and fault detection
- **QUIC+mTLS networking** for secure peer-to-peer communication
- **PostgreSQL** for transaction storage and audit logs
- **etcd cluster** for distributed coordination

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Grafana (Port 3000)                     │
│                    Visualization Layer                       │
└───────────────────────────┬─────────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────────┐
│                   Prometheus (Port 9090)                     │
│                    Metrics Collection                        │
└─┬─────────┬──────────┬──────────┬──────────┬───────────────┘
  │         │          │          │          │
  │         │          │          │          │
┌─▼─┐    ┌─▼──┐    ┌──▼───┐   ┌─▼────┐  ┌──▼─────┐
│N-1│    │N-2 │    │ N-3  │   │Postgr│  │  etcd  │
│:80│    │:80 │    │ :80  │   │:9187 │  │ :2379  │
└───┘    └────┘    └──────┘   └──────┘  └────────┘
  │         │          │          │          │
┌─▼─────────▼──────────▼──────────▼──────────▼───────┐
│              cAdvisor (Port 8081)                   │
│            Container Resource Metrics               │
└─────────────────────────────────────────────────────┘
```

## Components

### 1. Prometheus
- **Port**: 9090
- **Purpose**: Time-series metrics collection and storage
- **Retention**: 30 days
- **Scrape interval**: 15s (10s for MPC nodes)

### 2. Grafana
- **Port**: 3000
- **Purpose**: Metrics visualization and dashboarding
- **Default credentials**: admin / (set via GRAFANA_ADMIN_PASSWORD)

### 3. PostgreSQL Exporter
- **Port**: 9187
- **Purpose**: Export PostgreSQL database metrics

### 4. cAdvisor
- **Port**: 8081
- **Purpose**: Container resource usage metrics (CPU, memory, network, I/O)

### 5. Node Exporter
- **Port**: 9100
- **Purpose**: Host system metrics (optional)

## Quick Start

### Prerequisites

1. Docker and Docker Compose installed
2. Main MPC wallet stack running (see `production/docker/docker-compose.yml`)
3. Environment variables configured

### Environment Variables

Create a `.env` file in the `production/monitoring/` directory:

```bash
# Grafana admin credentials
GRAFANA_ADMIN_USER=admin
GRAFANA_ADMIN_PASSWORD=your_secure_password_here

# PostgreSQL connection (same as main stack)
POSTGRES_USER=mpc
POSTGRES_PASSWORD=your_postgres_password
POSTGRES_DB=mpc_wallet
```

### Starting the Monitoring Stack

```bash
# Navigate to monitoring directory
cd production/monitoring

# Start all monitoring services
docker-compose -f docker-compose.monitoring.yml up -d

# Check service health
docker-compose -f docker-compose.monitoring.yml ps

# View logs
docker-compose -f docker-compose.monitoring.yml logs -f
```

### Accessing Services

- **Grafana**: http://localhost:3000
- **Prometheus**: http://localhost:9090
- **cAdvisor**: http://localhost:8081

## Grafana Dashboards

The stack includes 5 pre-built dashboards:

### 1. MPC Cluster Overview
**UID**: `mpc-cluster-overview`

Key panels:
- Active nodes count
- Byzantine violations counter
- Transaction queue size
- Transaction success rate gauge
- Node health matrix (up/down status)
- Transaction throughput by state
- Connected peers per node
- CPU and memory usage

**Use for**: Overall cluster health monitoring, quick system status check

### 2. Byzantine Consensus Monitoring
**UID**: `byzantine-consensus`

Key panels:
- Consensus threshold indicator
- Byzantine violations (5m window)
- Votes per second
- Vote approval rate
- Votes per node vs threshold
- Vote results by node (approve/reject)
- Byzantine violation types (pie chart)
- Violations per node
- Vote processing latency (p50, p95, p99)
- Consensus round duration
- Violation detail table

**Use for**: Detecting Byzantine faults, monitoring consensus health, investigating voting anomalies

### 3. Signature Performance
**UID**: `signature-performance`

Key panels:
- Signatures per second
- Signature success rate
- Average presignature pool size
- Signature duration (p95)
- Signature duration percentiles by protocol (CGGMP24, FROST)
- Signature throughput by protocol
- Presignature pool size per node
- DKG ceremony duration
- Protocol usage distribution
- Signature failures by protocol
- Protocol performance summary table

**Use for**: Optimizing signature generation, monitoring DKG ceremonies, tracking presignature pools

### 4. Infrastructure Monitoring
**UID**: `infrastructure`

Key panels:

**etcd Section**:
- Leader status
- Nodes up count
- Failed proposals
- WAL fsync latency
- Proposals committed rate
- Peer round trip time

**PostgreSQL Section**:
- Database status
- Active connections
- Deadlocks counter
- Transactions per second
- Database operations (inserts, updates, deletes)
- Query performance

**Container Resources Section**:
- CPU usage per container
- Memory usage per container
- Network I/O
- Disk I/O

**Use for**: Infrastructure health checks, database performance tuning, resource capacity planning

### 5. Network Monitoring
**UID**: `network-monitoring`

Key panels:
- Average connected peers
- Network errors per second
- TLS handshake failures
- Total network RX bandwidth
- Connected peers per node
- Network bandwidth per node (RX/TX)
- Network errors per node
- TLS handshake failures per node
- Network packet rate
- Network errors and drops
- Network status summary table

**Use for**: Diagnosing connectivity issues, monitoring QUIC performance, detecting network attacks

## Metrics Reference

### MPC Node Metrics

All MPC nodes expose metrics on port 8080 at `/metrics` endpoint.

#### Counters

```prometheus
# Total transactions by state
mpc_transactions_total{state="pending|approved|rejected|failed", node_id="1"}

# Total votes cast
mpc_votes_total{node_id="1", result="approve|reject"}

# Byzantine violations detected
mpc_byzantine_violations_total{type="double_vote|invalid_signature|timeout", node_id="1"}

# Signatures generated
mpc_signatures_total{protocol="cggmp24|frost", result="success|failure", node_id="1"}

# Network errors
mpc_network_errors_total{node_id="1", error_type="connection|timeout|handshake"}

# TLS handshake failures
mpc_tls_handshake_failures_total{node_id="1"}
```

#### Gauges

```prometheus
# Number of active nodes in cluster
mpc_active_nodes{cluster="mpc-wallet"}

# Required votes for consensus
mpc_consensus_threshold{cluster="mpc-wallet"}

# Available presignatures
mpc_presignature_pool_size{node_id="1", protocol="cggmp24|frost"}

# Connected peer count
mpc_connected_peers{node_id="1"}

# Pending transactions
mpc_transaction_queue_size{node_id="1"}
```

#### Histograms

```prometheus
# Signature generation duration
mpc_signature_duration_seconds{protocol="cggmp24|frost", node_id="1"}

# DKG ceremony duration
mpc_dkg_duration_seconds{protocol="cggmp24|frost", node_id="1"}

# Vote processing time
mpc_vote_processing_duration_seconds{node_id="1"}

# Consensus round duration
mpc_consensus_round_duration_seconds{node_id="1"}
```

### PostgreSQL Metrics

```prometheus
# Active connections
pg_stat_activity_count

# Transactions committed/rolled back
pg_stat_database_xact_commit
pg_stat_database_xact_rollback

# Tuples operations
pg_stat_database_tup_inserted
pg_stat_database_tup_updated
pg_stat_database_tup_deleted

# Deadlocks
pg_stat_database_deadlocks

# Query performance
pg_stat_statements_mean_time_seconds
```

### etcd Metrics

```prometheus
# Leader election
etcd_server_has_leader

# Proposals
etcd_server_proposals_committed_total
etcd_server_proposals_failed_total

# Disk performance
etcd_disk_wal_fsync_duration_seconds

# Network latency
etcd_network_peer_round_trip_time_seconds
```

### Container Metrics (cAdvisor)

```prometheus
# CPU usage
container_cpu_usage_seconds_total{name="mpc-node-1"}

# Memory usage
container_memory_usage_bytes{name="mpc-node-1"}

# Network I/O
container_network_receive_bytes_total{name="mpc-node-1"}
container_network_transmit_bytes_total{name="mpc-node-1"}

# Disk I/O
container_fs_reads_bytes_total{name="mpc-node-1"}
container_fs_writes_bytes_total{name="mpc-node-1"}
```

## Alert Rules

### Critical Alerts

1. **MPCNodeDown**: Node unreachable for 1+ minute
2. **MPCClusterDegraded**: Less than 4 nodes available
3. **ByzantineViolationDetected**: Any Byzantine fault detected
4. **ByzantineDoubleVoteDetected**: Double voting attempt
5. **PresignaturePoolDepleted**: No presignatures available
6. **EtcdNoLeader**: etcd cluster has no leader
7. **PostgresDown**: Database unreachable
8. **NoPeerConnectivity**: Node has zero connected peers

### Warning Alerts

1. **MPCNodeHighMemoryUsage**: >85% memory usage for 5+ minutes
2. **MPCNodeHighCPUUsage**: >90% CPU usage for 10+ minutes
3. **ConsensusThresholdNotReached**: Insufficient votes for 5+ minutes
4. **HighTransactionFailureRate**: >10% failure rate
5. **SlowSignatureGeneration**: p95 latency >10s
6. **PresignaturePoolLow**: <10 presignatures available
7. **LowPeerConnectivity**: <3 connected peers
8. **TLSHandshakeFailures**: >0.5 failures/second

### Info Alerts

1. **NoTransactionsProcessed**: No activity for 15+ minutes (may be normal)

## Adding Custom Metrics

### 1. In Your Rust Code

```rust
use crate::metrics;

// Initialize metrics on startup
metrics::initialize_metrics("node-1", 4, 5);

// Record a transaction
metrics::record_transaction("node-1", "approved");

// Record a vote
metrics::record_vote("node-1", "approve");

// Record a signature with timing
let start = std::time::Instant::now();
// ... signature generation ...
let duration = start.elapsed().as_secs_f64();
metrics::record_signature("node-1", "cggmp24", "success", duration);

// Update gauge metrics
metrics::update_connected_peers("node-1", 4);
metrics::update_presignature_pool_size("node-1", "frost", 25);
```

### 2. Update Prometheus Config

Add new scrape targets or relabel configs in `prometheus/prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'my-custom-service'
    static_configs:
      - targets: ['my-service:9090']
        labels:
          service: 'custom'
```

### 3. Create Grafana Panel

1. Open Grafana at http://localhost:3000
2. Navigate to dashboard
3. Click "Add panel"
4. Enter PromQL query:
   ```promql
   rate(my_custom_metric[5m])
   ```
5. Configure visualization and save

## Troubleshooting

### Metrics Not Appearing

1. **Check Prometheus targets**:
   - Visit http://localhost:9090/targets
   - Ensure all targets show "UP" status
   - Check for scrape errors

2. **Verify metrics endpoint**:
   ```bash
   # Test MPC node metrics
   curl http://localhost:8080/metrics

   # Test PostgreSQL exporter
   curl http://localhost:9187/metrics
   ```

3. **Check network connectivity**:
   ```bash
   # Ensure monitoring network can reach MPC network
   docker network inspect docker_mpc-internal
   docker network inspect monitoring_monitoring
   ```

### Grafana Dashboard Not Loading

1. **Check datasource**:
   - Settings → Data Sources → Prometheus
   - Click "Test" button
   - Should show "Data source is working"

2. **Check dashboard provisioning**:
   ```bash
   docker exec mpc-grafana ls -la /etc/grafana/provisioning/dashboards/dashboards/
   ```

3. **View Grafana logs**:
   ```bash
   docker-compose -f docker-compose.monitoring.yml logs grafana
   ```

### High Memory Usage

1. **Reduce Prometheus retention**:
   Edit `docker-compose.monitoring.yml`:
   ```yaml
   - '--storage.tsdb.retention.time=15d'  # Reduced from 30d
   ```

2. **Adjust scrape intervals**:
   Edit `prometheus/prometheus.yml`:
   ```yaml
   global:
     scrape_interval: 30s  # Increased from 15s
   ```

### Alert Not Firing

1. **Check alert rules**:
   - Visit http://localhost:9090/alerts
   - Verify rule syntax and evaluation

2. **Test alert query manually**:
   - Go to Prometheus → Graph
   - Enter alert query
   - Check if condition is met

3. **Configure Alertmanager** (optional):
   - Add Alertmanager service to docker-compose
   - Configure notification channels (email, Slack, etc.)

## Performance Tuning

### Prometheus Optimization

```yaml
# prometheus.yml
global:
  scrape_interval: 15s      # Balance between granularity and load
  evaluation_interval: 15s  # How often to evaluate rules

# For high-cardinality metrics, use longer intervals
scrape_configs:
  - job_name: 'cadvisor'
    scrape_interval: 30s    # Container metrics less frequently
```

### Grafana Optimization

1. **Use query caching**:
   - Set appropriate refresh intervals (10s, 30s, 1m)
   - Avoid auto-refresh on complex dashboards

2. **Optimize PromQL queries**:
   ```promql
   # Bad: High cardinality
   rate(mpc_votes_total[5m])

   # Good: Aggregated
   sum(rate(mpc_votes_total[5m])) by (node_id)
   ```

3. **Limit dashboard time range**:
   - Default: Last 1 hour
   - For overview: Last 6 hours
   - For debugging: Last 15 minutes

## Backup and Restore

### Backup Prometheus Data

```bash
# Stop Prometheus
docker-compose -f docker-compose.monitoring.yml stop prometheus

# Backup data volume
docker run --rm \
  -v monitoring_prometheus-data:/data \
  -v $(pwd)/backups:/backup \
  alpine tar czf /backup/prometheus-$(date +%Y%m%d).tar.gz /data

# Restart Prometheus
docker-compose -f docker-compose.monitoring.yml start prometheus
```

### Backup Grafana Dashboards

```bash
# Export all dashboards via API
curl -u admin:your_password \
  http://localhost:3000/api/search?query=& | \
  jq -r '.[] | .uid' | \
  xargs -I {} curl -u admin:your_password \
  http://localhost:3000/api/dashboards/uid/{} > dashboard-{}.json
```

### Restore Grafana Dashboard

```bash
# Import dashboard via API
curl -X POST \
  -H "Content-Type: application/json" \
  -u admin:your_password \
  -d @dashboard.json \
  http://localhost:3000/api/dashboards/db
```

## Security Best Practices

1. **Change default passwords**:
   - Set strong `GRAFANA_ADMIN_PASSWORD`
   - Rotate credentials regularly

2. **Enable authentication**:
   - Grafana: Basic auth enabled by default
   - Prometheus: Add reverse proxy with auth if exposing externally

3. **Network isolation**:
   - Keep monitoring network separate from public networks
   - Use firewall rules to restrict access

4. **TLS encryption**:
   - Enable HTTPS for Grafana in production
   - Use certificate from production CA

5. **Audit logs**:
   - Enable Grafana audit logging
   - Monitor access to dashboards and data sources

## Integration with Main Stack

To integrate with the main MPC wallet stack:

```bash
# Start main stack first
cd production/docker
docker-compose up -d

# Then start monitoring
cd ../monitoring
docker-compose -f docker-compose.monitoring.yml up -d

# Verify both stacks are running
docker ps | grep mpc
```

## Maintenance

### Regular Tasks

1. **Weekly**: Review alert history and adjust thresholds
2. **Monthly**: Check disk usage and cleanup old data
3. **Quarterly**: Update dashboard queries for new features
4. **Yearly**: Update monitoring stack versions

### Updating Stack

```bash
# Pull latest images
docker-compose -f docker-compose.monitoring.yml pull

# Restart with new images
docker-compose -f docker-compose.monitoring.yml up -d

# Verify health
docker-compose -f docker-compose.monitoring.yml ps
```

## Support

For issues or questions:

1. Check Prometheus logs: `docker logs mpc-prometheus`
2. Check Grafana logs: `docker logs mpc-grafana`
3. Review this README and troubleshooting section
4. Check Prometheus/Grafana documentation

## License

MIT License - See main project LICENSE file
