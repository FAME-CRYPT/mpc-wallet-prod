#!/bin/bash
# Test script for MessageBoard endpoints
# Tests request management and node message exchange

set -e  # Exit on error

MB_URL="${MB_URL:-http://localhost:8080}"

echo "========================================="
echo "MessageBoard Test Suite"
echo "========================================="
echo ""

# Test 1: Health Check
echo "[Test 1] Health Check"
echo "GET $MB_URL/health"
HEALTH_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" "$MB_URL/health")
HTTP_CODE=$(echo "$HEALTH_RESPONSE" | grep "HTTP_CODE:" | cut -d: -f2)

if [ "$HTTP_CODE" = "200" ]; then
    echo "✓ Health check passed (HTTP $HTTP_CODE)"
else
    echo "✗ Health check failed (HTTP $HTTP_CODE)"
    exit 1
fi
echo ""

# Test 2: Create Signing Request
echo "[Test 2] Create Signing Request"
echo "POST $MB_URL/requests"
PAYLOAD='{"message":"Test message from direct MessageBoard access"}'
echo "  Payload: $PAYLOAD"

CREATE_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" \
    -X POST "$MB_URL/requests" \
    -H "Content-Type: application/json" \
    -d "$PAYLOAD")

HTTP_CODE=$(echo "$CREATE_RESPONSE" | grep "HTTP_CODE:" | cut -d: -f2)
BODY=$(echo "$CREATE_RESPONSE" | grep -v "HTTP_CODE:")

if [ "$HTTP_CODE" = "201" ] || [ "$HTTP_CODE" = "200" ]; then
    echo "✓ Request created (HTTP $HTTP_CODE)"
    echo "  Response: $BODY"

    REQUEST_ID=$(echo "$BODY" | grep -o '"request_id":"[^"]*"' | cut -d'"' -f4)
    echo "  Request ID: $REQUEST_ID"
else
    echo "✗ Request creation failed (HTTP $HTTP_CODE)"
    echo "  Response: $BODY"
    exit 1
fi
echo ""

# Test 3: Get Request by ID
echo "[Test 3] Get Request by ID"
echo "GET $MB_URL/requests/$REQUEST_ID"

GET_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" \
    "$MB_URL/requests/$REQUEST_ID")

HTTP_CODE=$(echo "$GET_RESPONSE" | grep "HTTP_CODE:" | cut -d: -f2)
BODY=$(echo "$GET_RESPONSE" | grep -v "HTTP_CODE:")

if [ "$HTTP_CODE" = "200" ]; then
    echo "✓ Request retrieved (HTTP $HTTP_CODE)"
    echo "  Response: $BODY"
else
    echo "✗ Request retrieval failed (HTTP $HTTP_CODE)"
    exit 1
fi
echo ""

# Test 4: Post Message from Node
echo "[Test 4] Post Message from Node"
echo "POST $MB_URL/messages"
MSG_PAYLOAD='{
  "request_id":"'"$REQUEST_ID"'",
  "from_node":"node-1",
  "to_node":"node-2",
  "round":1,
  "payload":"{\"test\":\"protocol message\"}"
}'
echo "  Payload: $MSG_PAYLOAD"

POST_MSG_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" \
    -X POST "$MB_URL/messages" \
    -H "Content-Type: application/json" \
    -d "$MSG_PAYLOAD")

HTTP_CODE=$(echo "$POST_MSG_RESPONSE" | grep "HTTP_CODE:" | cut -d: -f2)
BODY=$(echo "$POST_MSG_RESPONSE" | grep -v "HTTP_CODE:")

if [ "$HTTP_CODE" = "201" ] || [ "$HTTP_CODE" = "200" ]; then
    echo "✓ Message posted (HTTP $HTTP_CODE)"
    echo "  Response: $BODY"
else
    echo "✗ Message posting failed (HTTP $HTTP_CODE)"
    echo "  Response: $BODY"
    exit 1
fi
echo ""

# Test 5: Get Messages for Request
echo "[Test 5] Get Messages for Request (filtered by node)"
echo "GET $MB_URL/messages?request_id=$REQUEST_ID&to_node=node-2"

GET_MSG_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" \
    "$MB_URL/messages?request_id=$REQUEST_ID&to_node=node-2")

HTTP_CODE=$(echo "$GET_MSG_RESPONSE" | grep "HTTP_CODE:" | cut -d: -f2)
BODY=$(echo "$GET_MSG_RESPONSE" | grep -v "HTTP_CODE:")

if [ "$HTTP_CODE" = "200" ]; then
    echo "✓ Messages retrieved (HTTP $HTTP_CODE)"
    echo "  Response: $BODY"

    # Check if our message is in the response
    if echo "$BODY" | grep -q "node-1"; then
        echo "  ✓ Message from node-1 found in results"
    else
        echo "  ✗ Expected message not found"
        exit 1
    fi
else
    echo "✗ Message retrieval failed (HTTP $HTTP_CODE)"
    exit 1
fi
echo ""

# Test 6: Post Broadcast Message
echo "[Test 6] Post Broadcast Message"
echo "POST $MB_URL/messages"
BROADCAST_PAYLOAD='{
  "request_id":"'"$REQUEST_ID"'",
  "from_node":"node-1",
  "to_node":"*",
  "round":1,
  "payload":"{\"broadcast\":true}"
}'
echo "  Payload: $BROADCAST_PAYLOAD"

BROADCAST_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" \
    -X POST "$MB_URL/messages" \
    -H "Content-Type: application/json" \
    -d "$BROADCAST_PAYLOAD")

HTTP_CODE=$(echo "$BROADCAST_RESPONSE" | grep "HTTP_CODE:" | cut -d: -f2)

if [ "$HTTP_CODE" = "201" ] || [ "$HTTP_CODE" = "200" ]; then
    echo "✓ Broadcast message posted (HTTP $HTTP_CODE)"
else
    echo "✗ Broadcast message failed (HTTP $HTTP_CODE)"
    exit 1
fi
echo ""

# Test 7: Update Request Status
echo "[Test 7] Update Request Status"
echo "PUT $MB_URL/requests/$REQUEST_ID"
UPDATE_PAYLOAD='{"status":"in_progress"}'
echo "  Payload: $UPDATE_PAYLOAD"

UPDATE_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" \
    -X PUT "$MB_URL/requests/$REQUEST_ID" \
    -H "Content-Type: application/json" \
    -d "$UPDATE_PAYLOAD")

HTTP_CODE=$(echo "$UPDATE_RESPONSE" | grep "HTTP_CODE:" | cut -d: -f2)

if [ "$HTTP_CODE" = "200" ]; then
    echo "✓ Request status updated (HTTP $HTTP_CODE)"
else
    echo "✗ Request status update failed (HTTP $HTTP_CODE)"
    exit 1
fi
echo ""

# Test 8: Set Signature
echo "[Test 8] Set Signature (Complete Request)"
echo "PUT $MB_URL/requests/$REQUEST_ID"
SIG_PAYLOAD='{"signature":"3045022100abcdef1234567890"}'
echo "  Payload: $SIG_PAYLOAD"

SIG_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" \
    -X PUT "$MB_URL/requests/$REQUEST_ID" \
    -H "Content-Type: application/json" \
    -d "$SIG_PAYLOAD")

HTTP_CODE=$(echo "$SIG_RESPONSE" | grep "HTTP_CODE:" | cut -d: -f2)

if [ "$HTTP_CODE" = "200" ]; then
    echo "✓ Signature set (HTTP $HTTP_CODE)"
else
    echo "✗ Signature setting failed (HTTP $HTTP_CODE)"
    exit 1
fi
echo ""

# Test 9: Verify Signature is Saved
echo "[Test 9] Verify Signature is Saved"
echo "GET $MB_URL/requests/$REQUEST_ID"

VERIFY_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" \
    "$MB_URL/requests/$REQUEST_ID")

HTTP_CODE=$(echo "$VERIFY_RESPONSE" | grep "HTTP_CODE:" | cut -d: -f2)
BODY=$(echo "$VERIFY_RESPONSE" | grep -v "HTTP_CODE:")

if [ "$HTTP_CODE" = "200" ]; then
    echo "✓ Request retrieved (HTTP $HTTP_CODE)"

    if echo "$BODY" | grep -q "3045022100abcdef1234567890"; then
        echo "  ✓ Signature found in response"
    else
        echo "  ✗ Signature not found in response"
        exit 1
    fi

    if echo "$BODY" | grep -q '"status":"completed"'; then
        echo "  ✓ Status is 'completed'"
    else
        echo "  ✗ Status is not 'completed'"
        exit 1
    fi
else
    echo "✗ Verification failed (HTTP $HTTP_CODE)"
    exit 1
fi
echo ""

echo "========================================="
echo "All MessageBoard tests passed! ✓"
echo "========================================="
