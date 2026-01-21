#!/bin/sh
# Health check script for MPC Wallet nodes
# Returns 0 if healthy, 1 if unhealthy

set -e

# Configuration
API_PORT="${LISTEN_ADDR:-0.0.0.0:8080}"
API_HOST="127.0.0.1"
API_PORT_NUM=$(echo "$API_PORT" | cut -d: -f2)
TIMEOUT=5

# Function to check if port is listening
check_port() {
    nc -z "$API_HOST" "$API_PORT_NUM" > /dev/null 2>&1
}

# Function to check HTTP endpoint
check_http() {
    response=$(wget -q -O - --timeout="$TIMEOUT" "http://${API_HOST}:${API_PORT_NUM}/health" 2>/dev/null || echo "")

    if [ -z "$response" ]; then
        return 1
    fi

    # Check if response contains "ok" or "healthy"
    echo "$response" | grep -q -i "ok\|healthy"
}

# Main health check
main() {
    # Check 1: Is the port listening?
    if ! check_port; then
        echo "UNHEALTHY: Port $API_PORT_NUM is not listening"
        exit 1
    fi

    # Check 2: Is the HTTP endpoint responding?
    if ! check_http; then
        echo "UNHEALTHY: Health endpoint not responding"
        exit 1
    fi

    # All checks passed
    echo "HEALTHY"
    exit 0
}

main
