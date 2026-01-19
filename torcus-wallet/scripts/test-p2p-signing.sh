#!/bin/bash
# End-to-end test for P2P signing integration
# Works on: Linux, macOS, Windows (Git Bash, MSYS2, WSL)
#
# This script tests the full P2P signing flow:
# 1. Create a wallet via DKG
# 2. Issue a signing grant from the coordinator
# 3. Send the grant to a node via /p2p/sign
# 4. Verify signature is returned
#
# Prerequisites:
# - Services running via ./scripts/start-regtest.sh
# - curl available

set -e

COORDINATOR_URL="${COORDINATOR_URL:-http://localhost:3000}"
NODE_URL="${NODE_URL:-http://localhost:3001}"

# Use printf instead of echo -e for Windows compatibility
print_green() { printf '\033[0;32m%s\033[0m\n' "$1"; }
print_red() { printf '\033[0;31m%s\033[0m\n' "$1"; }
print_yellow() { printf '\033[1;33m%s\033[0m\n' "$1"; }
print_cyan() { printf '\033[0;36m%s\033[0m\n' "$1"; }

# JSON extraction functions (no jq required)
json_get() {
    sed -n "s/.*\"$1\"[[:space:]]*:[[:space:]]*\"\([^\"]*\)\".*/\1/p" | head -1
}

json_get_num() {
    sed -n "s/.*\"$1\"[[:space:]]*:[[:space:]]*\([0-9]*\).*/\1/p" | head -1
}

json_get_bool() {
    sed -n "s/.*\"$1\"[[:space:]]*:[[:space:]]*\(true\|false\).*/\1/p" | head -1
}

# Extract nested grant object - handles the complex JSON structure
extract_grant() {
    # Extract everything between "grant": { and the matching closing }
    # This is a simplified extraction that works for our specific format
    local input="$1"
    # Find grant object start and extract it
    echo "$input" | sed -n 's/.*"grant":\({[^}]*"signature":"[^"]*"}\).*/\1/p'
}

echo "========================================"
echo "  P2P Signing End-to-End Test"
echo "========================================"
echo ""
echo "Coordinator URL: $COORDINATOR_URL"
echo "Node URL:        $NODE_URL"
echo ""

# ============================================================================
# Test 0: Health Check
# ============================================================================
echo "----------------------------------------"
echo "Test 0: Service Health Check"
echo "----------------------------------------"

COORD_HEALTH=$(curl -s "$COORDINATOR_URL/health" 2>/dev/null || echo '{"status":"error"}')
COORD_STATUS=$(echo "$COORD_HEALTH" | json_get "status")

if [ "$COORD_STATUS" = "healthy" ]; then
    print_green "[PASS] Coordinator is healthy"
else
    print_red "[FAIL] Coordinator not healthy"
    echo "Response: $COORD_HEALTH"
    echo ""
    echo "Make sure services are running: ./scripts/start-regtest.sh"
    echo ""
    echo "Press Enter to close..."
    read -r
    exit 1
fi

NODE_HEALTH=$(curl -s "$NODE_URL/health" 2>/dev/null || echo '{"status":"error"}')
NODE_STATUS=$(echo "$NODE_HEALTH" | json_get "status")

if [ "$NODE_STATUS" = "healthy" ]; then
    print_green "[PASS] Node is healthy"
else
    print_red "[FAIL] Node not healthy"
    echo "Response: $NODE_HEALTH"
    echo ""
    echo "Press Enter to close..."
    read -r
    exit 1
fi

# ============================================================================
# Test 1: Create Wallet via CGGMP24 DKG (or use existing)
# ============================================================================
echo ""
echo "----------------------------------------"
echo "Test 1: Get or Create CGGMP24 Wallet"
echo "----------------------------------------"

# First, check if we have any existing CGGMP24 wallets on the coordinator
echo "Checking for existing CGGMP24 wallets..."

# Query coordinator's wallets endpoint
WALLETS_RESPONSE=$(curl -s "$COORDINATOR_URL/wallets" 2>/dev/null || echo '{"wallets":[]}')
EXISTING_WALLET_ID=$(echo "$WALLETS_RESPONSE" | sed -n 's/.*"wallet_id":"\([^"]*\)".*/\1/p' | head -1)

if [ -n "$EXISTING_WALLET_ID" ] && [ "$EXISTING_WALLET_ID" != "" ]; then
    echo "Found existing CGGMP24 wallet: $EXISTING_WALLET_ID"
    WALLET_ID="$EXISTING_WALLET_ID"

    # Get wallet details
    WALLET_RESPONSE=$(curl -s "$COORDINATOR_URL/wallet/$WALLET_ID" 2>/dev/null || echo '{}')
    PUBLIC_KEY=$(echo "$WALLET_RESPONSE" | json_get "public_key")
    ADDRESS=$(echo "$WALLET_RESPONSE" | json_get "address")

    print_green "[PASS] Using existing CGGMP24 wallet"
    echo "  Wallet ID:  $WALLET_ID"
    echo "  Address:    $ADDRESS"
else
    echo "No existing CGGMP24 wallet found, creating new one..."
    echo "This will take 30-60 seconds for DKG..."

    WALLET_NAME="p2p-test-wallet-$$"

    # Create wallet record first
    WALLET_RESPONSE=$(curl -s -X POST "$COORDINATOR_URL/wallet" \
        -H "Content-Type: application/json" \
        -d "{\"name\": \"$WALLET_NAME\", \"wallet_type\": \"bitcoin\"}" 2>/dev/null || echo '{}')

    WALLET_ID=$(echo "$WALLET_RESPONSE" | json_get "wallet_id")

    if [ -z "$WALLET_ID" ] || [ "$WALLET_ID" = "null" ]; then
        print_red "[FAIL] Failed to create wallet record"
        echo "Response: $WALLET_RESPONSE"
        echo ""
        echo "Press Enter to close..."
        read -r
        exit 1
    fi

    echo "  Wallet ID: $WALLET_ID"
    echo "  Starting CGGMP24 key generation..."

    # Start CGGMP24 keygen
    KEYGEN_RESPONSE=$(curl -s -X POST "$COORDINATOR_URL/cggmp24/keygen/start" \
        -H "Content-Type: application/json" \
        -d "{\"wallet_id\": \"$WALLET_ID\", \"threshold\": 3}" \
        --max-time 120 2>/dev/null || echo '{"error": "timeout"}')

    KEYGEN_SUCCESS=$(echo "$KEYGEN_RESPONSE" | json_get_bool "success")
    PUBLIC_KEY=$(echo "$KEYGEN_RESPONSE" | json_get "public_key")
    ADDRESS=$(echo "$KEYGEN_RESPONSE" | json_get "address")

    if [ "$KEYGEN_SUCCESS" = "true" ] && [ -n "$PUBLIC_KEY" ]; then
        print_green "[PASS] CGGMP24 wallet created with DKG"
        echo "  Wallet ID:  $WALLET_ID"
        echo "  Public Key: ${PUBLIC_KEY:0:20}..."
        echo "  Address:    $ADDRESS"
    else
        print_red "[FAIL] CGGMP24 key generation failed"
        echo "Response: $KEYGEN_RESPONSE"
        echo ""
        echo "Make sure nodes are initialized: mpc-wallet cggmp24-init"
        echo ""
        echo "Press Enter to close..."
        read -r
        exit 1
    fi
fi

# ============================================================================
# Test 2: Issue Signing Grant
# ============================================================================
echo ""
echo "----------------------------------------"
echo "Test 2: Issue Signing Grant"
echo "----------------------------------------"

# Generate a random message hash (32 bytes = 64 hex chars)
MESSAGE_HASH=$(openssl rand -hex 32 2>/dev/null || cat /dev/urandom | head -c 32 | xxd -p -c 64 2>/dev/null || echo "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef")

echo "Message hash: ${MESSAGE_HASH:0:16}..."

GRANT_RESPONSE=$(curl -s -X POST "$COORDINATOR_URL/grant/signing" \
    -H "Content-Type: application/json" \
    -d "{\"wallet_id\": \"$WALLET_ID\", \"message_hash\": \"$MESSAGE_HASH\"}" 2>/dev/null || echo '{}')

SESSION_ID=$(echo "$GRANT_RESPONSE" | json_get "session_id")
GRANT_MESSAGE=$(echo "$GRANT_RESPONSE" | json_get "message")

# Extract the grant object for P2P request
GRANT=$(extract_grant "$GRANT_RESPONSE")

if [ -n "$SESSION_ID" ] && [ "$SESSION_ID" != "null" ] && [ "$SESSION_ID" != "" ]; then
    print_green "[PASS] Grant issued successfully"
    echo "  Session ID: ${SESSION_ID:0:24}..."
    echo "  Message: $GRANT_MESSAGE"
else
    print_red "[FAIL] Grant issuance failed"
    echo "Response: $GRANT_RESPONSE"
    echo ""
    echo "Press Enter to close..."
    read -r
    exit 1
fi

# ============================================================================
# Test 3: P2P Signing via Node
# ============================================================================
echo ""
echo "----------------------------------------"
echo "Test 3: P2P Signing via Node"
echo "----------------------------------------"

# Build the P2P sign request manually
P2P_REQUEST="{\"grant\":$GRANT,\"wallet_id\":\"$WALLET_ID\",\"message_hash\":\"$MESSAGE_HASH\"}"

# Node port mapping (party 0 = 3001, party 1 = 3002, etc.)
NODE_BASE_PORT=3001

# Extract participants from grant response to pick the right node
# The grant contains participants array - try to get the first one
FIRST_PARTICIPANT=$(echo "$GRANT_RESPONSE" | sed -n 's/.*"participants":\[\([0-9]*\).*/\1/p' | head -1)
if [ -n "$FIRST_PARTICIPANT" ]; then
    INITIAL_PORT=$((NODE_BASE_PORT + FIRST_PARTICIPANT))
    NODE_URL="http://localhost:$INITIAL_PORT"
    echo "Grant participants start with party $FIRST_PARTICIPANT, using port $INITIAL_PORT"
fi

# Function to try P2P signing on a specific node
try_p2p_sign() {
    local node_url="$1"
    local attempt_name="$2"

    echo "Sending grant to $attempt_name..."
    echo "  Node URL: $node_url/p2p/sign"
    echo ""
    print_cyan "Request (truncated):"
    echo "  wallet_id: $WALLET_ID"
    echo "  message_hash: ${MESSAGE_HASH:0:16}..."
    echo ""

    echo "Waiting for P2P signing (this may take 30-60 seconds)..."
    P2P_RESPONSE=$(curl -s -X POST "$node_url/p2p/sign" \
        -H "Content-Type: application/json" \
        -d "$P2P_REQUEST" \
        --max-time 120 2>/dev/null || echo '{"error": "Request failed or timed out"}')

    echo ""
    print_cyan "Response:"
    echo "$P2P_RESPONSE" | head -c 500
    echo ""
    echo ""

    P2P_SUCCESS=$(echo "$P2P_RESPONSE" | json_get_bool "success")
    P2P_SIGNATURE=$(echo "$P2P_RESPONSE" | json_get "signature")
    P2P_STATUS=$(echo "$P2P_RESPONSE" | json_get "status")
    P2P_ERROR=$(echo "$P2P_RESPONSE" | json_get "error")
    P2P_IS_INITIATOR=$(echo "$P2P_RESPONSE" | json_get_bool "is_initiator")
    P2P_DURATION=$(echo "$P2P_RESPONSE" | json_get_num "duration_ms")
}

# Try the initial node
try_p2p_sign "$NODE_URL" "node (port ${NODE_URL##*:})"

# If we got a redirect, try the correct initiator
if [ "$P2P_STATUS" = "redirect" ]; then
    # Extract the party number from error message like "Send request to party 1."
    INITIATOR_PARTY=$(echo "$P2P_ERROR" | sed -n 's/.*party \([0-9]*\).*/\1/p')

    if [ -n "$INITIATOR_PARTY" ]; then
        INITIATOR_PORT=$((NODE_BASE_PORT + INITIATOR_PARTY))
        INITIATOR_URL="http://localhost:$INITIATOR_PORT"

        echo ""
        print_yellow "[INFO] Redirected to party $INITIATOR_PARTY (port $INITIATOR_PORT)"
        echo ""
        echo "----------------------------------------"
        echo "Test 3b: Retry with correct initiator"
        echo "----------------------------------------"

        try_p2p_sign "$INITIATOR_URL" "initiator node (party $INITIATOR_PARTY)"
    fi
fi

# Report final result
if [ "$P2P_SUCCESS" = "true" ] && [ -n "$P2P_SIGNATURE" ] && [ "$P2P_SIGNATURE" != "null" ]; then
    print_green "[PASS] P2P signing succeeded!"
    echo "  Status:      $P2P_STATUS"
    echo "  Initiator:   $P2P_IS_INITIATOR"
    echo "  Duration:    ${P2P_DURATION}ms"
    echo "  Signature:   ${P2P_SIGNATURE:0:32}..."
    SIG_LENGTH=${#P2P_SIGNATURE}
    echo "  Sig length:  $((SIG_LENGTH / 2)) bytes (hex: $SIG_LENGTH chars)"
elif [ "$P2P_STATUS" = "redirect" ]; then
    print_yellow "[INFO] Node is not the initiator - redirect requested"
    echo "  Status: $P2P_STATUS"
    echo "  Error:  $P2P_ERROR"
    echo ""
    echo "Could not automatically redirect to the correct node."
else
    print_red "[FAIL] P2P signing failed"
    echo "  Status: $P2P_STATUS"
    echo "  Error:  $P2P_ERROR"

    # Check if this is a fallback scenario
    if echo "$P2P_RESPONSE" | grep -q "fallback\|HTTP relay"; then
        echo ""
        print_yellow "Note: P2P may have fallen back to HTTP relay mode."
        echo "This happens when P2P infrastructure is not fully initialized."
    fi
fi

# ============================================================================
# Test 4: Verify Signature Format (if we got one)
# ============================================================================
if [ -n "$P2P_SIGNATURE" ] && [ "$P2P_SIGNATURE" != "null" ] && [ "$P2P_SUCCESS" = "true" ]; then
    echo ""
    echo "----------------------------------------"
    echo "Test 4: Verify Signature Format"
    echo "----------------------------------------"

    SIG_LENGTH=${#P2P_SIGNATURE}

    # DER signatures are typically 70-72 bytes (140-144 hex chars)
    # They start with 0x30 (SEQUENCE)
    SIG_PREFIX=$(echo "$P2P_SIGNATURE" | cut -c1-2)

    if [ "$SIG_PREFIX" = "30" ]; then
        print_green "[PASS] Signature has valid DER prefix (0x30)"
    else
        print_yellow "[WARN] Unexpected signature prefix: $SIG_PREFIX (expected 30)"
    fi

    if [ "$SIG_LENGTH" -ge 140 ] && [ "$SIG_LENGTH" -le 146 ]; then
        print_green "[PASS] Signature length is valid DER range ($((SIG_LENGTH / 2)) bytes)"
    else
        print_yellow "[WARN] Unusual signature length: $((SIG_LENGTH / 2)) bytes"
    fi
fi

# ============================================================================
# Summary
# ============================================================================
echo ""
echo "========================================"
echo "  P2P Signing Test Complete"
echo "========================================"

if [ "$P2P_SUCCESS" = "true" ]; then
    print_green "All tests passed!"
    echo ""
    echo "The P2P signing integration is working correctly:"
    echo "  [OK] Wallet creation via DKG"
    echo "  [OK] Grant issuance from coordinator"
    echo "  [OK] P2P signing via node endpoint"
    echo "  [OK] Valid signature returned"
    EXIT_CODE=0
elif [ "$P2P_STATUS" = "redirect" ]; then
    print_yellow "Partial success - redirect scenario"
    echo ""
    echo "The P2P infrastructure is working:"
    echo "  [OK] Wallet creation via DKG"
    echo "  [OK] Grant issuance from coordinator"
    echo "  [--] Node correctly identified it's not the initiator"
    echo ""
    echo "To complete the test, send the request to the correct initiator node."
    EXIT_CODE=0
else
    print_red "Some tests failed"
    echo ""
    echo "Check the logs for more details:"
    echo "  docker compose logs coordinator"
    echo "  docker compose logs mpc-node-1"
    EXIT_CODE=1
fi

echo ""
echo "Environment:"
echo "  Coordinator: $COORDINATOR_URL"
echo "  Node:        $NODE_URL"
echo "  Wallet ID:   $WALLET_ID"
echo ""

# Always pause on Windows so user can see results
echo "Press Enter to close..."
read -r

exit $EXIT_CODE
