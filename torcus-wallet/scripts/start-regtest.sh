#!/bin/bash

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

echo "=========================================="
echo "  MPC Wallet - Starting Services (REGTEST)"
echo "=========================================="
echo ""
echo "Configuration:"
echo "  - Network: Bitcoin Regtest (local)"
echo "  - Nodes: 4"
echo "  - Threshold: 3-of-4"
echo "  - Backend: Bitcoin Core RPC"
echo ""

# Build and start all services including bitcoind
echo "Building Docker images..."
docker compose build

echo ""
echo "Starting bitcoind (regtest)..."
docker compose up -d bitcoind

echo ""
echo "Waiting for bitcoind to be ready..."
sleep 3

echo ""
echo "Starting MPC services..."
docker compose up -d coordinator mpc-node-1 mpc-node-2 mpc-node-3 mpc-node-4

echo ""
echo "Waiting for services to be healthy..."
sleep 5

# Check health of all services
echo ""
echo "Service Status:"
echo "---------------"

for service in bitcoind coordinator mpc-node-1 mpc-node-2 mpc-node-3 mpc-node-4; do
    if [ "$service" = "coordinator" ]; then
        container_name="mpc-coordinator"
    elif [ "$service" = "bitcoind" ]; then
        container_name="bitcoind-regtest"
    else
        container_name="$service"
    fi
    status=$(docker inspect --format='{{.State.Health.Status}}' "$container_name" 2>/dev/null || echo "starting")
    printf "  %-15s: %s\n" "$service" "$status"
done

echo ""
echo "=========================================="
echo "  Services Started! (REGTEST MODE)"
echo "=========================================="
echo ""
echo "Network:     Bitcoin Regtest (local)"
echo "Coordinator: http://localhost:3000"
echo "Bitcoin RPC: http://localhost:18443"
echo ""
echo "Quick Start:"
echo "  # Create a Taproot wallet (FROST)"
echo "  cargo run --bin mpc-wallet -- taproot-create --name \"Test Wallet\""
echo ""
echo "  # Mine 101 blocks to fund the wallet (coinbase needs 100 confirmations)"
echo "  cargo run --bin mpc-wallet -- mine --wallet-id <UUID> --blocks 101"
echo ""
echo "  # Check balance"
echo "  cargo run --bin mpc-wallet -- balance --wallet-id <UUID>"
echo ""
echo "  # Send Bitcoin"
echo "  cargo run --bin mpc-wallet -- taproot-send --wallet-id <UUID> --to <ADDRESS> --amount 100000000"
echo ""
echo "  # Mine a block to confirm transaction"
echo "  cargo run --bin mpc-wallet -- mine --wallet-id <UUID> --blocks 1"
echo ""
echo "To view logs:  docker compose logs -f"
echo "To stop:       ./scripts/stop.sh"
echo ""
