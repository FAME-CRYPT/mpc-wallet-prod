#!/bin/bash

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

echo "=========================================="
echo "  MPC Wallet - Starting Services"
echo "=========================================="
echo ""
echo "Configuration:"
echo "  - Nodes: 4"
echo "  - Threshold: 3-of-4"
echo ""

# Build and start services
echo "Building Docker images..."
docker compose build

echo ""
echo "Starting services..."
docker compose up -d

echo ""
echo "Waiting for services to be healthy..."
sleep 5

# Check health of all services
echo ""
echo "Service Status:"
echo "---------------"

for service in coordinator mpc-node-1 mpc-node-2 mpc-node-3 mpc-node-4; do
    status=$(docker inspect --format='{{.State.Health.Status}}' "mpc-${service}" 2>/dev/null || echo "unknown")
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
echo "  Services Started!"
echo "=========================================="
echo ""
echo "Coordinator is available at: http://localhost:3000"
echo ""
echo "To create a wallet, run:"
echo "  cargo run --bin mpc-wallet -- taproot-create --name \"My Wallet\""
echo ""
echo "Or for SegWit/ECDSA:"
echo "  cargo run --bin mpc-wallet -- cggmp24-create --name \"My Wallet\""
echo ""
echo "To view logs:"
echo "  docker compose logs -f"
echo ""
echo "To stop services:"
echo "  docker compose down"
echo ""
