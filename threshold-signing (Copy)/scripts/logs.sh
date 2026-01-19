#!/bin/bash
# View logs from all services or a specific service

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

if [ -z "$1" ]; then
    echo "Following logs for all services..."
    echo "Press Ctrl+C to exit"
    echo ""
    podman-compose logs -f
else
    echo "Following logs for $1..."
    echo "Press Ctrl+C to exit"
    echo ""
    podman-compose logs -f "$1"
fi
