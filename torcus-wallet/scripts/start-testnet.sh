#!/bin/bash

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

echo "=========================================="
echo "  MPC Wallet - Starting Services (TESTNET)"
echo "=========================================="
echo ""
echo "Configuration:"
echo "  - Network: Bitcoin Testnet"
echo "  - Nodes: 4"
echo "  - Threshold: 3-of-4"
echo "  - Backend: Esplora API (Blockstream)"
echo ""

# Set testnet environment
export BITCOIN_NETWORK=testnet

# Build and start services (without bitcoind)
echo "Building Docker images..."
docker compose build coordinator mpc-node-1 mpc-node-2 mpc-node-3 mpc-node-4

echo ""
echo "Starting services (without bitcoind)..."
docker compose up -d coordinator mpc-node-1 mpc-node-2 mpc-node-3 mpc-node-4

echo ""
echo "Waiting for services to be healthy..."
sleep 5

# Check health of all services
echo ""
echo "Service Status:"
echo "---------------"

for service in coordinator mpc-node-1 mpc-node-2 mpc-node-3 mpc-node-4; do
    if [ "$service" = "coordinator" ]; then
        container_name="mpc-coordinator"
    else
        container_name="$service"
    fi
    status=$(docker inspect --format='{{.State.Health.Status}}' "$container_name" 2>/dev/null || echo "starting")
    printf "  %-15s: %s\n" "$service" "$status"
done

echo ""
echo "=========================================="
echo "  Services Started! (TESTNET MODE)"
echo "=========================================="
echo ""
echo "Network:     Bitcoin Testnet"
echo "Coordinator: http://localhost:3000"
echo ""
echo "Quick Start:"
echo "  # Create a SegWit wallet (CGGMP24)"
echo "  cargo run --bin mpc-wallet -- cggmp24-create --name \"My Wallet\""
echo ""
echo "  # Create a Taproot wallet (FROST)"
echo "  cargo run --bin mpc-wallet -- taproot-create --name \"My Taproot\""
echo ""
echo "  # Get testnet coins from faucet"
echo "  cargo run --bin mpc-wallet -- faucet --wallet-id <UUID>"
echo ""
echo "  # Check balance"
echo "  cargo run --bin mpc-wallet -- balance --wallet-id <UUID>"
echo ""
echo "To view logs:  docker compose logs -f"
echo "To stop:       ./scripts/stop.sh"
echo ""
