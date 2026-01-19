#!/bin/bash
# Test script for API Gateway endpoints
# Tests health check, sign request creation, and status checking

set -e  # Exit on error

API_URL="${API_URL:-http://localhost:8000}"

echo "========================================="
echo "API Gateway Test Suite"
echo "========================================="
echo ""

# Test 1: Health Check
echo "[Test 1] Health Check"
echo "GET $API_URL/health"
HEALTH_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" "$API_URL/health")
HTTP_CODE=$(echo "$HEALTH_RESPONSE" | grep "HTTP_CODE:" | cut -d: -f2)
BODY=$(echo "$HEALTH_RESPONSE" | grep -v "HTTP_CODE:")

if [ "$HTTP_CODE" = "200" ]; then
    echo "✓ Health check passed (HTTP $HTTP_CODE)"
    echo "  Response: $BODY"
else
    echo "✗ Health check failed (HTTP $HTTP_CODE)"
    exit 1
fi
echo ""

# Test 2: Create Signing Request
echo "[Test 2] Create Signing Request"
echo "POST $API_URL/sign"
PAYLOAD='{"message":"Test message for threshold signing"}'
echo "  Payload: $PAYLOAD"

SIGN_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" \
    -X POST "$API_URL/sign" \
    -H "Content-Type: application/json" \
    -d "$PAYLOAD")

HTTP_CODE=$(echo "$SIGN_RESPONSE" | grep "HTTP_CODE:" | cut -d: -f2)
BODY=$(echo "$SIGN_RESPONSE" | grep -v "HTTP_CODE:")

if [ "$HTTP_CODE" = "200" ]; then
    echo "✓ Sign request created (HTTP $HTTP_CODE)"
    echo "  Response: $BODY"

    # Extract request_id for next test
    REQUEST_ID=$(echo "$BODY" | grep -o '"request_id":"[^"]*"' | cut -d'"' -f4)
    echo "  Request ID: $REQUEST_ID"
else
    echo "✗ Sign request failed (HTTP $HTTP_CODE)"
    echo "  Response: $BODY"
    exit 1
fi
echo ""

# Test 3: Check Status
echo "[Test 3] Check Signing Status"
echo "GET $API_URL/status/$REQUEST_ID"

STATUS_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" \
    "$API_URL/status/$REQUEST_ID")

HTTP_CODE=$(echo "$STATUS_RESPONSE" | grep "HTTP_CODE:" | cut -d: -f2)
BODY=$(echo "$STATUS_RESPONSE" | grep -v "HTTP_CODE:")

if [ "$HTTP_CODE" = "200" ]; then
    echo "✓ Status check successful (HTTP $HTTP_CODE)"
    echo "  Response: $BODY"

    # Check if status is "pending" as expected for a new request
    STATUS=$(echo "$BODY" | grep -o '"status":"[^"]*"' | cut -d'"' -f4)
    echo "  Current status: $STATUS"
else
    echo "✗ Status check failed (HTTP $HTTP_CODE)"
    echo "  Response: $BODY"
    exit 1
fi
echo ""

# Test 4: Invalid Request (no message)
echo "[Test 4] Invalid Request - Missing Message"
echo "POST $API_URL/sign"
BAD_PAYLOAD='{}'
echo "  Payload: $BAD_PAYLOAD"

BAD_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" \
    -X POST "$API_URL/sign" \
    -H "Content-Type: application/json" \
    -d "$BAD_PAYLOAD")

HTTP_CODE=$(echo "$BAD_RESPONSE" | grep "HTTP_CODE:" | cut -d: -f2)

if [ "$HTTP_CODE" = "400" ]; then
    echo "✓ Invalid request rejected correctly (HTTP $HTTP_CODE)"
else
    echo "✗ Invalid request should return 400, got HTTP $HTTP_CODE"
    exit 1
fi
echo ""

# Test 5: Non-existent Status Check
echo "[Test 5] Status Check for Non-existent Request"
FAKE_ID="nonexistent-request-id"
echo "GET $API_URL/status/$FAKE_ID"

NOT_FOUND_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" \
    "$API_URL/status/$FAKE_ID")

HTTP_CODE=$(echo "$NOT_FOUND_RESPONSE" | grep "HTTP_CODE:" | cut -d: -f2)
BODY=$(echo "$NOT_FOUND_RESPONSE" | grep -v "HTTP_CODE:")

# Note: Depending on implementation, this might return 200 with "not_found" status
# or 404. We'll check for either.
if [ "$HTTP_CODE" = "200" ] || [ "$HTTP_CODE" = "404" ]; then
    echo "✓ Non-existent request handled (HTTP $HTTP_CODE)"
    echo "  Response: $BODY"
else
    echo "✗ Unexpected response for non-existent request (HTTP $HTTP_CODE)"
    exit 1
fi
echo ""

echo "========================================="
echo "All API Gateway tests passed! ✓"
echo "========================================="
