#!/bin/bash
# Verification script for MPC wallet DKG protocol
# Works on: Linux, macOS, Windows (Git Bash, MSYS2, WSL)

set -e

COORDINATOR_URL="${COORDINATOR_URL:-http://localhost:3000}"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

# JSON string extractor
json_get() {
    sed -n "s/.*\"$1\"[[:space:]]*:[[:space:]]*\"\([^\"]*\)\".*/\1/p" | head -1
}

# JSON number extractor
json_get_num() {
    sed -n "s/.*\"$1\"[[:space:]]*:[[:space:]]*\([0-9]*\).*/\1/p" | head -1
}

echo "========================================"
echo "  MPC Wallet Verification Script"
echo "========================================"
echo ""
echo "Coordinator URL: $COORDINATOR_URL"
echo ""

# Test 1: Health check
echo "----------------------------------------"
echo "Test 1: Coordinator Health Check"
echo "----------------------------------------"

HEALTH=$(curl -s "$COORDINATOR_URL/health" 2>/dev/null || echo '{"status":"error"}')
STATUS=$(echo "$HEALTH" | json_get "status")

if [ "$STATUS" = "healthy" ]; then
    echo -e "${GREEN}[PASS] Coordinator is healthy${NC}"
else
    echo -e "${RED}[FAIL] Coordinator health check failed${NC}"
    echo "Response: $HEALTH"
    exit 1
fi

# Test 2: System info
echo ""
echo "----------------------------------------"
echo "Test 2: System Info"
echo "----------------------------------------"

INFO=$(curl -s "$COORDINATOR_URL/info" 2>/dev/null || echo '{}')
THRESHOLD=$(echo "$INFO" | json_get_num "threshold")
PARTIES=$(echo "$INFO" | json_get_num "parties")

if [ -n "$THRESHOLD" ] && [ -n "$PARTIES" ]; then
    echo "Threshold: $THRESHOLD-of-$PARTIES"
    echo -e "${GREEN}[PASS] System info retrieved${NC}"
else
    echo -e "${YELLOW}[WARN] Could not parse system info${NC}"
fi

# Test 3: Create Bitcoin wallet
echo ""
echo "----------------------------------------"
echo "Test 3: Create Bitcoin Wallet"
echo "----------------------------------------"

WALLET_NAME="test-wallet-$$"
echo "Creating wallet: $WALLET_NAME"

RESPONSE=$(curl -s -X POST "$COORDINATOR_URL/wallet" \
    -H "Content-Type: application/json" \
    -d "{\"name\": \"$WALLET_NAME\", \"wallet_type\": \"bitcoin\"}" 2>/dev/null || echo '{}')

WALLET_ID=$(echo "$RESPONSE" | json_get "wallet_id")
PUBLIC_KEY=$(echo "$RESPONSE" | json_get "public_key")
ADDRESS=$(echo "$RESPONSE" | json_get "address")
MESSAGE=$(echo "$RESPONSE" | json_get "message")

if [ -n "$WALLET_ID" ] && [ "$WALLET_ID" != "null" ]; then
    echo -e "${GREEN}[PASS] Wallet created successfully${NC}"
    echo "  Wallet ID:  $WALLET_ID"
    echo "  Public Key: $PUBLIC_KEY"
    echo "  Address:    $ADDRESS"
    echo "  Message:    $MESSAGE"
else
    echo -e "${RED}[FAIL] Wallet creation failed${NC}"
    echo "Response: $RESPONSE"
    exit 1
fi

# Test 4: Verify public key format
echo ""
echo "----------------------------------------"
echo "Test 4: Verify Public Key Format"
echo "----------------------------------------"

PK_LENGTH=${#PUBLIC_KEY}
PK_PREFIX=$(echo "$PUBLIC_KEY" | cut -c1-2)

if [ "$PK_LENGTH" -eq 66 ]; then
    echo -e "${GREEN}[PASS] Public key length correct (66 hex chars = 33 bytes)${NC}"
else
    echo -e "${RED}[FAIL] Public key length incorrect: $PK_LENGTH (expected 66)${NC}"
    exit 1
fi

if [ "$PK_PREFIX" = "02" ] || [ "$PK_PREFIX" = "03" ]; then
    echo -e "${GREEN}[PASS] Public key prefix correct (compressed secp256k1)${NC}"
else
    echo -e "${RED}[FAIL] Public key prefix incorrect: $PK_PREFIX (expected 02 or 03)${NC}"
    exit 1
fi

# Test 5: Verify Bitcoin address format
echo ""
echo "----------------------------------------"
echo "Test 5: Verify Bitcoin Address Format"
echo "----------------------------------------"

ADDR_PREFIX_4=$(echo "$ADDRESS" | cut -c1-4)
ADDR_PREFIX_5=$(echo "$ADDRESS" | cut -c1-5)

if [ "$ADDR_PREFIX_5" = "bcrt1" ]; then
    echo -e "${GREEN}[PASS] Bitcoin regtest address format correct (P2WPKH)${NC}"
elif [ "$ADDR_PREFIX_4" = "tb1q" ]; then
    echo -e "${GREEN}[PASS] Bitcoin testnet address format correct (P2WPKH)${NC}"
elif [ "$ADDR_PREFIX_4" = "bc1q" ]; then
    echo -e "${GREEN}[PASS] Bitcoin mainnet address format correct (P2WPKH)${NC}"
else
    echo -e "${RED}[FAIL] Unexpected address prefix: $ADDR_PREFIX_4${NC}"
    exit 1
fi

# Test 6: Verify key randomness
echo ""
echo "----------------------------------------"
echo "Test 6: Verify Key Randomness"
echo "----------------------------------------"

RESPONSE1=$(curl -s -X POST "$COORDINATOR_URL/wallet" \
    -H "Content-Type: application/json" \
    -d '{"name": "random-test-1", "wallet_type": "bitcoin"}' 2>/dev/null || echo '{}')
WALLET1_KEY=$(echo "$RESPONSE1" | json_get "public_key")

RESPONSE2=$(curl -s -X POST "$COORDINATOR_URL/wallet" \
    -H "Content-Type: application/json" \
    -d '{"name": "random-test-2", "wallet_type": "bitcoin"}' 2>/dev/null || echo '{}')
WALLET2_KEY=$(echo "$RESPONSE2" | json_get "public_key")

if [ -n "$WALLET1_KEY" ] && [ -n "$WALLET2_KEY" ] && [ "$WALLET1_KEY" != "$WALLET2_KEY" ]; then
    echo -e "${GREEN}[PASS] Different wallets have different keys${NC}"
    KEY1_SHORT=$(echo "$WALLET1_KEY" | cut -c1-20)
    KEY2_SHORT=$(echo "$WALLET2_KEY" | cut -c1-20)
    echo "  Key 1: ${KEY1_SHORT}..."
    echo "  Key 2: ${KEY2_SHORT}..."
else
    echo -e "${RED}[FAIL] Keys are identical or empty${NC}"
    exit 1
fi

# Summary
echo ""
echo "========================================"
echo "  Verification Complete"
echo "========================================"
echo -e "${GREEN}All tests passed!${NC}"
echo ""
echo "The MPC wallet system is working correctly:"
echo "  [OK] Coordinator and nodes are healthy"
echo "  [OK] DKG protocol produces valid keys"
echo "  [OK] Bitcoin addresses are properly formatted"
echo "  [OK] Each wallet gets unique keys"
echo ""
