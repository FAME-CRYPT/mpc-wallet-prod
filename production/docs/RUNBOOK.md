# Operator Runbook

**Version**: 0.1.0
**Last Updated**: 2026-01-20
**On-Call**: https://oncall.your-org.com

## Table of Contents

1. [Daily Operations](#daily-operations)
2. [Monitoring and Alerting](#monitoring-and-alerting)
3. [Common Issues and Troubleshooting](#common-issues-and-troubleshooting)
4. [Incident Response](#incident-response)
5. [Maintenance Procedures](#maintenance-procedures)
6. [Disaster Recovery](#disaster-recovery)
7. [Certificate Management](#certificate-management)
8. [Database Operations](#database-operations)
9. [Cluster Management](#cluster-management)

## Daily Operations

### Morning Health Check (15 minutes)

#### 1. Check Cluster Status

```bash
# From any node, check API health
curl http://localhost:8080/health

# Expected response:
{
  "status": "healthy",
  "timestamp": "2026-01-20T08:00:00Z",
  "version": "0.1.0"
}

# Check all nodes
for port in 8080 8081 8082 8083 8084; do
  echo "Node on port $port:"
  curl -s http://localhost:$port/health | jq .
done
```

**Green Flag**: All 5 nodes respond "healthy"
**Yellow Flag**: 4 nodes healthy, 1 degraded → Investigate
**Red Flag**: <4 nodes healthy → URGENT, cluster cannot sign

#### 2. Check Service Health

```bash
# Check all Docker containers
docker-compose ps

# Expected: All services "Up" and "healthy"

# Check specific services
docker inspect --format='{{.State.Health.Status}}' mpc-node-1  # healthy
docker inspect --format='{{.State.Health.Status}}' mpc-postgres  # healthy
docker inspect --format='{{.State.Health.Status}}' mpc-etcd-1   # healthy
```

#### 3. Review Overnight Activity

```bash
# Check transaction count (last 24 hours)
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT state, COUNT(*)
  FROM transactions
  WHERE created_at > NOW() - INTERVAL '24 hours'
  GROUP BY state;
"

# Expected output shows distribution across states

# Check for Byzantine violations
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT COUNT(*), violation_type
  FROM byzantine_violations
  WHERE detected_at > NOW() - INTERVAL '24 hours'
  GROUP BY violation_type;
"

# Expected: 0 violations (any violations need investigation)
```

#### 4. Check Disk Space

```bash
# Host system
df -h

# Warning threshold: 80% full
# Critical threshold: 90% full

# Docker volumes
docker system df -v

# Check PostgreSQL disk usage
docker exec mpc-postgres du -sh /var/lib/postgresql/data
```

#### 5. Check Logs for Errors

```bash
# Recent errors from all nodes (last hour)
docker-compose logs --since 1h | grep -i "error\|fatal\|panic"

# Expected: No critical errors

# Check for specific issues
docker-compose logs --since 1h | grep -i "byzantine"
docker-compose logs --since 1h | grep -i "timeout"
docker-compose logs --since 1h | grep -i "certificate"
```

### Evening Review (10 minutes)

#### 1. Transaction Summary

```bash
# Daily transaction stats
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT
    state,
    COUNT(*) as count,
    SUM(amount_sats) / 100000000.0 as total_btc,
    AVG(EXTRACT(EPOCH FROM (completed_at - created_at))) as avg_duration_secs
  FROM transactions
  WHERE created_at > CURRENT_DATE
  GROUP BY state;
"
```

#### 2. Node Performance

```bash
# Check resource usage
docker stats --no-stream --format "table {{.Name}}\t{{.CPUPerc}}\t{{.MemUsage}}"

# Warning: >80% CPU or memory sustained
```

#### 3. Certificate Expiration

```bash
# Check certificate validity (warn if <30 days)
./scripts/verify-certs.sh

# Expected: All certificates valid for >30 days
```

## Monitoring and Alerting

### Grafana Dashboards

Access: http://localhost:3000

#### Primary Dashboard: MPC Cluster Overview

**Critical Panels**:
1. **Active Nodes**: Should always be 5
2. **Byzantine Violations (24h)**: Should be 0
3. **Transaction Success Rate**: Should be >95%
4. **Presignature Pool Size**: Should be >20

**Daily Review**:
- Check for anomalies in transaction throughput
- Verify no unusual spikes in errors
- Confirm presignature pools are replenishing

#### Secondary Dashboards

1. **Byzantine Consensus**: Deep dive into voting patterns
2. **Signature Performance**: Protocol timing analysis
3. **Infrastructure**: Database and etcd health
4. **Network Monitoring**: Connectivity and bandwidth

### Alert Thresholds

#### Critical Alerts (Page On-Call)

| Alert | Threshold | Action |
|-------|-----------|--------|
| MPCNodeDown | Node unreachable >1min | Restart node immediately |
| MPCClusterDegraded | <4 nodes available | Emergency response |
| ByzantineDoubleVoteDetected | Any occurrence | Investigate node, possible compromise |
| PostgresDown | Database unreachable | Restore database service |
| EtcdNoLeader | No leader >30s | Check etcd cluster |
| PresignaturePoolDepleted | Pool size = 0 | Signing blocked, generate urgently |

#### Warning Alerts (Investigate Within 1 Hour)

| Alert | Threshold | Action |
|-------|-----------|--------|
| HighTransactionFailureRate | >10% failed | Check logs for pattern |
| SlowSignatureGeneration | p95 >10s | Performance investigation |
| PresignaturePoolLow | Pool <10 | Trigger generation |
| MPCNodeHighMemoryUsage | >85% for 5min | Check for memory leak |
| TLSHandshakeFailures | >0.5/sec | Certificate or network issue |

### Prometheus Queries

Access: http://localhost:9090

**Key Queries**:

```promql
# Cluster health
mpc_active_nodes{cluster="mpc-wallet"}

# Transaction throughput
rate(mpc_transactions_total[5m])

# Byzantine violations
increase(mpc_byzantine_violations_total[24h])

# Signature latency (p95)
histogram_quantile(0.95, rate(mpc_signature_duration_seconds_bucket[5m]))

# Presignature pool health
min(mpc_presignature_pool_size) by (node_id)
```

## Common Issues and Troubleshooting

### Issue: Node Not Responding

#### Symptoms
- Health check returns 500 or times out
- Node not visible in cluster status
- Grafana shows node as down

#### Diagnosis

```bash
# 1. Check if container is running
docker ps | grep node-1

# 2. Check container logs (last 100 lines)
docker logs node-1 --tail 100

# 3. Check resource usage
docker stats node-1 --no-stream

# 4. Check network connectivity
docker exec node-1 nc -zv node-2 9000
```

#### Resolution

**If container crashed**:
```bash
# Check exit code
docker inspect node-1 --format='{{.State.ExitCode}}'

# Restart container
docker-compose restart node-1

# Wait 30 seconds for startup
sleep 30

# Verify health
curl http://localhost:8080/health
```

**If resource exhaustion**:
```bash
# Check memory/CPU limits in docker-compose.yml
# Increase if needed:
#   memory: 4G  (was 2G)
#   cpus: '4'   (was '2')

# Apply changes
docker-compose up -d node-1
```

**If persistent failure**:
```bash
# Check certificate validity
openssl x509 -in certs/node1.crt -text -noout | grep "Not After"

# Check etcd connectivity
docker exec node-1 curl http://etcd-1:2379/health

# Check PostgreSQL connectivity
docker exec node-1 pg_isready -h postgres -p 5432
```

#### Escalation Criteria
- Node fails to restart after 3 attempts
- Certificates expired (emergency rotation needed)
- Database connectivity issues (escalate to DBA)

### Issue: Byzantine Violation Detected

#### Symptoms
- Alert: ByzantineViolationDetected fires
- Logs show "Byzantine violation: double_vote|invalid_signature|minority_vote"
- Node automatically banned

#### Diagnosis

```bash
# 1. Query violation details
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT
    id, node_id, violation_type, tx_id,
    evidence, detected_at, action_taken
  FROM byzantine_violations
  ORDER BY detected_at DESC
  LIMIT 10;
"

# 2. Check node history
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT violation_type, COUNT(*)
  FROM byzantine_violations
  WHERE node_id = <NODE_ID>
  GROUP BY violation_type;
"

# 3. Check if node is currently banned
docker exec mpc-etcd-1 etcdctl get /bans/<NODE_ID>
```

#### Resolution

**For DoubleVote**:
1. **CRITICAL**: Potential compromise or bug
2. Immediately isolate node from cluster
3. Review node logs for evidence of compromise
4. If compromised: Rotate all certificates, redeploy node
5. If bug: Investigate code path, file incident report

**For InvalidSignature**:
1. Check if certificate expired or corrupted
2. Verify node's private key matches certificate
3. If certificate issue: Rotate certificate
4. If persistent: Redeploy node

**For MinorityVote**:
1. Less critical, may be timing issue
2. Review node's validation logic
3. Check if node has stale data
4. If repeated: Investigate node's state

#### Manual Unban (if false positive)

```bash
# Only if you're certain this was a false positive
docker exec mpc-etcd-1 etcdctl del /bans/<NODE_ID>

# Clear violation record (audit trail preserved in PostgreSQL)
# DO NOT delete from PostgreSQL (audit trail is immutable)
```

### Issue: Consensus Timeout

#### Symptoms
- Transaction stuck in "voting" state >5 minutes
- Logs show "consensus timeout"
- Not enough votes received

#### Diagnosis

```bash
# 1. Check transaction state
curl http://localhost:8080/api/v1/transactions/<TXID>

# 2. Check vote count
docker exec mpc-etcd-1 etcdctl get /tx/<TXID>/votes/approve
docker exec mpc-etcd-1 etcdctl get /tx/<TXID>/votes/reject

# 3. Check which nodes voted
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT node_id, approve, received_at
  FROM votes
  WHERE tx_id = '<TXID>'
  ORDER BY received_at;
"

# 4. Check node connectivity
for i in 1 2 3 4 5; do
  curl -s http://localhost:808$((i-1))/api/v1/cluster/status | jq .
done
```

#### Resolution

1. **Identify missing nodes**: Nodes that haven't voted
2. **Check node health**: Ensure nodes are online and healthy
3. **Check network**: Verify QUIC connectivity between nodes
4. **Restart missing nodes**: If unresponsive

```bash
# Restart unresponsive node
docker-compose restart node-<N>

# Transaction will automatically retry or timeout
# Manual retry may be needed if transaction stuck
```

### Issue: Presignature Pool Depleted

#### Symptoms
- Alert: PresignaturePoolDepleted
- Signature generation takes 2+ seconds (falling back to full protocol)
- Logs show "presignature pool empty"

#### Diagnosis

```bash
# Check pool status on all nodes
for i in 1 2 3 4 5; do
  echo "Node $i:"
  curl -s http://localhost:808$((i-1))/api/v1/presig/status | jq .
done
```

#### Resolution

```bash
# Trigger presignature generation
threshold-wallet presig generate --count 50

# Or via API
curl -X POST http://localhost:8080/api/v1/presig/generate \
  -H "Content-Type: application/json" \
  -d '{"count": 50}'

# Monitor generation progress
watch -n 5 'threshold-wallet presig status'
```

**Prevention**:
- Configure automatic pool replenishment:
  - Target pool size: 50
  - Minimum before generation: 20
  - Generation batch size: 10

### Issue: Certificate Expiring

#### Symptoms
- Alert: Certificate expires in <30 days
- TLS handshake warnings in logs

#### Diagnosis

```bash
# Check all certificate expirations
./scripts/verify-certs.sh

# Check specific certificate
openssl x509 -in certs/node1.crt -enddate -noout
```

#### Resolution

See [Certificate Rotation](#certificate-rotation) section below.

### Issue: etcd Cluster Degraded

#### Symptoms
- etcd endpoint health shows unhealthy members
- Leader election failures
- Slow consensus

#### Diagnosis

```bash
# Check cluster health
docker exec mpc-etcd-1 etcdctl endpoint health --cluster

# Check member list
docker exec mpc-etcd-1 etcdctl member list

# Check cluster status
docker exec mpc-etcd-1 etcdctl endpoint status --cluster -w table
```

#### Resolution

**If 1 member down (quorum maintained)**:
```bash
# Restart unhealthy member
docker-compose restart etcd-<N>

# Verify rejoined
docker exec mpc-etcd-1 etcdctl member list
```

**If quorum lost (2+ members down) - CRITICAL**:
```bash
# Stop all etcd members
docker-compose stop etcd-1 etcd-2 etcd-3

# Start fresh cluster (WARNING: loses state)
docker-compose up -d etcd-1 etcd-2 etcd-3

# Nodes will re-sync from PostgreSQL on next transaction
```

### Issue: PostgreSQL Connection Errors

#### Symptoms
- Logs show "connection refused" or "too many connections"
- Transactions fail to save

#### Diagnosis

```bash
# Check PostgreSQL is running
docker ps | grep postgres

# Check connection count
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT count(*) FROM pg_stat_activity;
"

# Check max connections
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SHOW max_connections;
"
```

#### Resolution

**If connection pool exhausted**:
```bash
# Increase max_connections (requires restart)
# Edit docker-compose.yml:
#   environment:
#     - POSTGRES_MAX_CONNECTIONS=200  # was 100

docker-compose restart postgres

# Or kill idle connections
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT pg_terminate_backend(pid)
  FROM pg_stat_activity
  WHERE state = 'idle'
  AND state_change < NOW() - INTERVAL '5 minutes';
"
```

## Incident Response

### Severity Levels

| SEV | Description | Response Time | Example |
|-----|-------------|---------------|---------|
| SEV1 | Critical outage | Immediate | <4 nodes available, signing blocked |
| SEV2 | Major degradation | 15 minutes | High error rate, performance issues |
| SEV3 | Minor impact | 1 hour | Single node down, degraded mode |
| SEV4 | Low impact | 4 hours | Warning alerts, monitoring gaps |

### SEV1: Critical Outage

**Examples**:
- <4 nodes available (cannot reach signing threshold)
- Database completely unavailable
- Multiple Byzantine violations (potential attack)
- Complete network partition

**Immediate Actions**:

1. **Declare incident** (page on-call team)
2. **Assess scope**:
   ```bash
   # Quick cluster status
   ./scripts/quick-health-check.sh

   # Check transaction backlog
   threshold-wallet tx list --state pending
   ```

3. **Communicate**:
   - Update status page
   - Notify stakeholders
   - Create incident channel (Slack/Teams)

4. **Stop the bleeding**:
   - If attack suspected: Isolate compromised nodes
   - If infrastructure issue: Failover to backup systems
   - If config issue: Revert recent changes

5. **Restore service**:
   - Bring nodes online one by one
   - Verify health after each node
   - Resume transaction processing

6. **Monitor recovery**:
   ```bash
   # Watch cluster status
   watch -n 5 'curl -s http://localhost:8080/api/v1/cluster/status'

   # Monitor transaction processing
   watch -n 10 'threshold-wallet tx list --limit 10'
   ```

### SEV2: Major Degradation

**Examples**:
- High transaction failure rate (>20%)
- Slow signature generation (>10s consistently)
- Single node completely down

**Response Procedure**:

1. **Assess impact**: How many users affected?
2. **Identify root cause**:
   ```bash
   # Check recent changes
   docker-compose logs --since 30m

   # Check resource usage
   docker stats --no-stream

   # Check network latency
   ./scripts/check-network-latency.sh
   ```

3. **Mitigate**:
   - Restart affected services
   - Scale resources if needed
   - Route around failed components

4. **Verify resolution**:
   - Transaction success rate back to >95%
   - Latency within normal bounds
   - No error spikes

### Post-Incident Review

Required for SEV1 and SEV2 incidents within 48 hours:

1. **Timeline**: Detailed sequence of events
2. **Root Cause**: What actually caused the incident?
3. **Impact**: User impact, duration, data loss
4. **What Went Well**: Effective actions taken
5. **What Went Wrong**: Gaps in process or tools
6. **Action Items**: Prevent recurrence
   - [ ] Code fixes
   - [ ] Monitoring improvements
   - [ ] Documentation updates
   - [ ] Runbook updates

## Maintenance Procedures

### Planned Maintenance Windows

**Recommended Schedule**:
- Monthly: 2nd Tuesday, 02:00-04:00 UTC
- Duration: 2 hours
- Frequency: Monthly for routine updates, quarterly for major upgrades

**Preparation** (1 week before):
1. Announce maintenance window
2. Review changes to be applied
3. Prepare rollback plan
4. Test changes in staging environment

**Maintenance Checklist**:

#### 1. Pre-Maintenance (T-1 hour)

```bash
# Backup everything
./scripts/backup-all.sh

# Verify backups
ls -lh backups/
# Should show: postgres-*.sql, etcd-*.db, certs-*.tar.gz

# Check cluster health
./scripts/health-check.sh

# Document current state
docker-compose ps > pre-maintenance-state.txt
threshold-wallet cluster status > pre-maintenance-cluster.txt
```

#### 2. Maintenance Window

**Software Update**:
```bash
# Pull latest images
docker-compose pull

# Rebuild if using custom images
docker-compose build --no-cache

# Rolling update (zero downtime)
for node in 1 2 3 4 5; do
  echo "Updating node-$node..."
  docker-compose stop node-$node
  docker-compose up -d node-$node

  # Wait for node to rejoin cluster
  sleep 60

  # Verify node healthy
  curl http://localhost:808$((node-1))/health

  # Check cluster still has quorum
  threshold-wallet cluster status | grep "active_nodes: 4|5"
done
```

**Certificate Rotation** (if scheduled):
```bash
# See Certificate Rotation section
./scripts/renew-certs.sh
```

**Database Maintenance**:
```bash
# Vacuum and analyze (improves performance)
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "VACUUM ANALYZE;"

# Reindex if needed
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "REINDEX DATABASE mpc_wallet;"
```

#### 3. Post-Maintenance (Verification)

```bash
# Verify all nodes healthy
./scripts/health-check.sh

# Run smoke test
./scripts/smoke-test.sh
# - Creates test transaction
# - Verifies consensus
# - Checks signature generation

# Compare state
threshold-wallet cluster status > post-maintenance-cluster.txt
diff pre-maintenance-cluster.txt post-maintenance-cluster.txt

# Monitor for 30 minutes
watch -n 30 './scripts/health-check.sh'
```

**Rollback Criteria**:
- Any node fails to start
- Transaction failure rate >10%
- Byzantine violations detected
- Performance degradation >50%

**Rollback Procedure**:
```bash
# Stop all services
docker-compose down

# Restore from backup
./scripts/restore-from-backup.sh backups/postgres-<timestamp>.sql

# Start with previous configuration
git checkout <previous-commit>
docker-compose up -d

# Verify rollback successful
./scripts/health-check.sh
```

### Certificate Rotation

**When to Rotate**:
- Quarterly (every 90 days) as best practice
- When certificate expires in <30 days
- After suspected key compromise
- When adding/removing nodes

**Rotation Procedure** (Zero Downtime):

```bash
# 1. Generate new certificates (keep old ones)
cd scripts
./renew-certs.sh

# This creates new certificates in certs/new/

# 2. Deploy new CA certificate to all nodes first
for node in node-1 node-2 node-3 node-4 node-5; do
  docker cp certs/new/ca.crt $node:/certs/ca-new.crt
done

# 3. Configure nodes to trust both old and new CA (requires code change)
# Deploy this change:
# - Load both /certs/ca.crt and /certs/ca-new.crt as trusted CAs

# 4. Rolling update with new certificates
for i in 1 2 3 4 5; do
  # Stop node
  docker-compose stop node-$i

  # Replace certificates
  cp certs/new/node$i.crt certs/node$i.crt
  cp certs/new/node$i.key certs/node$i.key

  # Start node (will use new cert, trust both CAs)
  docker-compose up -d node-$i

  # Wait for node to rejoin
  sleep 60

  # Verify connectivity
  curl http://localhost:808$((i-1))/health
done

# 5. Remove old CA trust after all nodes updated
# Deploy code change to only trust new CA

# 6. Clean up
mv certs/old certs/backup-$(date +%Y%m%d)
mv certs/new/* certs/
rmdir certs/new
```

## Disaster Recovery

### Recovery Time Objective (RTO)

**Target**: <1 hour to restore service
**Worst Case**: <4 hours for complete rebuild

### Recovery Point Objective (RPO)

**Target**: <5 minutes of transaction data loss
**Achieved By**: PostgreSQL continuous archiving (WAL)

### Backup Strategy

**Automated Backups**:
- PostgreSQL: Daily full + continuous WAL archiving
- etcd: Hourly snapshots
- Certificates: Weekly
- Configuration: Git-tracked

**Backup Locations**:
- Primary: Local disk (`/backups`)
- Secondary: S3/GCS bucket (encrypted)
- Tertiary: Offline storage (monthly)

**Backup Script** (runs daily via cron):

```bash
#!/bin/bash
# /opt/mpc-wallet/scripts/backup-all.sh

BACKUP_DIR="/backups/$(date +%Y%m%d-%H%M%S)"
mkdir -p "$BACKUP_DIR"

# PostgreSQL full backup
docker exec mpc-postgres pg_dump -U mpc -d mpc_wallet | gzip > "$BACKUP_DIR/postgres.sql.gz"

# etcd snapshot
for i in 1 2 3; do
  docker exec mpc-etcd-$i etcdctl snapshot save /tmp/etcd-backup.db
  docker cp mpc-etcd-$i:/tmp/etcd-backup.db "$BACKUP_DIR/etcd-$i.db"
done

# Certificates
tar czf "$BACKUP_DIR/certs.tar.gz" certs/

# Configuration
tar czf "$BACKUP_DIR/config.tar.gz" docker-compose.yml .env scripts/

# Upload to S3
aws s3 cp "$BACKUP_DIR" s3://mpc-wallet-backups/ --recursive --sse

# Verify backup
if [ $? -eq 0 ]; then
  echo "Backup successful: $BACKUP_DIR"
else
  echo "Backup FAILED" >&2
  exit 1
fi

# Clean up old backups (keep 30 days)
find /backups -type d -mtime +30 -exec rm -rf {} +
```

### Disaster Scenarios

#### Scenario 1: Complete Data Center Failure

**Impact**: All nodes, database, and etcd offline

**Recovery Procedure**:

```bash
# 1. Provision new infrastructure
# - 5 compute instances for nodes
# - PostgreSQL instance
# - 3 etcd instances

# 2. Restore PostgreSQL
# Download latest backup from S3
aws s3 cp s3://mpc-wallet-backups/latest/postgres.sql.gz .

# Restore database
gunzip postgres.sql.gz
docker exec -i mpc-postgres psql -U mpc -d mpc_wallet < postgres.sql

# 3. Restore certificates
aws s3 cp s3://mpc-wallet-backups/latest/certs.tar.gz .
tar xzf certs.tar.gz -C /opt/mpc-wallet/

# 4. Restore configuration
aws s3 cp s3://mpc-wallet-backups/latest/config.tar.gz .
tar xzf config.tar.gz -C /opt/mpc-wallet/

# 5. Initialize etcd cluster (fresh start OK)
docker-compose up -d etcd-1 etcd-2 etcd-3

# 6. Start all nodes
docker-compose up -d node-1 node-2 node-3 node-4 node-5

# 7. Verify recovery
./scripts/health-check.sh
threshold-wallet cluster status

# 8. Resume operations
# Nodes will re-sync state from PostgreSQL automatically
```

**Estimated Recovery Time**: 45 minutes

#### Scenario 2: Database Corruption

**Impact**: PostgreSQL data corrupted or deleted

**Recovery Procedure**:

```bash
# 1. Stop all nodes (prevent further writes)
docker-compose stop node-1 node-2 node-3 node-4 node-5

# 2. Stop PostgreSQL
docker-compose stop postgres

# 3. Restore from backup
./scripts/restore-postgres.sh backups/<latest-backup>/postgres.sql.gz

# 4. Verify data integrity
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT COUNT(*) FROM transactions;
  SELECT COUNT(*) FROM votes;
  SELECT COUNT(*) FROM byzantine_violations;
"

# 5. Restart nodes
docker-compose start node-1 node-2 node-3 node-4 node-5

# 6. Verify operations resumed
threshold-wallet tx list --limit 10
```

#### Scenario 3: Byzantine Attack (Multiple Compromised Nodes)

**Impact**: Potential data integrity compromise

**Response**:

```bash
# 1. IMMEDIATELY isolate all nodes
docker-compose down

# 2. Analyze violation logs
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT * FROM byzantine_violations
  WHERE detected_at > NOW() - INTERVAL '1 hour'
  ORDER BY detected_at;
"

# 3. Identify compromised nodes
# Review evidence JSON for each violation

# 4. Restore from known-good backup (before attack)
./scripts/restore-from-backup.sh backups/<pre-attack-timestamp>/

# 5. Redeploy with new certificates
cd scripts
./generate-certs.sh
# Creates entirely new PKI

# 6. Audit and harden
# - Review all recent configuration changes
# - Check for unauthorized access
# - Update firewall rules
# - Rotate all secrets

# 7. Gradual restart with monitoring
# Start 1 node at a time, monitoring for anomalies

# 8. Incident report
# Document attack vector, impact, remediation
```

### DR Testing

**Frequency**: Quarterly

**Test Procedure**:

```bash
# 1. Schedule DR drill (non-production environment)
# 2. Simulate failure scenario
# 3. Execute recovery procedure
# 4. Measure RTO and RPO
# 5. Document gaps and improvements
# 6. Update runbook
```

## Database Operations

### Performance Monitoring

```bash
# Active queries
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT pid, usename, state, query, query_start
  FROM pg_stat_activity
  WHERE state != 'idle'
  ORDER BY query_start;
"

# Slow queries (>1 second)
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT query, calls, total_time, mean_time
  FROM pg_stat_statements
  WHERE mean_time > 1000
  ORDER BY total_time DESC
  LIMIT 20;
"

# Table sizes
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  SELECT
    schemaname,
    tablename,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) AS size
  FROM pg_tables
  WHERE schemaname = 'public'
  ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC;
"
```

### Data Retention

**Policy**:
- Transactions: Keep all (archive after 1 year)
- Votes: Keep all (audit requirement)
- Byzantine violations: Keep all (security audit)
- Audit logs: Keep all (compliance requirement)
- Old presignature usage: Archive after 90 days

**Archive Script** (run monthly):

```bash
#!/bin/bash
# Archive old data to cold storage

# Archive presignature usage >90 days
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  COPY (
    SELECT * FROM presignature_usage
    WHERE used_at < NOW() - INTERVAL '90 days'
  ) TO '/tmp/presig_archive.csv' CSV HEADER;
"

# Move to S3
docker cp mpc-postgres:/tmp/presig_archive.csv .
aws s3 cp presig_archive.csv s3://mpc-wallet-archives/presig/$(date +%Y%m)/

# Delete from active database
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "
  DELETE FROM presignature_usage
  WHERE used_at < NOW() - INTERVAL '90 days';
"
```

## Cluster Management

### Adding a Node (5-of-6 Expansion)

**Prerequisites**:
- New server provisioned
- Network connectivity to existing nodes
- New certificate generated

**Procedure**:

```bash
# 1. Generate certificate for node-6
cd scripts
./generate-certs.sh --node-id 6

# 2. Update configuration
# Edit .env:
TOTAL_NODES=6
THRESHOLD=5  # Update threshold if desired

# 3. Add node-6 service to docker-compose.yml
# (Copy node-5 configuration, update IDs)

# 4. Update BOOTSTRAP_PEERS on all existing nodes
# Add node-6:9000 to peer lists

# 5. Start new node
docker-compose up -d node-6

# 6. Verify node joined cluster
threshold-wallet cluster nodes
# Should show 6 nodes

# 7. Run DKG to include new node
threshold-wallet dkg start --protocol cggmp24 --threshold 5 --total 6
```

### Removing a Node (5-of-4 Reduction)

**Use Case**: Decommissioning node or reducing cluster size

```bash
# 1. Stop node gracefully
docker-compose stop node-5

# 2. Update configuration
TOTAL_NODES=4
THRESHOLD=3

# 3. Remove from docker-compose.yml

# 4. Update BOOTSTRAP_PEERS (remove node-5)

# 5. Run new DKG (creates new key shares for 4 nodes)
threshold-wallet dkg start --protocol cggmp24 --threshold 3 --total 4

# 6. Verify cluster operates with 4 nodes
threshold-wallet cluster status
```

### Node Replacement (Security Incident)

**Scenario**: Node compromised, needs complete replacement

```bash
# 1. Immediately stop compromised node
docker-compose stop node-3

# 2. Ban node in etcd (prevent reconnection)
docker exec mpc-etcd-1 etcdctl put /bans/node-3 "true"

# 3. Provision new server (different hardware/IP)

# 4. Generate new certificate
./generate-certs.sh --node-id 3 --force

# 5. Deploy fresh node-3
# Use new server, new cert

# 6. Update peer lists on all nodes
# Replace old node-3 IP with new IP

# 7. Run DKG to create new key shares
# Old key shares are invalidated
threshold-wallet dkg start --protocol cggmp24 --threshold 4 --total 5

# 8. Decommission old node
# Securely wipe disks
# Revoke old certificate
```

---

**Runbook Version**: 1.0
**Last Updated**: 2026-01-20
**Next Review**: 2026-04-20
**Maintained By**: MPC Operations Team
