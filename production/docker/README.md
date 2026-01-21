# MPC Wallet Docker Deployment Guide

Complete Docker deployment configuration for the production MPC wallet system with Byzantine consensus, threshold signatures, and distributed coordination.

## Table of Contents

- [Overview](#overview)
- [Architecture](#architecture)
- [Prerequisites](#prerequisites)
- [Quick Start](#quick-start)
- [Configuration](#configuration)
- [Deployment](#deployment)
- [Monitoring](#monitoring)
- [Security](#security)
- [Troubleshooting](#troubleshooting)
- [Production Checklist](#production-checklist)

## Overview

This deployment includes:

- **5 MPC Wallet Nodes**: Distributed threshold wallet with 4-of-5 signature threshold
- **3-node etcd Cluster**: Distributed coordination and consensus
- **PostgreSQL Database**: Audit logs and transaction storage
- **QUIC + mTLS**: Secure peer-to-peer communication
- **REST API**: HTTP endpoints for wallet operations
- **Health Checks**: Automatic service health monitoring
- **Resource Limits**: Production-ready resource constraints

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     External Network                         │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │
│  │  Node 1  │  │  Node 2  │  │  Node 3  │  │  Node 4  │   │
│  │ API:8081 │  │ API:8082 │  │ API:8083 │  │ API:8084 │   │
│  │QUIC:9001 │  │QUIC:9002 │  │QUIC:9003 │  │QUIC:9004 │   │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘   │
│       │             │             │             │           │
│  ┌────┴─────┐       │             │             │           │
│  │  Node 5  │       │             │             │           │
│  │ API:8085 │───────┴─────────────┴─────────────┘           │
│  │QUIC:9005 │                                                │
│  └────┬─────┘                                                │
└───────┼──────────────────────────────────────────────────────┘
        │
┌───────┼──────────────────────────────────────────────────────┐
│       │              Internal Network                         │
│       │                                                       │
│  ┌────┴───────┐    ┌────────┐    ┌────────┐                │
│  │ PostgreSQL │    │ etcd-1 │    │ etcd-2 │                │
│  │   :5432    │    │ :2379  │    │ :2379  │                │
│  └────────────┘    └────────┘    └────────┘                │
│                                    ┌────────┐                │
│                                    │ etcd-3 │                │
│                                    │ :2379  │                │
│                                    └────────┘                │
└──────────────────────────────────────────────────────────────┘
```

## Prerequisites

### Required Software

- Docker 24.0 or later
- Docker Compose 2.20 or later
- 16GB RAM minimum (32GB recommended)
- 100GB free disk space
- Linux host (Ubuntu 22.04+ or similar)

### Required Certificates

Generate TLS certificates before deployment:

```bash
# Run certificate generation script
cd ../scripts
./generate-certs.sh

# Verify certificates are present
ls -la ../certs/
# Should contain: ca.crt, node1.crt, node1.key, node2.crt, node2.key, etc.
```

See `../scripts/README.md` for certificate generation instructions.

## Quick Start

### 1. Clone and Configure

```bash
cd production/docker

# Copy environment template
cp .env.example .env

# Edit configuration (IMPORTANT: Set strong passwords!)
nano .env
```

### 2. Build Images

```bash
# Build the node image
docker-compose build
```

### 3. Start Services

```bash
# Start all services in detached mode
docker-compose up -d

# Watch logs
docker-compose logs -f

# Check service health
docker-compose ps
```

### 4. Verify Deployment

```bash
# Check etcd cluster health
docker exec mpc-etcd-1 etcdctl endpoint health --cluster

# Check PostgreSQL
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "SELECT version();"

# Check node health
curl http://localhost:8081/health
curl http://localhost:8082/health
```

## Configuration

### Environment Variables

Edit `.env` file with your configuration:

```bash
# Database
POSTGRES_USER=mpc
POSTGRES_PASSWORD=your_strong_password_here
POSTGRES_DB=mpc_wallet

# Cluster
THRESHOLD=4          # Minimum signatures required
TOTAL_NODES=5        # Total nodes in cluster

# Bitcoin
BITCOIN_NETWORK=testnet
ESPLORA_URL=https://blockstream.info/testnet/api

# Certificates
CERTS_PATH=../certs  # Path to TLS certificates

# Logging
RUST_LOG=info        # Log level: error, warn, info, debug, trace
```

### Network Configuration

The deployment creates two networks:

- **mpc-internal**: Internal network for infrastructure (etcd, PostgreSQL)
  - Isolated from external access
  - Used for secure inter-service communication

- **mpc-external**: External network for API access
  - Nodes expose REST APIs on this network
  - Port mappings for development (see docker-compose.dev.yml)

### Resource Limits

Each service has resource limits configured:

| Service    | Memory Limit | Memory Reserved | CPU Limit | CPU Reserved |
|------------|--------------|-----------------|-----------|--------------|
| Node       | 2GB          | 1GB             | 2 cores   | 1 core       |
| PostgreSQL | 1GB          | 512MB           | -         | -            |
| etcd       | 512MB        | 256MB           | -         | -            |

Adjust in `docker-compose.yml` under `deploy.resources`.

## Deployment

### Production Deployment

```bash
# Start services with production configuration
docker-compose up -d

# Monitor startup
docker-compose logs -f

# Wait for all services to be healthy
watch docker-compose ps
```

### Development Deployment

Use the development overlay for local development:

```bash
# Start with development configuration
docker-compose -f docker-compose.yml -f docker-compose.dev.yml up -d

# This enables:
# - Port mappings for all services
# - Debug logging
# - Volume mounts for live code updates
```

### Scaling Nodes

To change the number of nodes:

1. Update `TOTAL_NODES` and `THRESHOLD` in `.env`
2. Add/remove node services in `docker-compose.yml`
3. Update `BOOTSTRAP_PEERS` for all nodes
4. Regenerate certificates for new nodes
5. Restart the cluster

```bash
docker-compose down
docker-compose up -d
```

## Monitoring

### Health Checks

All services have health checks configured:

```bash
# Check all services
docker-compose ps

# Service-specific health
docker inspect --format='{{.State.Health.Status}}' mpc-node-1
docker inspect --format='{{.State.Health.Status}}' mpc-postgres
docker inspect --format='{{.State.Health.Status}}' mpc-etcd-1
```

### Logs

```bash
# All services
docker-compose logs -f

# Specific service
docker-compose logs -f node-1

# Filter by time
docker-compose logs --since 1h node-1

# JSON formatted logs
docker-compose logs --no-color node-1 | jq .
```

### Metrics

Access API metrics:

```bash
# Node 1 metrics
curl http://localhost:8081/api/v1/cluster/status

# Transaction statistics
curl http://localhost:8081/api/v1/transactions

# Node health
curl http://localhost:8081/api/v1/cluster/nodes
```

### Database Queries

```bash
# Connect to PostgreSQL
docker exec -it mpc-postgres psql -U mpc -d mpc_wallet

# Transaction statistics
SELECT * FROM transaction_summary;

# Node health
SELECT * FROM node_health;

# Audit log
SELECT * FROM audit_log ORDER BY timestamp DESC LIMIT 10;
```

### etcd Status

```bash
# Cluster health
docker exec mpc-etcd-1 etcdctl endpoint health --cluster

# Member list
docker exec mpc-etcd-1 etcdctl member list

# Cluster status
docker exec mpc-etcd-1 etcdctl endpoint status --cluster -w table
```

## Security

### Certificate Management

- **Never commit certificates to version control**
- Use strong passphrases for private keys
- Rotate certificates regularly (every 90 days)
- Store CA private key offline in production
- Use hardware security modules (HSM) for production keys

### Network Security

```bash
# Firewall rules (example using iptables)
# Allow only necessary ports
iptables -A INPUT -p tcp --dport 8080:8085 -j ACCEPT  # API ports
iptables -A INPUT -p tcp --dport 9000:9005 -j ACCEPT  # QUIC ports
iptables -A INPUT -j DROP  # Drop all other traffic
```

### Database Security

- Use strong passwords (20+ characters)
- Enable SSL for PostgreSQL connections
- Regular backups with encryption
- Limit connection pool size
- Enable audit logging

### Secrets Management

For production, use proper secrets management:

```bash
# Docker Swarm Secrets
echo "strong_password" | docker secret create postgres_password -

# Kubernetes Secrets
kubectl create secret generic postgres-password --from-literal=password=strong_password

# HashiCorp Vault
vault kv put secret/mpc-wallet postgres_password=strong_password
```

## Troubleshooting

### Services Won't Start

```bash
# Check logs
docker-compose logs

# Check specific service
docker-compose logs postgres
docker-compose logs etcd-1

# Verify configuration
docker-compose config

# Check disk space
df -h

# Check Docker resources
docker system df
```

### etcd Cluster Issues

```bash
# Reset etcd cluster (WARNING: Deletes all data)
docker-compose down -v
docker-compose up -d

# Check etcd logs
docker-compose logs etcd-1 etcd-2 etcd-3

# Manually verify connectivity
docker exec mpc-etcd-1 etcdctl endpoint status
```

### PostgreSQL Connection Issues

```bash
# Test connection
docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "SELECT 1;"

# Check PostgreSQL logs
docker-compose logs postgres

# Verify credentials
docker exec -it mpc-postgres env | grep POSTGRES
```

### Node Communication Issues

```bash
# Check QUIC connectivity between nodes
docker exec mpc-node-1 nc -zv node-2 9000

# Verify certificates
docker exec mpc-node-1 ls -la /certs/

# Check network
docker network inspect mpc-wallet_mpc-internal
docker network inspect mpc-wallet_mpc-external

# Test API endpoint
curl -v http://localhost:8081/health
```

### Certificate Issues

```bash
# Verify certificate validity
openssl x509 -in ../certs/node1.crt -text -noout

# Check certificate expiration
openssl x509 -in ../certs/node1.crt -enddate -noout

# Verify certificate chain
openssl verify -CAfile ../certs/ca.crt ../certs/node1.crt
```

### Performance Issues

```bash
# Check resource usage
docker stats

# Check disk I/O
iostat -x 1

# Check network
netstat -an | grep ESTABLISHED

# PostgreSQL query performance
docker exec -it mpc-postgres psql -U mpc -d mpc_wallet -c "
SELECT query, calls, total_time, mean_time
FROM pg_stat_statements
ORDER BY total_time DESC
LIMIT 10;"
```

## Production Checklist

Before deploying to production:

### Security
- [ ] Strong passwords for all services (20+ characters)
- [ ] Valid TLS certificates from trusted CA
- [ ] Certificate private keys stored securely
- [ ] Secrets management configured (Vault, AWS Secrets Manager, etc.)
- [ ] Network security groups/firewall rules configured
- [ ] PostgreSQL SSL enabled
- [ ] etcd authentication enabled
- [ ] Regular security audits scheduled

### Monitoring
- [ ] Logging aggregation configured (ELK, Splunk, etc.)
- [ ] Metrics collection configured (Prometheus, Grafana)
- [ ] Alerting rules defined
- [ ] On-call rotation established
- [ ] Runbooks documented

### Backup & Recovery
- [ ] PostgreSQL backup strategy implemented
- [ ] Backup testing performed
- [ ] Disaster recovery plan documented
- [ ] Recovery time objective (RTO) defined
- [ ] Recovery point objective (RPO) defined

### Operations
- [ ] Deployment automation configured
- [ ] Rollback procedure tested
- [ ] Scaling strategy documented
- [ ] Upgrade procedure tested
- [ ] Certificate rotation procedure documented

### Compliance
- [ ] Security audit completed
- [ ] Compliance requirements verified (SOC2, GDPR, etc.)
- [ ] Audit logging configured
- [ ] Data retention policies implemented
- [ ] Incident response plan documented

### Testing
- [ ] Load testing completed
- [ ] Failover testing completed
- [ ] Network partition testing completed
- [ ] Byzantine fault tolerance verified
- [ ] End-to-end transaction testing completed

## Maintenance

### Regular Tasks

**Daily:**
- Monitor service health
- Check disk space
- Review error logs

**Weekly:**
- Review transaction statistics
- Check Byzantine violations
- Verify backup integrity

**Monthly:**
- Rotate logs
- Update Docker images
- Security patches

**Quarterly:**
- Rotate certificates
- Disaster recovery drill
- Performance review

### Backup Procedures

```bash
# Backup PostgreSQL
docker exec mpc-postgres pg_dump -U mpc -d mpc_wallet > backup.sql

# Backup etcd
docker exec mpc-etcd-1 etcdctl snapshot save /tmp/etcd-backup.db
docker cp mpc-etcd-1:/tmp/etcd-backup.db ./etcd-backup.db

# Backup certificates
tar czf certs-backup.tar.gz ../certs/
```

### Upgrade Procedures

```bash
# 1. Backup everything
./backup.sh

# 2. Pull latest images
docker-compose pull

# 3. Rebuild local images
docker-compose build --no-cache

# 4. Rolling upgrade (one node at a time)
for i in 1 2 3 4 5; do
    docker-compose stop node-$i
    docker-compose up -d node-$i
    sleep 30  # Wait for node to rejoin
done

# 5. Verify cluster health
docker-compose ps
```

## Support

For issues and questions:

- GitHub Issues: https://github.com/your-org/mpc-wallet/issues
- Documentation: https://docs.your-org.com/mpc-wallet
- Email: support@your-org.com

## License

MIT License - See LICENSE file for details
