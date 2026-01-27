#!/bin/bash
# Parallel DKG test - all nodes start simultaneously

echo "=========================================="
echo "Testing DKG Ceremony (Parallel Start)"
echo "=========================================="

# First, get session ID that will be created
# We'll initiate and join all in parallel, but need to get session ID first

# Step 1: Initiate DKG on node-1 in background
echo "[1/2] Starting DKG ceremony..."
curl -s -X POST http://localhost:8081/api/v1/dkg/initiate \
  -H "Content-Type: application/json" \
  -d '{"protocol":"cggmp24","threshold":4,"total_nodes":5}' \
  --max-time 120 > /tmp/node1.json &
PID1=$!

# Give it a moment to create the session in database
sleep 0.5

# Step 2: Query database for the session ID
SESSION_ID=$(docker exec mpc-postgres psql -U mpc -d mpc_wallet -t -c "SELECT session_id FROM dkg_ceremonies WHERE status='running' ORDER BY started_at DESC LIMIT 1;" | tr -d ' \n')

if [ -z "$SESSION_ID" ]; then
    echo "ERROR: No running ceremony found in database"
    wait $PID1
    cat /tmp/node1.json
    exit 1
fi

echo "Found session ID: $SESSION_ID"
echo ""

# Step 3: Join from nodes 2-5 immediately
echo "[2/2] Nodes 2-5 joining..."
curl -s -X POST http://localhost:8082/api/v1/dkg/join/$SESSION_ID --max-time 120 > /tmp/node2.json &
curl -s -X POST http://localhost:8083/api/v1/dkg/join/$SESSION_ID --max-time 120 > /tmp/node3.json &
curl -s -X POST http://localhost:8084/api/v1/dkg/join/$SESSION_ID --max-time 120 > /tmp/node4.json &
curl -s -X POST http://localhost:8085/api/v1/dkg/join/$SESSION_ID --max-time 120 > /tmp/node5.json &

echo "Waiting for all nodes to complete DKG protocol..."
wait

echo "✓ All nodes completed"
echo ""

# Check results
echo "=========================================="
echo "Results:"
echo "=========================================="
echo ""
echo "--- Node-1 ---"
cat /tmp/node1.json
echo ""
echo ""
echo "--- Node-2 ---"
cat /tmp/node2.json
echo ""
echo ""
echo "--- Node-3 ---"
cat /tmp/node3.json
echo ""
echo ""
echo "--- Node-4 ---"
cat /tmp/node4.json
echo ""
echo ""
echo "--- Node-5 ---"
cat /tmp/node5.json
echo ""
echo ""

# Extract and compare public keys from all nodes
PK1=$(cat /tmp/node1.json | grep -o '"public_key":"[^"]*"' | sed 's/"public_key":"\([^"]*\)"/\1/')
PK2=$(cat /tmp/node2.json | grep -o '"public_key":"[^"]*"' | sed 's/"public_key":"\([^"]*\)"/\1/')
PK3=$(cat /tmp/node3.json | grep -o '"public_key":"[^"]*"' | sed 's/"public_key":"\([^"]*\)"/\1/')
PK4=$(cat /tmp/node4.json | grep -o '"public_key":"[^"]*"' | sed 's/"public_key":"\([^"]*\)"/\1/')
PK5=$(cat /tmp/node5.json | grep -o '"public_key":"[^"]*"' | sed 's/"public_key":"\([^"]*\)"/\1/')

echo "=========================================="
echo "Verification:"
echo "=========================================="
echo "Public Key (node-1): $PK1"
echo "Public Key (node-2): $PK2"
echo "Public Key (node-3): $PK3"
echo "Public Key (node-4): $PK4"
echo "Public Key (node-5): $PK5"

# Check all keys match
if [ -z "$PK1" ]; then
    echo ""
    echo "✗✗✗ FAILURE: No public key generated"
    exit 1
elif [ "$PK1" = "$PK2" ] && [ "$PK1" = "$PK3" ] && [ "$PK1" = "$PK4" ] && [ "$PK1" = "$PK5" ]; then
    echo ""
    echo "✓✓✓ SUCCESS: DKG completed successfully!"
    echo "✓✓✓ All 5 nodes have matching public keys!"
    exit 0
else
    echo ""
    echo "✗✗✗ FAILURE: Public key mismatch"
    exit 1
fi
