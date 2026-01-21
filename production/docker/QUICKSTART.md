# MPC Wallet Quick Start Guide

Get your MPC wallet cluster running in 5 minutes!

## Prerequisites

- Docker 24.0+
- Docker Compose 2.20+
- 16GB RAM minimum
- TLS certificates (see below)

## Step 1: Generate Certificates

```bash
cd ../scripts
./generate-certs.sh
```

This creates certificates in `../certs/`:
- `ca.crt` (Certificate Authority)
- `node1.crt`, `node1.key` (Node 1 certificate and key)
- `node2.crt`, `node2.key` (Node 2 certificate and key)
- ... up to `node5.crt`, `node5.key`

## Step 2: Configure Environment

```bash
cd ../docker

# Copy environment template
cp .env.example .env

# Edit configuration
nano .env
```

**IMPORTANT**: Change these values in `.env`:
```bash
POSTGRES_PASSWORD=your_strong_password_here  # Change this!
CERTS_PATH=../certs                           # Verify path to certs
BITCOIN_NETWORK=testnet                       # or mainnet
```

## Step 3: Build and Start

```bash
# Build the Docker image
docker-compose build

# Start all services
docker-compose up -d

# Watch the logs
docker-compose logs -f
```

## Step 4: Verify Deployment

Wait 1-2 minutes for all services to start, then check:

```bash
# Check all services are running
docker-compose ps

# All services should show "Up (healthy)"

# Test node APIs
curl http://localhost:8081/health
curl http://localhost:8082/health
curl http://localhost:8083/health
curl http://localhost:8084/health
curl http://localhost:8085/health

# All should return: {"status":"ok"}
```

## Step 5: Access Your Wallet

```bash
# Get wallet address
curl http://localhost:8081/api/v1/wallet/address

# Get balance
curl http://localhost:8081/api/v1/wallet/balance

# Send transaction
curl -X POST http://localhost:8081/api/v1/transactions \
  -H "Content-Type: application/json" \
  -d '{
    "to": "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx",
    "amount_sats": 10000,
    "metadata": "Test transaction"
  }'
```

## Development Mode

For local development with port mappings and debug logging:

```bash
# Start with development configuration
docker-compose -f docker-compose.yml -f docker-compose.dev.yml up -d

# This exposes:
# - Node 1 API: http://localhost:8081
# - Node 2 API: http://localhost:8082
# - Node 3 API: http://localhost:8083
# - Node 4 API: http://localhost:8084
# - Node 5 API: http://localhost:8085
# - PostgreSQL: localhost:5432
# - etcd-1: localhost:2379
```

## Stopping the Cluster

```bash
# Stop all services
docker-compose down

# Stop and remove volumes (DELETES ALL DATA!)
docker-compose down -v
```

## Troubleshooting

### Services won't start

```bash
# Check logs
docker-compose logs

# Check disk space
df -h

# Check Docker resources
docker system df
```

### Port conflicts

If ports 8080-8085 are in use:

1. Stop conflicting services
2. Or edit `docker-compose.dev.yml` to use different ports

### Certificate errors

```bash
# Verify certificates exist
ls -la ../certs/

# Should see: ca.crt, node1.crt, node1.key, etc.

# Regenerate if needed
cd ../scripts
./generate-certs.sh
```

## Next Steps

- Read the full [README.md](README.md) for detailed documentation
- Configure monitoring and alerting
- Set up backups
- Review security checklist
- Tune performance parameters

## Common Commands

```bash
# View logs
docker-compose logs -f node-1

# Restart a node
docker-compose restart node-1

# Check etcd cluster health
docker exec mpc-etcd-1 etcdctl endpoint health --cluster

# Access PostgreSQL
docker exec -it mpc-postgres psql -U mpc -d mpc_wallet

# Check resource usage
docker stats
```

## Support

Having issues? Check:
1. [README.md](README.md) - Full documentation
2. [Troubleshooting section](README.md#troubleshooting)
3. GitHub Issues
4. Community Discord

## Architecture Overview

```
5 MPC Nodes (4-of-5 threshold)
    ↓
3-node etcd cluster (coordination)
    ↓
PostgreSQL (audit logs)
```

**Features:**
- Byzantine fault tolerance
- Threshold signatures (CGGMP24 + FROST)
- QUIC + mTLS networking
- Automatic health checks
- Production-ready configuration

Happy wallet building! 🚀
