# MPC Wallet Docker Deployment Checklist

Use this checklist to ensure a successful deployment.

## Pre-Deployment

### Environment Setup
- [ ] Docker 24.0+ installed
- [ ] Docker Compose 2.20+ installed
- [ ] Minimum 16GB RAM available
- [ ] 100GB free disk space
- [ ] Linux host (or WSL2 on Windows)

### Certificate Generation
- [ ] Run `cd ../scripts && ./generate-certs.sh`
- [ ] Verify `../certs/ca.crt` exists
- [ ] Verify `../certs/node1.crt` through `node5.crt` exist
- [ ] Verify `../certs/node1.key` through `node5.key` exist
- [ ] Certificate permissions are correct (600 for .key files)

### Configuration
- [ ] Copy `.env.example` to `.env`
- [ ] Set `POSTGRES_PASSWORD` (strong password, 20+ chars)
- [ ] Verify `CERTS_PATH` points to certificate directory
- [ ] Set `BITCOIN_NETWORK` (testnet or mainnet)
- [ ] Set `ESPLORA_URL` for chosen network
- [ ] Review `THRESHOLD` setting (default: 4)
- [ ] Review `TOTAL_NODES` setting (default: 5)
- [ ] Set `RUST_LOG` level (info for prod, debug for dev)

### File Verification
- [ ] `Dockerfile.node` exists
- [ ] `docker-compose.yml` exists
- [ ] `.env` file configured
- [ ] All scripts in `scripts/` are executable
- [ ] `server.rs` binary exists at `crates/api/src/bin/server.rs`
- [ ] `Cargo.toml` updated with binary target

## Build Phase

### Image Building
- [ ] Run `docker-compose build`
- [ ] Build completes without errors
- [ ] Image size reasonable (<500MB for node image)
- [ ] No security warnings in build output

### Configuration Validation
- [ ] Run `docker-compose config` to validate YAML
- [ ] No syntax errors in docker-compose files
- [ ] Environment variables resolve correctly
- [ ] Network configuration valid
- [ ] Volume configuration valid

## Deployment Phase

### Service Startup
- [ ] Run `docker-compose up -d` (or use Makefile: `make up`)
- [ ] All containers start successfully
- [ ] No immediate crashes or restarts
- [ ] Wait 60 seconds for initialization

### Container Status
- [ ] Run `docker-compose ps`
- [ ] All containers show "Up" status
- [ ] Health checks show "healthy" (may take 1-2 minutes)
- [ ] No containers in "restarting" state

### etcd Cluster
- [ ] Run `docker exec mpc-etcd-1 etcdctl endpoint health --cluster`
- [ ] All 3 etcd nodes show "healthy"
- [ ] Run `docker exec mpc-etcd-1 etcdctl member list`
- [ ] Verify 3 cluster members listed
- [ ] No error messages in etcd logs

### PostgreSQL
- [ ] Run `docker exec mpc-postgres pg_isready -U mpc`
- [ ] PostgreSQL shows "accepting connections"
- [ ] Run `docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "SELECT version();"`
- [ ] Database connection successful
- [ ] Run `docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='public';"`
- [ ] Tables created (should show 7+ tables)

### MPC Nodes
- [ ] Test Node 1: `curl http://localhost:8081/health`
- [ ] Test Node 2: `curl http://localhost:8082/health`
- [ ] Test Node 3: `curl http://localhost:8083/health`
- [ ] Test Node 4: `curl http://localhost:8084/health`
- [ ] Test Node 5: `curl http://localhost:8085/health`
- [ ] All nodes return `{"status":"ok"}` or similar
- [ ] No connection refused errors

### Network Connectivity
- [ ] Run `docker network ls`
- [ ] Verify `mpc-wallet_mpc-internal` network exists
- [ ] Verify `mpc-wallet_mpc-external` network exists
- [ ] Test inter-node communication (QUIC ports)

### Volume Persistence
- [ ] Run `docker volume ls`
- [ ] Verify etcd data volumes exist (3)
- [ ] Verify postgres data volume exists
- [ ] Verify node data volumes exist (5)

## Post-Deployment Validation

### Automated Validation
- [ ] Run `./scripts/validate-deployment.sh`
- [ ] All checks pass
- [ ] No critical errors reported
- [ ] Review summary output

### API Functionality
- [ ] Test cluster status: `curl http://localhost:8081/api/v1/cluster/status`
- [ ] Test node list: `curl http://localhost:8081/api/v1/cluster/nodes`
- [ ] Test wallet address: `curl http://localhost:8081/api/v1/wallet/address`
- [ ] Test wallet balance: `curl http://localhost:8081/api/v1/wallet/balance`
- [ ] All endpoints return valid JSON responses

### Logging
- [ ] Run `docker-compose logs` to check all logs
- [ ] No error messages in node logs
- [ ] No panic messages
- [ ] No connection failures
- [ ] Structured JSON logging working

### Resource Usage
- [ ] Run `docker stats --no-stream`
- [ ] Memory usage within limits
- [ ] CPU usage reasonable
- [ ] No containers at 100% resource usage

## Security Verification

### Certificate Security
- [ ] Private keys not exposed in logs
- [ ] Certificate files have correct permissions
- [ ] CA private key stored securely offline
- [ ] Certificates not committed to git

### Network Security
- [ ] Internal network isolated (no external access)
- [ ] External network properly configured
- [ ] Firewall rules configured (if applicable)
- [ ] No unnecessary ports exposed

### Access Control
- [ ] PostgreSQL password is strong (not default)
- [ ] PostgreSQL accessible only from internal network
- [ ] etcd accessible only from internal network
- [ ] No default credentials in use

### Secrets Management
- [ ] `.env` file not committed to git
- [ ] Passwords not in logs
- [ ] Consider using Docker secrets for production
- [ ] Consider using secrets management system (Vault, etc.)

## Monitoring Setup

### Health Monitoring
- [ ] Health checks configured for all services
- [ ] Health endpoints responding
- [ ] Consider external monitoring (Datadog, etc.)
- [ ] Set up alerting for failures

### Log Aggregation
- [ ] Logs accessible via `docker-compose logs`
- [ ] Consider log aggregation (ELK, Splunk)
- [ ] Log rotation configured (prod mode)
- [ ] Audit logs being written to PostgreSQL

### Metrics
- [ ] Consider Prometheus/Grafana setup
- [ ] Resource metrics being collected
- [ ] Consider application-level metrics
- [ ] Set up dashboards

## Backup Configuration

### Backup Strategy
- [ ] Backup script tested: `make backup`
- [ ] PostgreSQL backups working
- [ ] etcd snapshots working
- [ ] Backup storage configured
- [ ] Backup retention policy defined

### Recovery Testing
- [ ] Test restore procedure
- [ ] Document recovery steps
- [ ] Verify backup integrity
- [ ] Define RTO (Recovery Time Objective)
- [ ] Define RPO (Recovery Point Objective)

## Production Readiness

### Performance
- [ ] Load testing completed
- [ ] Transaction throughput acceptable
- [ ] Latency within acceptable range
- [ ] Resource usage optimized
- [ ] Database queries optimized

### Reliability
- [ ] Failover testing completed
- [ ] Network partition testing completed
- [ ] Node restart testing completed
- [ ] Data persistence verified
- [ ] Byzantine fault tolerance verified

### Documentation
- [ ] Deployment documented
- [ ] Runbooks created
- [ ] Troubleshooting guide available
- [ ] On-call procedures defined
- [ ] Team trained on operations

### Compliance
- [ ] Security audit completed
- [ ] Compliance requirements met (SOC2, GDPR, etc.)
- [ ] Audit logging enabled
- [ ] Data retention policies implemented
- [ ] Incident response plan documented

## Operational Procedures

### Daily Operations
- [ ] Monitoring dashboards accessible
- [ ] Alerting configured
- [ ] Log review process defined
- [ ] On-call rotation established

### Maintenance
- [ ] Upgrade procedure documented
- [ ] Backup schedule configured
- [ ] Log rotation working
- [ ] Certificate renewal process defined

### Disaster Recovery
- [ ] DR plan documented
- [ ] Backup restore tested
- [ ] Failover procedures tested
- [ ] Recovery contacts identified

## Final Verification

### End-to-End Testing
- [ ] Create test transaction
- [ ] Verify voting process
- [ ] Verify signing process
- [ ] Verify transaction broadcast
- [ ] Verify database audit trail

### Stress Testing
- [ ] Multiple concurrent transactions
- [ ] Node failure scenarios
- [ ] Network partition scenarios
- [ ] Resource exhaustion scenarios

### Sign-Off
- [ ] Development team sign-off
- [ ] Operations team sign-off
- [ ] Security team sign-off
- [ ] Management approval

## Go-Live Checklist

### Pre-Launch
- [ ] All previous checklist items completed
- [ ] Final backup taken
- [ ] Rollback plan documented
- [ ] Support team ready
- [ ] Monitoring active

### Launch
- [ ] Start deployment
- [ ] Monitor logs in real-time
- [ ] Verify all services healthy
- [ ] Run validation script
- [ ] Test critical paths

### Post-Launch
- [ ] Monitor for 1 hour
- [ ] Verify no errors
- [ ] Check resource usage
- [ ] Verify backups running
- [ ] Update documentation

## Rollback Plan

If deployment fails:
- [ ] Document failure reason
- [ ] Stop services: `docker-compose down`
- [ ] Restore from backup if needed
- [ ] Review logs for root cause
- [ ] Fix issues before retry
- [ ] Update checklist with lessons learned

## Support Contacts

- **Primary Contact**: ___________________________
- **Secondary Contact**: ___________________________
- **On-Call**: ___________________________
- **Emergency**: ___________________________

## Sign-Off

| Role | Name | Signature | Date |
|------|------|-----------|------|
| Developer | | | |
| DevOps | | | |
| Security | | | |
| Manager | | | |

---

## Notes

Use this space for deployment-specific notes, issues encountered, or deviations from standard procedures:

_______________________________________________________________________________

_______________________________________________________________________________

_______________________________________________________________________________

_______________________________________________________________________________

---

**Deployment Date**: _______________
**Deployed By**: _______________
**Deployment Version**: _______________
**Environment**: [ ] Production [ ] Staging [ ] Development

---

## Quick Reference

### Essential Commands
```bash
# Build
docker-compose build

# Deploy
docker-compose up -d

# Validate
./scripts/validate-deployment.sh

# Check health
docker-compose ps
make health

# View logs
docker-compose logs -f

# Stop
docker-compose down
```

### Validation Script
```bash
./scripts/validate-deployment.sh
```
This script checks everything automatically!

### Troubleshooting
See [README.md](README.md#troubleshooting) for detailed troubleshooting guide.

---

Last Updated: 2026-01-20
