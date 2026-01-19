#!/bin/bash
# Interactive demo showing the complete threshold signing flow
# This demonstrates how all components work together

set -e

API_URL="${API_URL:-http://localhost:8000}"
MB_URL="${MB_URL:-http://localhost:8080}"

# Colors for pretty output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo ""
echo "========================================="
echo "  Threshold Signing System Demo"
echo "========================================="
echo ""
echo "This demo shows the complete flow:"
echo "  1. Client submits signing request to API Gateway"
echo "  2. API Gateway forwards to MessageBoard"
echo "  3. Nodes exchange messages via MessageBoard"
echo "  4. Client retrieves signature status"
echo ""
echo "Components:"
echo "  API Gateway:   $API_URL"
echo "  MessageBoard:  $MB_URL"
echo "  Nodes:         4 nodes (3-of-4 threshold)"
echo ""
read -p "Press Enter to start the demo..."
echo ""

# Step 1: Submit signing request
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}Step 1: Submit Signing Request${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo "Client sends message to API Gateway:"
echo ""
MESSAGE="Hello from threshold signing demo!"
PAYLOAD="{\"message\":\"$MESSAGE\"}"
echo "  Message: $MESSAGE"
echo "  Endpoint: POST $API_URL/sign"
echo ""
read -p "Press Enter to send request..."

RESPONSE=$(curl -s -X POST "$API_URL/sign" \
    -H "Content-Type: application/json" \
    -d "$PAYLOAD")

echo ""
echo -e "${GREEN}Response from API Gateway:${NC}"
echo "$RESPONSE" | jq '.'

REQUEST_ID=$(echo "$RESPONSE" | jq -r '.request_id')
STATUS=$(echo "$RESPONSE" | jq -r '.status')

echo ""
echo -e "${GREEN}✓ Request created${NC}"
echo "  Request ID: $REQUEST_ID"
echo "  Status: $STATUS"
echo ""
read -p "Press Enter to continue..."
echo ""

# Step 2: Show request on MessageBoard
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}Step 2: Check MessageBoard${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo "The API Gateway forwarded the request to the MessageBoard."
echo "Let's check the request details on the MessageBoard:"
echo ""
echo "  Endpoint: GET $MB_URL/requests/$REQUEST_ID"
echo ""
read -p "Press Enter to query MessageBoard..."

MB_RESPONSE=$(curl -s "$MB_URL/requests/$REQUEST_ID")

echo ""
echo -e "${GREEN}Response from MessageBoard:${NC}"
echo "$MB_RESPONSE" | jq '.'

echo ""
echo -e "${GREEN}✓ Request stored on MessageBoard${NC}"
echo "  Nodes can now see this request and begin signing"
echo ""
read -p "Press Enter to continue..."
echo ""

# Step 3: Simulate node communication
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}Step 3: Node Communication${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo "In a real system, nodes would now:"
echo "  1. Poll MessageBoard for new requests"
echo "  2. Execute threshold signing protocol (CGGMP24)"
echo "  3. Exchange protocol messages via MessageBoard"
echo "  4. One node submits the final signature"
echo ""
echo "Let's simulate some node messages being exchanged:"
echo ""
read -p "Press Enter to simulate node messages..."
echo ""

# Simulate node-1 sending a message to node-2
echo "Node-1 → Node-2 (Round 1):"
MSG1='{
  "request_id":"'"$REQUEST_ID"'",
  "from_node":"node-1",
  "to_node":"node-2",
  "round":1,
  "payload":"{\"commitment\":\"0x1234...\"}"
}'
curl -s -X POST "$MB_URL/messages" \
    -H "Content-Type: application/json" \
    -d "$MSG1" > /dev/null
echo "  ✓ Message posted"
echo ""

# Simulate node-2 broadcasting
echo "Node-2 → All Nodes (Broadcast):"
MSG2='{
  "request_id":"'"$REQUEST_ID"'",
  "from_node":"node-2",
  "to_node":"*",
  "round":1,
  "payload":"{\"share\":\"0xabcd...\"}"
}'
curl -s -X POST "$MB_URL/messages" \
    -H "Content-Type: application/json" \
    -d "$MSG2" > /dev/null
echo "  ✓ Broadcast message posted"
echo ""

# Show messages for a node
echo "Messages available for node-2:"
MESSAGES=$(curl -s "$MB_URL/messages?request_id=$REQUEST_ID&to_node=node-2")
echo "$MESSAGES" | jq '.messages | length' | xargs echo "  Total messages:"
echo ""

read -p "Press Enter to continue..."
echo ""

# Step 4: Simulate signature completion
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}Step 4: Signature Completion${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo "After the protocol completes, one node submits the signature."
echo ""
echo "Let's simulate a completed signature:"
echo ""
read -p "Press Enter to submit signature..."

# Simulate signature submission (this would come from a node)
SIGNATURE="3045022100a1b2c3d4e5f6789012345678901234567890123456789012345678901234567802204f5e6d7c8b9a012345678901234567890123456789012345678901234567890"
SIG_UPDATE="{\"signature\":\"$SIGNATURE\"}"

curl -s -X PUT "$MB_URL/requests/$REQUEST_ID" \
    -H "Content-Type: application/json" \
    -d "$SIG_UPDATE" > /dev/null

echo ""
echo -e "${GREEN}✓ Signature submitted to MessageBoard${NC}"
echo "  Signature: $SIGNATURE"
echo ""
read -p "Press Enter to continue..."
echo ""

# Step 5: Client retrieves signature
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}Step 5: Retrieve Signature${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo "Client checks the status via API Gateway:"
echo ""
echo "  Endpoint: GET $API_URL/status/$REQUEST_ID"
echo ""
read -p "Press Enter to check status..."

FINAL_RESPONSE=$(curl -s "$API_URL/status/$REQUEST_ID")

echo ""
echo -e "${GREEN}Response from API Gateway:${NC}"
echo "$FINAL_RESPONSE" | jq '.'

FINAL_STATUS=$(echo "$FINAL_RESPONSE" | jq -r '.status')
FINAL_SIG=$(echo "$FINAL_RESPONSE" | jq -r '.signature')

echo ""
if [ "$FINAL_STATUS" = "completed" ]; then
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${GREEN}✓ SUCCESS!${NC}"
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
    echo "The threshold signature has been created!"
    echo ""
    echo "Summary:"
    echo "  Original Message: $MESSAGE"
    echo "  Request ID:       $REQUEST_ID"
    echo "  Status:           $FINAL_STATUS"
    echo "  Signature:        ${FINAL_SIG:0:50}..."
else
    echo -e "${YELLOW}Status: $FINAL_STATUS${NC}"
fi

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Demo complete!"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
