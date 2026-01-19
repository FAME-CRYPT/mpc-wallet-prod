#!/bin/bash
set -e

echo "====================================="
echo "Threshold Voting System Test Scenario"
echo "====================================="
echo ""

echo "Starting infrastructure..."
docker-compose up -d etcd1 etcd2 etcd3 postgres
echo "Waiting for services to be ready..."
sleep 10

echo ""
echo "Starting voting nodes..."
docker-compose up -d node1 node2 node3 node4 node5
echo "Waiting for nodes to initialize..."
sleep 15

echo ""
echo "====================================="
echo "Test Scenario 1: Successful Consensus"
echo "====================================="
echo "Transaction: tx_001"
echo "Expected: 4 nodes vote '42', threshold reached"
echo ""

echo "Node 1 votes 42..."
docker-compose exec -T node1 /app/threshold-voting-system vote --tx-id tx_001 --value 42 || true

echo "Node 2 votes 42..."
docker-compose exec -T node2 /app/threshold-voting-system vote --tx-id tx_001 --value 42 || true

echo "Node 3 votes 42..."
docker-compose exec -T node3 /app/threshold-voting-system vote --tx-id tx_001 --value 42 || true

echo "Node 4 votes 42..."
docker-compose exec -T node4 /app/threshold-voting-system vote --tx-id tx_001 --value 42 || true

echo ""
echo "Checking logs for consensus..."
docker-compose logs --tail=20 node1 | grep -i "threshold\|consensus" || true

echo ""
echo "====================================="
echo "Test Scenario 2: Byzantine Detection"
echo "====================================="
echo "Transaction: tx_002"
echo "Expected: Node 5 votes differently, detected as Byzantine"
echo ""

echo "Node 1 votes 100..."
docker-compose exec -T node1 /app/threshold-voting-system vote --tx-id tx_002 --value 100 || true

echo "Node 2 votes 100..."
docker-compose exec -T node2 /app/threshold-voting-system vote --tx-id tx_002 --value 100 || true

echo "Node 3 votes 100..."
docker-compose exec -T node3 /app/threshold-voting-system vote --tx-id tx_002 --value 100 || true

echo "Node 4 votes 100..."
docker-compose exec -T node4 /app/threshold-voting-system vote --tx-id tx_002 --value 100 || true

echo "Node 5 votes 999 (Byzantine)..."
docker-compose exec -T node5 /app/threshold-voting-system vote --tx-id tx_002 --value 999 || true

echo ""
echo "Checking logs for Byzantine detection..."
docker-compose logs --tail=20 | grep -i "byzantine\|minority" || true

echo ""
echo "====================================="
echo "Test Scenario 3: Double Voting"
echo "====================================="
echo "Transaction: tx_003"
echo "Expected: Node 1 tries to vote twice with different values"
echo ""

echo "Node 1 votes 50..."
docker-compose exec -T node1 /app/threshold-voting-system vote --tx-id tx_003 --value 50 || true

echo "Node 1 votes 99 (Double voting attempt)..."
docker-compose exec -T node1 /app/threshold-voting-system vote --tx-id tx_003 --value 99 || true

echo ""
echo "Checking logs for double voting detection..."
docker-compose logs --tail=20 | grep -i "double" || true

echo ""
echo "====================================="
echo "Test Complete!"
echo "====================================="
echo ""
echo "To view full logs:"
echo "  docker-compose logs -f"
echo ""
echo "To stop all services:"
echo "  docker-compose down"
echo ""
echo "To clean up everything (including data):"
echo "  docker-compose down -v"
echo ""
