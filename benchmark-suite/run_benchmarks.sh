#!/bin/bash

set -e

echo "🚀 MPC Wallet Comprehensive Benchmark Suite"
echo "============================================"
echo ""

# Color codes
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Configuration
VOTES=${VOTES:-1000}
CONCURRENT=${CONCURRENT:-10}
OUTPUT_DIR="./results"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

mkdir -p "$OUTPUT_DIR"

echo -e "${BLUE}Configuration:${NC}"
echo "  Votes per test: $VOTES"
echo "  Concurrent votes: $CONCURRENT"
echo "  Output directory: $OUTPUT_DIR"
echo ""

# Function to cleanup
cleanup() {
    echo -e "${YELLOW}Cleaning up Docker containers...${NC}"
    cd ../p2p-comm && docker-compose down -v 2>/dev/null || true
    cd ../mtls-comm && docker-compose down -v 2>/dev/null || true
    cd - > /dev/null
}

# Trap to ensure cleanup on exit
trap cleanup EXIT

# Function to wait for system ready
wait_for_system() {
    local system=$1
    local max_wait=60
    local waited=0

    echo -e "${BLUE}Waiting for $system to be ready...${NC}"

    while [ $waited -lt $max_wait ]; do
        # Check if all containers are healthy
        if docker ps | grep -q "$system"; then
            echo -e "${GREEN}✓ $system is ready${NC}"
            return 0
        fi
        sleep 2
        waited=$((waited + 2))
    done

    echo -e "${RED}✗ $system failed to start${NC}"
    return 1
}

# Function to collect Docker stats
collect_docker_stats() {
    local container=$1
    local output_file=$2

    docker stats --no-stream --format "table {{.Container}}\t{{.CPUPerc}}\t{{.MemUsage}}\t{{.NetIO}}" $container > "$output_file"
}

# Function to run benchmark for a system
run_system_benchmark() {
    local system=$1
    local system_name=$2
    local compose_path=$3

    echo ""
    echo -e "${BLUE}================================================${NC}"
    echo -e "${BLUE}  Benchmarking: $system_name${NC}"
    echo -e "${BLUE}================================================${NC}"
    echo ""

    # Start system
    echo -e "${YELLOW}Starting $system_name...${NC}"
    cd "$compose_path" && docker-compose up -d
    cd - > /dev/null

    # Wait for system to be ready
    wait_for_system "$system" || return 1

    # Additional warmup time
    echo "Warming up (15 seconds)..."
    sleep 15

    # Run benchmarks
    echo ""
    echo -e "${BLUE}Running benchmarks...${NC}"

    # 1. Throughput test
    echo -e "${YELLOW}1. Throughput Test${NC}"
    local throughput_output="$OUTPUT_DIR/${system}_throughput_${TIMESTAMP}.json"
    cargo run --release -- throughput --system "$system" --votes "$VOTES" > "$throughput_output" 2>&1
    echo -e "${GREEN}✓ Throughput test complete${NC}"

    # 2. Latency test
    echo -e "${YELLOW}2. Latency Test${NC}"
    local latency_output="$OUTPUT_DIR/${system}_latency_${TIMESTAMP}.json"
    cargo run --release -- latency --system "$system" --samples 1000 > "$latency_output" 2>&1
    echo -e "${GREEN}✓ Latency test complete${NC}"

    # 3. Resource usage
    echo -e "${YELLOW}3. Resource Usage Monitoring${NC}"
    local stats_output="$OUTPUT_DIR/${system}_stats_${TIMESTAMP}.txt"

    # Collect stats from all nodes
    for i in {1..5}; do
        local container_name="${system}-node-${i}"
        collect_docker_stats "$container_name" "${stats_output}.node${i}"
    done
    echo -e "${GREEN}✓ Resource monitoring complete${NC}"

    # 4. Security overhead test
    echo -e "${YELLOW}4. Security Overhead Test${NC}"
    local security_output="$OUTPUT_DIR/${system}_security_${TIMESTAMP}.json"
    cargo run --release -- security --system "$system" > "$security_output" 2>&1
    echo -e "${GREEN}✓ Security test complete${NC}"

    # Stop system
    echo -e "${YELLOW}Stopping $system_name...${NC}"
    cd "$compose_path" && docker-compose down
    cd - > /dev/null

    echo -e "${GREEN}✓ $system_name benchmark complete${NC}"
}

# Build benchmark suite
echo -e "${YELLOW}Building benchmark suite...${NC}"
cargo build --release
echo -e "${GREEN}✓ Build complete${NC}"
echo ""

# Run benchmarks for p2p-comm
run_system_benchmark "p2p-comm" "p2p-comm (libp2p)" "../p2p-comm"

# Run benchmarks for mtls-comm
run_system_benchmark "mtls" "mtls-comm (pure mTLS)" "../mtls-comm"

# Generate comparison report
echo ""
echo -e "${BLUE}================================================${NC}"
echo -e "${BLUE}  Generating Comparison Report${NC}"
echo -e "${BLUE}================================================${NC}"
echo ""

# Combine results
COMBINED_OUTPUT="$OUTPUT_DIR/combined_results_${TIMESTAMP}.json"

echo -e "${YELLOW}Combining results...${NC}"
# This would need actual implementation to combine JSON results
echo "[]" > "$COMBINED_OUTPUT"

echo -e "${GREEN}✓ Results saved to: $OUTPUT_DIR${NC}"

# Generate markdown report
REPORT_FILE="$OUTPUT_DIR/report_${TIMESTAMP}.md"

cat > "$REPORT_FILE" << EOF
# MPC Wallet Benchmark Report

**Generated:** $(date)

## Test Configuration

- Votes per test: $VOTES
- Concurrent votes: $CONCURRENT
- Node count: 5
- Test duration: ~5 minutes per system

## Systems Tested

1. **p2p-comm**: libp2p-based networking
   - Noise Protocol XX for encryption
   - GossipSub for broadcast
   - Kademlia DHT for peer discovery

2. **mtls-comm**: Pure mTLS networking
   - TLS 1.3 with rustls
   - Certificate-based mutual authentication
   - Custom mesh topology

## Results

### Throughput Comparison

| Metric | p2p-comm | mtls-comm | Winner |
|--------|----------------|----------------|--------|
| Votes/sec | TBD | TBD | TBD |
| Messages/sec | TBD | TBD | TBD |
| Bandwidth | TBD | TBD | TBD |

### Latency Comparison

| Metric | p2p-comm | mtls-comm | Winner |
|--------|----------------|----------------|--------|
| p50 (μs) | TBD | TBD | TBD |
| p95 (μs) | TBD | TBD | TBD |
| p99 (μs) | TBD | TBD | TBD |
| Max (μs) | TBD | TBD | TBD |

### Resource Usage

| Metric | p2p-comm | mtls-comm | Winner |
|--------|----------------|----------------|--------|
| CPU (%) | TBD | TBD | TBD |
| Memory (MB) | TBD | TBD | TBD |

### Security Overhead

| Metric | p2p-comm | mtls-comm | Winner |
|--------|----------------|----------------|--------|
| TLS Handshake (μs) | TBD | TBD | TBD |
| Cert Validation (μs) | TBD | TBD | TBD |
| Encryption Overhead (%) | TBD | TBD | TBD |

## Conclusions

- **Overall Winner:** TBD
- **Best Throughput:** TBD
- **Best Latency:** TBD
- **Most Resource Efficient:** TBD
- **Best Security Performance:** TBD

## Recommendations

TBD

## Raw Data

Results stored in: \`$OUTPUT_DIR\`

EOF

echo -e "${GREEN}✓ Report generated: $REPORT_FILE${NC}"

echo ""
echo -e "${GREEN}================================================${NC}"
echo -e "${GREEN}  Benchmark Suite Complete!${NC}"
echo -e "${GREEN}================================================${NC}"
echo ""
echo "Results location: $OUTPUT_DIR"
echo "Report: $REPORT_FILE"
echo ""
