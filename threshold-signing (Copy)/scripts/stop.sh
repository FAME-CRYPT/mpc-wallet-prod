#!/bin/bash
# Stop the threshold signing system

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

echo "Stopping Threshold Signing System..."
echo ""

podman-compose down

echo ""
echo "✓ System stopped successfully!"
echo ""
