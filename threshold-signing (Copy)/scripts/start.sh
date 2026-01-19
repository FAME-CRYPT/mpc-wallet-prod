#!/bin/bash
# Start all threshold signing services

echo "Building and starting threshold signing system..."
echo ""

# Build images and start containers
podman-compose build
podman-compose up -d

echo ""
echo "Services starting..."
echo ""
echo "Waiting for services to be ready..."
sleep 5

# Check health
echo ""
echo "Checking service health:"

if curl -s http://localhost:8000/health > /dev/null 2>&1; then
    echo "✓ API Gateway is running (http://localhost:8000)"
else
    echo "✗ API Gateway is not responding"
fi

if curl -s http://localhost:8080/health > /dev/null 2>&1; then
    echo "✓ MessageBoard is running (http://localhost:8080)"
else
    echo "✗ MessageBoard is not responding"
fi

echo ""
echo "All services started!"
echo ""
echo "Next steps:"
echo "  1. Wait 60 seconds for nodes to complete keygen"
echo "  2. Run end-to-end test: ./scripts/test-e2e.sh"
echo "  3. View logs: ./scripts/logs.sh node-1"
echo "  4. Stop services: ./scripts/stop.sh"
echo ""
echo "Services are running in background. Use 'podman-compose logs -f' to watch logs."
