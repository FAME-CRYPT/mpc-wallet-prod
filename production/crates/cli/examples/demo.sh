#!/bin/bash
# Demo script for threshold-wallet CLI
# This script demonstrates common CLI operations

set -e

echo "=== Threshold Wallet CLI Demo ==="
echo ""

# Set the CLI binary path
CLI="threshold-wallet"

# Check if API server is running
echo "1. Checking API server health..."
if curl -s http://localhost:8080/health > /dev/null 2>&1; then
    echo "   ✓ API server is running"
else
    echo "   ✗ API server is not running. Start it first with:"
    echo "     cargo run --package threshold-api"
    exit 1
fi
echo ""

# Configure CLI
echo "2. Configuring CLI..."
$CLI config show
echo ""

# Get wallet address
echo "3. Getting wallet address..."
$CLI wallet address
echo ""

# Get wallet balance
echo "4. Checking wallet balance..."
$CLI wallet balance
echo ""

# List transactions
echo "5. Listing transactions..."
$CLI tx list
echo ""

# Check cluster status
echo "6. Checking cluster status..."
$CLI cluster status
echo ""

# List cluster nodes
echo "7. Listing cluster nodes..."
$CLI cluster nodes
echo ""

# Example: Send transaction (commented out - requires manual confirmation)
# echo "8. Sending transaction..."
# $CLI send \
#   --to tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx \
#   --amount 50000 \
#   --metadata "Test transaction"
# echo ""

# JSON output example
echo "8. Getting balance in JSON format..."
$CLI wallet balance --json | jq '.'
echo ""

echo "=== Demo Complete ==="
echo ""
echo "Available commands:"
echo "  wallet balance        - Check wallet balance"
echo "  wallet address        - Get receiving address"
echo "  send                  - Send Bitcoin"
echo "  tx status <txid>      - Check transaction status"
echo "  tx list               - List all transactions"
echo "  cluster status        - View cluster health"
echo "  cluster nodes         - List cluster nodes"
echo "  dkg start             - Start DKG ceremony"
echo "  presig generate       - Generate presignatures"
echo ""
echo "For more help: threshold-wallet --help"
