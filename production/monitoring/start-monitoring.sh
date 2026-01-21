#!/bin/bash

# MPC Wallet Monitoring Stack Startup Script

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Print with color
print_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if .env file exists
if [ ! -f .env ]; then
    print_error ".env file not found!"
    print_info "Creating .env from .env.example..."
    cp .env.example .env
    print_warning "Please edit .env file with your actual credentials before continuing!"
    exit 1
fi

# Source .env file
set -a
source .env
set +a

# Check required environment variables
if [ -z "$GRAFANA_ADMIN_PASSWORD" ]; then
    print_error "GRAFANA_ADMIN_PASSWORD is not set in .env file!"
    exit 1
fi

if [ -z "$POSTGRES_PASSWORD" ]; then
    print_error "POSTGRES_PASSWORD is not set in .env file!"
    exit 1
fi

print_info "Starting MPC Wallet Monitoring Stack..."
print_info "==============================================="

# Check if main MPC stack network exists
if ! docker network inspect docker_mpc-internal >/dev/null 2>&1; then
    print_warning "Main MPC stack network 'docker_mpc-internal' not found!"
    print_warning "Make sure the main MPC wallet stack is running first."
    read -p "Continue anyway? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# Pull latest images
print_info "Pulling latest Docker images..."
docker-compose -f docker-compose.monitoring.yml pull

# Start monitoring stack
print_info "Starting monitoring services..."
docker-compose -f docker-compose.monitoring.yml up -d

# Wait for services to be healthy
print_info "Waiting for services to become healthy..."
sleep 10

# Check service health
print_info "Checking service status..."
docker-compose -f docker-compose.monitoring.yml ps

# Display access information
print_info "==============================================="
print_info "Monitoring stack started successfully!"
print_info ""
print_info "Access Points:"
print_info "  - Grafana:    http://localhost:3000"
print_info "  - Prometheus: http://localhost:9090"
print_info "  - cAdvisor:   http://localhost:8081"
print_info ""
print_info "Default Grafana Credentials:"
print_info "  - Username: ${GRAFANA_ADMIN_USER:-admin}"
print_info "  - Password: (set in .env file)"
print_info ""
print_info "Available Dashboards:"
print_info "  1. MPC Cluster Overview"
print_info "  2. Byzantine Consensus Monitoring"
print_info "  3. Signature Performance"
print_info "  4. Infrastructure Monitoring"
print_info "  5. Network Monitoring"
print_info ""
print_info "To view logs:"
print_info "  docker-compose -f docker-compose.monitoring.yml logs -f"
print_info ""
print_info "To stop monitoring:"
print_info "  docker-compose -f docker-compose.monitoring.yml down"
print_info "==============================================="

# Check if Prometheus can reach targets
print_info "Checking Prometheus targets..."
sleep 5

TARGETS_UP=$(curl -s http://localhost:9090/api/v1/targets 2>/dev/null | grep -o '"health":"up"' | wc -l || echo "0")
TARGETS_TOTAL=$(curl -s http://localhost:9090/api/v1/targets 2>/dev/null | grep -o '"health":' | wc -l || echo "0")

if [ "$TARGETS_TOTAL" -gt 0 ]; then
    print_info "Prometheus targets: $TARGETS_UP/$TARGETS_TOTAL are up"
    if [ "$TARGETS_UP" -lt "$TARGETS_TOTAL" ]; then
        print_warning "Some targets are down. Check http://localhost:9090/targets for details."
    fi
else
    print_warning "Could not check Prometheus targets. Visit http://localhost:9090/targets manually."
fi

print_info "Setup complete! Visit http://localhost:3000 to access Grafana."
