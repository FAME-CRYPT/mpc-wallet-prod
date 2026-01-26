#!/bin/bash
# Test DKG with coordinator + participant pattern

echo "=========================================="
echo "Testing DKG Ceremony with Join Pattern"
echo "=========================================="
echo ""

# Step 1: Coordinator (node-1) initiates DKG
echo "[1/3] Node-1 (coordinator) initiating DKG ceremony..."
RESPONSE=$(curl -s -X POST http://localhost:8081/api/v1/dkg/initiate \
  -H "Content-Type: application/json" \
  -d '{"protocol":"cggmp24","threshold":4,"total_nodes":5}' \
  --max-time 5)

echo "Response: $RESPONSE"

# Extract session ID
SESSION_ID=$(echo $RESPONSE | jq -r '.session_id')

if [ "$SESSION_ID" = "null" ] || [ -z "$SESSION_ID" ]; then
    echo "ERROR: Failed to get session ID from coordinator"
    echo "Full response: $RESPONSE"
    exit 1
fi

echo "✓ Session ID: $SESSION_ID"
echo ""

# Small delay to ensure ceremony is written to PostgreSQL
sleep 1

# Step 2: Participant nodes (2-5) join the ceremony
echo "[2/3] Nodes 2-5 (participants) joining DKG ceremony..."
for port in 8082 8083 8084 8085; do
    NODE_NUM=$((port - 8080))
    echo "  → Node-$NODE_NUM joining..."
    curl -s -X POST http://localhost:$port/api/v1/dkg/join/$SESSION_ID \
      --max-time 120 > /tmp/dkg_node_${NODE_NUM}.json &
done

echo "  Waiting for all nodes to complete DKG protocol..."
wait

echo "✓ All nodes completed their join requests"
echo ""

# Step 3: Check results
echo "[3/3] Checking DKG results..."
echo ""

for port in 8081 8082 8083 8084 8085; do
    NODE_NUM=$((port - 8080))
    echo "--- Node-$NODE_NUM Result ---"

    if [ $NODE_NUM -eq 1 ]; then
        # Node-1 response is in RESPONSE variable
        echo "$RESPONSE" | jq '.'
    else
        # Other nodes saved to files
        cat /tmp/dkg_node_${NODE_NUM}.json | jq '.'
    fi
    echo ""
done

# Extract public key from node-1 response
PUBLIC_KEY=$(echo $RESPONSE | jq -r '.public_key')
ADDRESS=$(echo $RESPONSE | jq -r '.address')

echo "=========================================="
echo "DKG Ceremony Completed!"
echo "=========================================="
echo "Session ID: $SESSION_ID"
echo "Public Key: $PUBLIC_KEY"
echo "Address:    $ADDRESS"
echo "=========================================="

# Verify all nodes have the same public key
echo ""
echo "Verifying consistency across nodes..."
CONSISTENT=true
for i in 2 3 4 5; do
    NODE_KEY=$(cat /tmp/dkg_node_${i}.json | jq -r '.public_key')
    if [ "$NODE_KEY" != "$PUBLIC_KEY" ]; then
        echo "✗ Node-$i has different public key: $NODE_KEY"
        CONSISTENT=false
    else
        echo "✓ Node-$i matches: $NODE_KEY"
    fi
done

if [ "$CONSISTENT" = true ]; then
    echo ""
    echo "✓✓✓ SUCCESS: All nodes generated the same public key!"
else
    echo ""
    echo "✗✗✗ FAILURE: Public key mismatch detected"
    exit 1
fi
