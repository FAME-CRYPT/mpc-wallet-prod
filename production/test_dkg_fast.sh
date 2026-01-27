#!/bin/bash
# Fast DKG test - nodes join immediately

echo "=========================================="
echo "Testing DKG Ceremony (Fast Join)"
echo "=========================================="

# Step 1: Initiate DKG on node-1 and extract session ID
echo "[1/3] Node-1 initiating DKG..."
RESPONSE=$(curl -s -X POST http://localhost:8081/api/v1/dkg/initiate \
  -H "Content-Type: application/json" \
  -d '{"protocol":"cggmp24","threshold":4,"total_nodes":5}' \
  --max-time 5)

echo "Response: $RESPONSE"

# Extract session ID using grep and sed (no jq needed)
SESSION_ID=$(echo "$RESPONSE" | grep -o '"session_id":"[^"]*"' | sed 's/"session_id":"\([^"]*\)"/\1/')

if [ -z "$SESSION_ID" ]; then
    echo "ERROR: Failed to get session ID"
    exit 1
fi

echo "Session ID: $SESSION_ID"
echo ""

# Step 2: Immediately join from nodes 2-5 (no delay)
echo "[2/3] Nodes 2-5 joining immediately..."
curl -s -X POST http://localhost:8082/api/v1/dkg/join/$SESSION_ID --max-time 120 > /tmp/node2.json &
curl -s -X POST http://localhost:8083/api/v1/dkg/join/$SESSION_ID --max-time 120 > /tmp/node3.json &
curl -s -X POST http://localhost:8084/api/v1/dkg/join/$SESSION_ID --max-time 120 > /tmp/node4.json &
curl -s -X POST http://localhost:8085/api/v1/dkg/join/$SESSION_ID --max-time 120 > /tmp/node5.json &

echo "Waiting for all nodes to complete..."
wait

echo "✓ All nodes completed"
echo ""

# Step 3: Check results
echo "[3/3] Checking results..."
echo ""
echo "--- Node-1 ---"
echo "$RESPONSE"
echo ""
echo "--- Node-2 ---"
cat /tmp/node2.json
echo ""
echo "--- Node-3 ---"
cat /tmp/node3.json
echo ""
echo "--- Node-4 ---"
cat /tmp/node4.json
echo ""
echo "--- Node-5 ---"
cat /tmp/node5.json
echo ""

# Extract public keys
PK1=$(echo "$RESPONSE" | grep -o '"public_key":"[^"]*"' | sed 's/"public_key":"\([^"]*\)"/\1/')
PK2=$(cat /tmp/node2.json | grep -o '"public_key":"[^"]*"' | sed 's/"public_key":"\([^"]*\)"/\1/')

echo "=========================================="
echo "Public Key from node-1: $PK1"
echo "Public Key from node-2: $PK2"
echo "=========================================="

if [ "$PK1" = "$PK2" ] && [ -n "$PK1" ]; then
    echo "✓✓✓ SUCCESS: Public keys match!"
else
    echo "✗✗✗ FAILURE: Public keys don't match or are empty"
    exit 1
fi
