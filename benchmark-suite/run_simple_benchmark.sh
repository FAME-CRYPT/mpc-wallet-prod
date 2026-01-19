#!/bin/bash

# Simple benchmark script that uses Docker exec to submit votes to running containers
# This measures the actual end-to-end performance of the running systems

set -e

RESULTS_FILE="simple_benchmark_results.txt"

echo "======================================"
echo "  MPC Wallet Simple Benchmark"
echo "======================================"
echo ""

# Check if containers are running
echo "Checking Docker containers..."
if ! docker ps | grep -q "threshold-node"; then
    echo "❌ p2p-comm containers are not running"
    echo "   Run: cd ../p2p-comm && docker-compose up -d"
    exit 1
fi

if ! docker ps | grep -q "mtls-node"; then
    echo "❌ mtls-comm containers are not running"
    echo "   Run: cd ../mtls-comm && docker-compose up -d"
    exit 1
fi

echo "✓ All containers are running"
echo ""

# Function to measure vote submission time
measure_vote() {
    local system=$1
    local container=$2
    local tx_id=$3
    local value=$4

    local start=$(date +%s%N)

    if [ "$system" = "sharedmem" ]; then
        docker exec $container /app/threshold-voting-system vote --tx-id "$tx_id" --value $value > /dev/null 2>&1
    else
        docker exec $container /app/threshold-voting vote --tx-id "$tx_id" --value $value > /dev/null 2>&1
    fi

    local end=$(date +%s%N)
    local elapsed=$(( ($end - $start) / 1000 ))  # Convert to microseconds

    echo $elapsed
}

echo "======================================"
echo "  Benchmark 1: p2p-comm (libp2p)"
echo "======================================"
echo ""

VOTE_COUNT=200
total_time_sharedmem=0
success_count_sharedmem=0

echo "Submitting $VOTE_COUNT votes to p2p-comm..."

for i in $(seq 1 $VOTE_COUNT); do
    tx_id="BENCH_SHARED_$(printf '%05d' $i)"
    value=$((i % 100))

    if latency=$(measure_vote "sharedmem" "threshold-node1" "$tx_id" $value); then
        total_time_sharedmem=$((total_time_sharedmem + latency))
        success_count_sharedmem=$((success_count_sharedmem + 1))

        if [ $((i % 10)) -eq 0 ]; then
            echo "  Progress: $i/$VOTE_COUNT votes"
        fi
    else
        echo "  ⚠️  Vote $i failed"
    fi
done

avg_latency_sharedmem=$((total_time_sharedmem / success_count_sharedmem))
# Calculate throughput: (success_count * 1000000 / total_time_us)
throughput_sharedmem=$((success_count_sharedmem * 1000000 / total_time_sharedmem))

echo ""
echo "Results:"
echo "  Success Rate:   $success_count_sharedmem/$VOTE_COUNT"
echo "  Avg Latency:    ${avg_latency_sharedmem} μs"
echo "  Throughput:     ${throughput_sharedmem} votes/sec"
echo ""

echo "======================================"
echo "  Benchmark 2: mtls-comm (pure mTLS)"
echo "======================================"
echo ""

total_time_mtls=0
success_count_mtls=0

echo "Submitting $VOTE_COUNT votes to mtls-comm..."

for i in $(seq 1 $VOTE_COUNT); do
    tx_id="BENCH_MTLS_$(printf '%05d' $i)"
    value=$((i % 100))

    if latency=$(measure_vote "mtls" "mtls-node-1" "$tx_id" $value); then
        total_time_mtls=$((total_time_mtls + latency))
        success_count_mtls=$((success_count_mtls + 1))

        if [ $((i % 10)) -eq 0 ]; then
            echo "  Progress: $i/$VOTE_COUNT votes"
        fi
    else
        echo "  ⚠️  Vote $i failed"
    fi
done

avg_latency_mtls=$((total_time_mtls / success_count_mtls))
throughput_mtls=$((success_count_mtls * 1000000 / total_time_mtls))

echo ""
echo "Results:"
echo "  Success Rate:   $success_count_mtls/$VOTE_COUNT"
echo "  Avg Latency:    ${avg_latency_mtls} μs"
echo "  Throughput:     ${throughput_mtls} votes/sec"
echo ""

echo "======================================"
echo "  COMPARISON SUMMARY"
echo "======================================"
echo ""

echo "┌─────────────────────────┬─────────────────┬─────────────────┬────────────────┐"
echo "│ Metric                  │ p2p-comm  │ mtls-comm  │ Difference     │"
echo "├─────────────────────────┼─────────────────┼─────────────────┼────────────────┤"

# Throughput comparison (percentage difference)
throughput_diff=$(( (throughput_mtls - throughput_sharedmem) * 100 / throughput_sharedmem ))
printf "│ Throughput (votes/sec)  │ %-15d │ %-15d │ %+13d%% │\n" $throughput_sharedmem $throughput_mtls $throughput_diff

# Latency comparison (percentage difference)
latency_diff=$(( (avg_latency_mtls - avg_latency_sharedmem) * 100 / avg_latency_sharedmem ))
printf "│ Avg Latency (μs)        │ %-15d │ %-15d │ %+13d%% │\n" $avg_latency_sharedmem $avg_latency_mtls $latency_diff

echo "└─────────────────────────┴─────────────────┴─────────────────┴────────────────┘"
echo ""

# Determine winner
echo "🏆 WINNER"
echo ""

if [ $throughput_mtls -gt $throughput_sharedmem ]; then
    echo "  Throughput:  mtls-comm (+${throughput_diff}%)"
else
    echo "  Throughput:  p2p-comm"
fi

if [ $avg_latency_mtls -lt $avg_latency_sharedmem ]; then
    echo "  Latency:     mtls-comm (${latency_diff}% faster)"
else
    echo "  Latency:     p2p-comm"
fi

echo ""
echo "✅ Benchmark complete!"
echo ""

# Save results to file
{
    echo "Benchmark Results - $(date)"
    echo "================================"
    echo ""
    echo "p2p-comm:"
    echo "  Throughput: $throughput_sharedmem votes/sec"
    echo "  Avg Latency: ${avg_latency_sharedmem} μs"
    echo "  Success Rate: $success_count_sharedmem/$VOTE_COUNT"
    echo ""
    echo "mtls-comm:"
    echo "  Throughput: $throughput_mtls votes/sec"
    echo "  Avg Latency: ${avg_latency_mtls} μs"
    echo "  Success Rate: $success_count_mtls/$VOTE_COUNT"
    echo ""
    echo "Comparison:"
    echo "  Throughput Difference: ${throughput_diff}%"
    echo "  Latency Difference: ${latency_diff}%"
} > "$RESULTS_FILE"

echo "Results saved to: $RESULTS_FILE"
