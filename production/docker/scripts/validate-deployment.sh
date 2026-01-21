#!/bin/bash
# Deployment Validation Script
# Verifies that all services are running correctly

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Counters
PASSED=0
FAILED=0

# Helper functions
print_success() {
    echo -e "${GREEN}✓${NC} $1"
    ((PASSED++))
}

print_failure() {
    echo -e "${RED}✗${NC} $1"
    ((FAILED++))
}

print_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

print_section() {
    echo ""
    echo "======================================"
    echo "$1"
    echo "======================================"
}

# Check if Docker is running
check_docker() {
    if ! docker info > /dev/null 2>&1; then
        print_failure "Docker is not running"
        exit 1
    fi
    print_success "Docker is running"
}

# Check if docker-compose is available
check_docker_compose() {
    if ! command -v docker-compose &> /dev/null; then
        print_failure "docker-compose is not installed"
        exit 1
    fi
    print_success "docker-compose is available"
}

# Check if .env file exists
check_env_file() {
    if [ ! -f .env ]; then
        print_failure ".env file not found"
        print_warning "Copy .env.example to .env and configure it"
        exit 1
    fi
    print_success ".env file exists"
}

# Check if certificates exist
check_certificates() {
    source .env
    if [ ! -d "$CERTS_PATH" ]; then
        print_failure "Certificates directory not found: $CERTS_PATH"
        exit 1
    fi

    required_certs=("ca.crt" "node1.crt" "node1.key" "node2.crt" "node2.key" "node3.crt" "node3.key" "node4.crt" "node4.key" "node5.crt" "node5.key")
    for cert in "${required_certs[@]}"; do
        if [ ! -f "$CERTS_PATH/$cert" ]; then
            print_failure "Certificate not found: $cert"
            return
        fi
    done
    print_success "All certificates found"
}

# Check container status
check_containers() {
    containers=("mpc-etcd-1" "mpc-etcd-2" "mpc-etcd-3" "mpc-postgres" "mpc-node-1" "mpc-node-2" "mpc-node-3" "mpc-node-4" "mpc-node-5")

    for container in "${containers[@]}"; do
        if docker ps --format '{{.Names}}' | grep -q "^${container}$"; then
            status=$(docker inspect --format='{{.State.Status}}' "$container")
            if [ "$status" == "running" ]; then
                print_success "Container $container is running"
            else
                print_failure "Container $container is $status"
            fi
        else
            print_failure "Container $container is not running"
        fi
    done
}

# Check container health
check_health() {
    containers=("mpc-etcd-1" "mpc-etcd-2" "mpc-etcd-3" "mpc-postgres" "mpc-node-1" "mpc-node-2" "mpc-node-3" "mpc-node-4" "mpc-node-5")

    for container in "${containers[@]}"; do
        if docker ps --format '{{.Names}}' | grep -q "^${container}$"; then
            health=$(docker inspect --format='{{.State.Health.Status}}' "$container" 2>/dev/null || echo "none")
            if [ "$health" == "healthy" ]; then
                print_success "Container $container is healthy"
            elif [ "$health" == "none" ]; then
                print_warning "Container $container has no health check"
            else
                print_failure "Container $container health: $health"
            fi
        fi
    done
}

# Check etcd cluster
check_etcd_cluster() {
    if docker exec mpc-etcd-1 etcdctl endpoint health --cluster > /dev/null 2>&1; then
        print_success "etcd cluster is healthy"

        # Check cluster members
        member_count=$(docker exec mpc-etcd-1 etcdctl member list | wc -l)
        if [ "$member_count" -eq 3 ]; then
            print_success "etcd cluster has 3 members"
        else
            print_failure "etcd cluster has $member_count members (expected 3)"
        fi
    else
        print_failure "etcd cluster is unhealthy"
    fi
}

# Check PostgreSQL
check_postgres() {
    if docker exec mpc-postgres pg_isready -U mpc > /dev/null 2>&1; then
        print_success "PostgreSQL is ready"

        # Check database exists
        if docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "SELECT 1;" > /dev/null 2>&1; then
            print_success "Database mpc_wallet exists"

            # Check tables
            table_count=$(docker exec mpc-postgres psql -U mpc -d mpc_wallet -t -c "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='public';" | tr -d ' \n')
            if [ "$table_count" -gt 0 ]; then
                print_success "Database has $table_count tables"
            else
                print_failure "Database has no tables"
            fi
        else
            print_failure "Database mpc_wallet not accessible"
        fi
    else
        print_failure "PostgreSQL is not ready"
    fi
}

# Check node APIs
check_node_apis() {
    for i in 1 2 3 4 5; do
        port=$((8080 + i))
        if curl -s -f "http://localhost:$port/health" > /dev/null 2>&1; then
            print_success "Node $i API responding on port $port"
        else
            print_failure "Node $i API not responding on port $port"
        fi
    done
}

# Check network connectivity
check_networks() {
    networks=("mpc-wallet_mpc-internal" "mpc-wallet_mpc-external")

    for network in "${networks[@]}"; do
        if docker network inspect "$network" > /dev/null 2>&1; then
            print_success "Network $network exists"
        else
            print_failure "Network $network not found"
        fi
    done
}

# Check volumes
check_volumes() {
    volumes=("mpc-wallet_etcd-1-data" "mpc-wallet_etcd-2-data" "mpc-wallet_etcd-3-data" "mpc-wallet_postgres-data" "mpc-wallet_node-1-data" "mpc-wallet_node-2-data" "mpc-wallet_node-3-data" "mpc-wallet_node-4-data" "mpc-wallet_node-5-data")

    for volume in "${volumes[@]}"; do
        if docker volume inspect "$volume" > /dev/null 2>&1; then
            print_success "Volume $volume exists"
        else
            print_failure "Volume $volume not found"
        fi
    done
}

# Check resource usage
check_resources() {
    # Check memory usage
    total_memory=$(docker stats --no-stream --format "{{.MemUsage}}" | awk '{print $1}' | sed 's/MiB//' | awk '{s+=$1} END {print s}')
    if [ -n "$total_memory" ]; then
        print_success "Total memory usage: ${total_memory}MB"
    fi
}

# Main execution
main() {
    echo "MPC Wallet Deployment Validation"
    echo "================================="

    print_section "Checking Prerequisites"
    check_docker
    check_docker_compose
    check_env_file
    check_certificates

    print_section "Checking Containers"
    check_containers

    print_section "Checking Health"
    check_health

    print_section "Checking etcd Cluster"
    check_etcd_cluster

    print_section "Checking PostgreSQL"
    check_postgres

    print_section "Checking Node APIs"
    check_node_apis

    print_section "Checking Networks"
    check_networks

    print_section "Checking Volumes"
    check_volumes

    print_section "Checking Resources"
    check_resources

    # Summary
    echo ""
    echo "======================================"
    echo "Validation Summary"
    echo "======================================"
    echo -e "${GREEN}Passed: $PASSED${NC}"
    echo -e "${RED}Failed: $FAILED${NC}"
    echo ""

    if [ $FAILED -eq 0 ]; then
        echo -e "${GREEN}✓ All checks passed! Deployment is healthy.${NC}"
        exit 0
    else
        echo -e "${RED}✗ Some checks failed. Please review the output above.${NC}"
        exit 1
    fi
}

# Run main function
main
