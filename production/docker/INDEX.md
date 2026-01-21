# Docker Deployment Files Index

Complete index of all files created for the MPC Wallet Docker deployment.

## Quick Navigation

- [Core Files](#core-files)
- [Configuration](#configuration)
- [Scripts](#scripts)
- [Documentation](#documentation)
- [Source Code](#source-code)

## Core Files

### Dockerfile.node
**Path**: `production/docker/Dockerfile.node`
**Size**: 100 lines
**Purpose**: Multi-stage Docker build for MPC wallet nodes

**Features**:
- Alpine Linux base (minimal size)
- Multi-stage build (builder + runtime)
- Dependency caching optimization
- Non-root user security
- Health check integration
- Ports: 8080 (API), 9000 (QUIC)

**Usage**:
```bash
docker build -f docker/Dockerfile.node -t mpc-wallet-node .
```

---

### docker-compose.yml
**Path**: `production/docker/docker-compose.yml`
**Size**: 400+ lines
**Purpose**: Main production deployment configuration

**Services**:
- 3x etcd nodes (distributed coordination)
- 1x PostgreSQL (audit logs, transactions)
- 5x MPC wallet nodes (4-of-5 threshold)

**Networks**:
- mpc-internal (infrastructure)
- mpc-external (API access)

**Usage**:
```bash
docker-compose up -d
```

---

### docker-compose.dev.yml
**Path**: `production/docker/docker-compose.dev.yml`
**Size**: 70+ lines
**Purpose**: Development environment overlay

**Features**:
- Port mappings for all services
- Debug logging enabled
- Volume mounts for live development
- Easy access to internal services

**Usage**:
```bash
docker-compose -f docker-compose.yml -f docker-compose.dev.yml up -d
```

---

### docker-compose.prod.yml
**Path**: `production/docker/docker-compose.prod.yml`
**Size**: 140+ lines
**Purpose**: Production optimizations overlay

**Features**:
- Stricter resource limits
- Log rotation configuration
- PostgreSQL performance tuning
- Always restart policy
- Production-grade settings

**Usage**:
```bash
docker-compose -f docker-compose.yml -f docker-compose.prod.yml up -d
```

---

## Configuration

### .env.example
**Path**: `production/docker/.env.example`
**Size**: 100+ lines
**Purpose**: Environment variable template

**Sections**:
- PostgreSQL configuration
- Cluster settings (threshold, total nodes)
- Bitcoin network settings
- Certificate paths
- Logging configuration
- Security checklist

**Usage**:
```bash
cp .env.example .env
nano .env  # Configure your settings
```

---

### .dockerignore
**Path**: `production/.dockerignore`
**Size**: 80+ lines
**Purpose**: Exclude files from Docker build context

**Excludes**:
- Build artifacts (target/)
- Documentation files
- Git files
- IDE configurations
- Test files
- Certificates (mounted at runtime)

---

## Scripts

### init-db.sh
**Path**: `production/docker/scripts/init-db.sh`
**Size**: 80+ lines
**Purpose**: PostgreSQL initialization after schema creation

**Functions**:
- Creates helper functions (cleanup_old_audit_logs, get_transaction_stats)
- Initializes node_status entries for 5 nodes
- Logs initialization event
- Displays initialization summary

**Runs**: Automatically via docker-entrypoint-initdb.d

---

### wait-for-it.sh
**Path**: `production/docker/scripts/wait-for-it.sh`
**Size**: 150+ lines
**Purpose**: Wait for services to be available

**Features**:
- TCP port checking
- Configurable timeout
- Multiple service support
- Command execution after ready

**Usage**:
```bash
./wait-for-it.sh postgres:5432 -t 30
```

---

### healthcheck.sh
**Path**: `production/docker/scripts/healthcheck.sh`
**Size**: 50+ lines
**Purpose**: Container health check implementation

**Checks**:
1. Port availability (netcat)
2. HTTP endpoint response (/health)
3. Response content validation

**Returns**: Exit code 0 (healthy) or 1 (unhealthy)

---

### validate-deployment.sh
**Path**: `production/docker/scripts/validate-deployment.sh`
**Size**: 300+ lines
**Purpose**: Comprehensive deployment validation

**Validates**:
- Docker and docker-compose availability
- Environment configuration
- Certificate presence
- Container status and health
- etcd cluster health
- PostgreSQL connectivity and schema
- Node API endpoints
- Network configuration
- Volume persistence
- Resource usage

**Usage**:
```bash
./scripts/validate-deployment.sh
```

**Output**: Color-coded pass/fail with summary

---

## Documentation

### README.md
**Path**: `production/docker/README.md`
**Size**: 700+ lines
**Purpose**: Comprehensive deployment guide

**Sections**:
- Overview and architecture
- Prerequisites
- Quick start guide
- Configuration instructions
- Production deployment
- Development deployment
- Monitoring and logging
- Security best practices
- Troubleshooting
- Production checklist
- Maintenance procedures
- Backup and recovery

---

### QUICKSTART.md
**Path**: `production/docker/QUICKSTART.md`
**Size**: 200+ lines
**Purpose**: Quick deployment guide (5 minutes)

**Contents**:
- Prerequisites
- 5-step deployment process
- Verification commands
- Development mode setup
- Common commands
- Troubleshooting basics

---

### DEPLOYMENT_SUMMARY.md
**Path**: `production/docker/DEPLOYMENT_SUMMARY.md`
**Size**: 400+ lines
**Purpose**: Technical implementation summary

**Contents**:
- Created files listing
- Architecture diagram
- Key features
- Configuration summary
- Deployment modes
- Security checklist
- Testing checklist
- Support information

---

### INDEX.md (this file)
**Path**: `production/docker/INDEX.md`
**Purpose**: Complete file index with descriptions

---

## Build Tools

### Makefile
**Path**: `production/docker/Makefile`
**Size**: 200+ lines
**Purpose**: Simplified deployment commands

**Commands** (20+):
- `make help` - Show all commands
- `make build` - Build images
- `make up` - Start services
- `make down` - Stop services
- `make logs` - View logs
- `make health` - Check health
- `make backup` - Backup data
- `make upgrade` - Rolling upgrade
- `make validate` - Validate config
- `make init` - Initialize deployment

**Usage**:
```bash
make help
make init
make health
```

---

## Source Code

### server.rs
**Path**: `production/crates/api/src/bin/server.rs`
**Size**: 130+ lines
**Purpose**: Main API server binary

**Features**:
- Environment-based configuration
- PostgreSQL connection pool initialization
- etcd client initialization
- Audit logger setup
- Graceful startup with logging
- Password masking in logs

**Binary Name**: `mpc-wallet-server`

**Runs**: As the main process in Docker containers

---

### Cargo.toml (updated)
**Path**: `production/crates/api/Cargo.toml`
**Purpose**: API crate configuration with binary target

**Added**:
```toml
[[bin]]
name = "mpc-wallet-server"
path = "src/bin/server.rs"
```

---

## File Statistics

### Total Files Created
- **17 files** across 4 categories
- **2,773 total lines of code**
- **All scripts executable** (chmod +x)

### Breakdown by Type
- Docker files: 4 (Dockerfile, docker-compose variants)
- Configuration: 2 (.env.example, .dockerignore)
- Scripts: 4 (init-db, wait-for-it, healthcheck, validate)
- Documentation: 4 (README, QUICKSTART, SUMMARY, INDEX)
- Build tools: 1 (Makefile)
- Source code: 2 (server.rs, Cargo.toml update)

### Languages Used
- Dockerfile
- YAML (Docker Compose)
- Shell Script (Bash)
- Markdown (Documentation)
- Makefile
- Rust (Server binary)

---

## Directory Structure

```
production/
├── docker/
│   ├── Dockerfile.node              # Node container build
│   ├── docker-compose.yml           # Production deployment
│   ├── docker-compose.dev.yml       # Development overlay
│   ├── docker-compose.prod.yml      # Production overlay
│   ├── .env.example                 # Environment template
│   ├── Makefile                     # Build automation
│   ├── README.md                    # Main documentation
│   ├── QUICKSTART.md                # Quick start guide
│   ├── DEPLOYMENT_SUMMARY.md        # Implementation summary
│   ├── INDEX.md                     # This file
│   └── scripts/
│       ├── init-db.sh              # DB initialization
│       ├── wait-for-it.sh          # Service readiness
│       ├── healthcheck.sh          # Health checks
│       └── validate-deployment.sh   # Validation script
├── .dockerignore                    # Build exclusions
└── crates/
    └── api/
        ├── Cargo.toml              # Updated with binary
        └── src/
            └── bin/
                └── server.rs       # API server binary
```

---

## Getting Started

1. **Read Documentation**
   - Start with [QUICKSTART.md](QUICKSTART.md)
   - Review [README.md](README.md) for details
   - Check [DEPLOYMENT_SUMMARY.md](DEPLOYMENT_SUMMARY.md) for technical details

2. **Generate Certificates**
   ```bash
   cd ../scripts
   ./generate-certs.sh
   ```

3. **Configure Environment**
   ```bash
   cd docker
   cp .env.example .env
   # Edit .env with your settings
   ```

4. **Deploy**
   ```bash
   make init
   # or
   docker-compose up -d
   ```

5. **Validate**
   ```bash
   ./scripts/validate-deployment.sh
   ```

---

## Quick Reference

### Essential Commands
```bash
# Build
docker-compose build

# Start (production)
docker-compose -f docker-compose.yml -f docker-compose.prod.yml up -d

# Start (development)
docker-compose -f docker-compose.yml -f docker-compose.dev.yml up -d

# Stop
docker-compose down

# Logs
docker-compose logs -f

# Health
docker-compose ps

# Validate
./scripts/validate-deployment.sh
```

### Essential Ports (Development)
- Node APIs: 8081-8085
- Node QUIC: 9001-9005
- PostgreSQL: 5432
- etcd: 2379, 22379, 32379

### Essential URLs
- Node 1 Health: http://localhost:8081/health
- Node 1 API: http://localhost:8081/api/v1/
- Node 1 Wallet: http://localhost:8081/api/v1/wallet/balance

---

## Support Resources

- **README.md**: Full documentation
- **QUICKSTART.md**: Quick deployment
- **DEPLOYMENT_SUMMARY.md**: Technical details
- **Makefile**: `make help` for commands
- **validate-deployment.sh**: Health checks

---

## Version Information

- **Docker**: 24.0+
- **Docker Compose**: 2.20+
- **Alpine Linux**: 3.19
- **Rust**: 1.75
- **PostgreSQL**: 16
- **etcd**: 3.5.11

---

## License

MIT License - See project LICENSE file

---

Last Updated: 2026-01-20
