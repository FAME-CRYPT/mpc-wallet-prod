#!/bin/bash
# End-to-end test for threshold signing system
# Tests complete flow: API request -> keygen -> signing -> signature verification

set -e

echo "=================================================="
echo "End-to-End Threshold Signing Test"
echo "=================================================="
echo ""

# Configuration
API_URL="http://localhost:8000"
MESSAGE_BOARD_URL="http://localhost:8080"

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Helper functions
print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

print_info() {
    echo -e "${YELLOW}→ $1${NC}"
}

# Check if services are running
print_info "Checking if services are running..."

if ! curl -s "$API_URL/health" > /dev/null 2>&1; then
    print_error "API Gateway is not running at $API_URL"
    echo "Start services with: podman-compose up -d"
    exit 1
fi
print_success "API Gateway is running"

if ! curl -s "$MESSAGE_BOARD_URL/health" > /dev/null 2>&1; then
    print_error "MessageBoard is not running at $MESSAGE_BOARD_URL"
    echo "Start services with: podman-compose up -d"
    exit 1
fi
print_success "MessageBoard is running"

echo ""
echo "=================================================="
echo "Step 1: Wait for nodes to complete keygen"
echo "=================================================="
print_info "Nodes need to run distributed key generation..."
print_info "This takes ~30-60 seconds on first startup"
print_info "Waiting 60 seconds..."
sleep 60
print_success "Keygen should be complete"

echo ""
echo "=================================================="
echo "Step 2: Submit signing request"
echo "=================================================="

MESSAGE="Hello from threshold signing test $(date +%s)"
print_info "Submitting message: $MESSAGE"

RESPONSE=$(curl -s -X POST "$API_URL/sign" \
    -H "Content-Type: application/json" \
    -d "{\"message\":\"$MESSAGE\"}")

REQUEST_ID=$(echo "$RESPONSE" | jq -r '.request_id')
STATUS=$(echo "$RESPONSE" | jq -r '.status')

if [ -z "$REQUEST_ID" ] || [ "$REQUEST_ID" = "null" ]; then
    print_error "Failed to submit signing request"
    echo "Response: $RESPONSE"
    exit 1
fi

print_success "Request submitted"
echo "  Request ID: $REQUEST_ID"
echo "  Status: $STATUS"

echo ""
echo "=================================================="
echo "Step 3: Wait for threshold signing"
echo "=================================================="
print_info "Waiting for 3 of 4 nodes to coordinate and sign..."
print_info "This involves multiple protocol rounds via HTTP polling"
print_info "Checking status every 5 seconds..."

# Poll for completion (max 2 minutes)
MAX_ATTEMPTS=24
ATTEMPT=0

while [ $ATTEMPT -lt $MAX_ATTEMPTS ]; do
    sleep 5
    ATTEMPT=$((ATTEMPT + 1))

    STATUS_RESPONSE=$(curl -s "$API_URL/status/$REQUEST_ID")
    CURRENT_STATUS=$(echo "$STATUS_RESPONSE" | jq -r '.status')

    echo "  Attempt $ATTEMPT/$MAX_ATTEMPTS: status=$CURRENT_STATUS"

    if [ "$CURRENT_STATUS" = "completed" ]; then
        print_success "Signing completed!"
        SIGNATURE=$(echo "$STATUS_RESPONSE" | jq -r '.signature')
        break
    fi
done

if [ "$CURRENT_STATUS" != "completed" ]; then
    print_error "Signing did not complete within 2 minutes"
    echo "Final status: $CURRENT_STATUS"
    echo ""
    echo "Check node logs with: ./scripts/logs.sh node-1"
    exit 1
fi

echo ""
echo "=================================================="
echo "Step 4: Verify signature"
echo "=================================================="

print_info "Signature received (first 100 chars):"
echo "${SIGNATURE:0:100}..."

# Save signature to temp file
TEMP_DIR=$(mktemp -d)
echo "$SIGNATURE" > "$TEMP_DIR/signature.json"

# Check if public key exists
PUBLIC_KEY_PATH="/tmp/public_key.json"
if [ ! -f "$PUBLIC_KEY_PATH" ]; then
    print_error "Public key not found at $PUBLIC_KEY_PATH"
    echo "The public key should be generated during keygen"
    echo "Check if keygen completed successfully"
    exit 1
fi

print_info "Verifying signature with verify-signature tool..."

# Run verification tool
cd node
if cargo run --bin verify-signature -- \
    --public-key "$PUBLIC_KEY_PATH" \
    --message "$MESSAGE" \
    --signature "$TEMP_DIR/signature.json" 2>&1 | grep -q "Signature is VALID"; then
    print_success "Signature is cryptographically valid!"
else
    print_error "Signature verification failed"
    exit 1
fi
cd ..

# Cleanup
rm -rf "$TEMP_DIR"

echo ""
echo "=================================================="
echo "Summary"
echo "=================================================="
print_success "All tests passed!"
echo ""
echo "Complete flow verified:"
echo "  1. API Gateway accepted signing request"
echo "  2. MessageBoard coordinated 4 nodes"
echo "  3. 3 of 4 nodes performed threshold signing"
echo "  4. Signature is cryptographically valid"
echo ""
echo "The threshold signing system is fully functional!"
echo "=================================================="
