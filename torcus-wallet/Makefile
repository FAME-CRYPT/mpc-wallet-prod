.PHONY: build docker-build run run-fg stop logs logs-coordinator logs-nodes clean \
	test verify full-test start-regtest start-testnet create-btc create-taproot \
	info curl-health curl-info curl-create-wallet help

# Build all Rust binaries locally
build:
	cargo build --release

# Build Docker images
docker-build:
	docker compose build

# Start all services in Docker
run:
	docker compose up -d --build
	@echo "Services starting... wait a few seconds then run 'make verify'"

# Start services in foreground (useful for debugging)
run-fg:
	docker compose up --build

# Stop all services
stop:
	docker compose down

# View logs
logs:
	docker compose logs -f

# View logs for specific service
logs-coordinator:
	docker compose logs -f coordinator

logs-nodes:
	docker compose logs -f mpc-node-1 mpc-node-2 mpc-node-3 mpc-node-4

# Clean up Docker resources
clean:
	docker compose down -v --rmi local
	cargo clean

# Run unit tests (verifies DKG protocol locally)
test:
	cargo test --package node -- --nocapture

# Run integration verification (requires running services)
# Works on Linux, macOS, Windows (Git Bash, MSYS2, WSL)
verify:
	@bash scripts/verify.sh

# Full test: build, run, verify, stop
full-test: docker-build
	docker compose up -d
	@echo "Waiting 15 seconds for services to start..."
	@sleep 15
	@bash scripts/verify.sh || (docker compose down && exit 1)
	@docker compose down
	@echo ""
	@echo "========================================"
	@echo "  Full test passed!"
	@echo "========================================"

# Start services with helper scripts
start-regtest:
	@bash scripts/start-regtest.sh

start-testnet:
	@bash scripts/start-testnet.sh

# Create Bitcoin wallet (use cggmp24-create for SegWit or taproot-create for Taproot)
create-btc:
	cargo run --bin mpc-wallet -- cggmp24-create --name "Bitcoin Wallet"

# Create Taproot wallet
create-taproot:
	cargo run --bin mpc-wallet -- taproot-create --name "Taproot Wallet"

# Get system info
info:
	cargo run --bin mpc-wallet -- info

# Quick test with curl (no jq required)
curl-health:
	@curl -s http://localhost:3000/health

curl-info:
	@curl -s http://localhost:3000/info

curl-create-wallet:
	@curl -s -X POST http://localhost:3000/wallet \
		-H "Content-Type: application/json" \
		-d '{"name": "Test Wallet", "wallet_type": "Bitcoin"}'

# Help
help:
	@echo "MPC Wallet - Available Commands"
	@echo "================================"
	@echo ""
	@echo "Quick Start:"
	@echo "  make full-test     - Build, run, verify, and stop (automated)"
	@echo ""
	@echo "Testing:"
	@echo "  make test          - Run unit tests (local, no Docker)"
	@echo "  make verify        - Run integration tests (requires running services)"
	@echo ""
	@echo "Docker:"
	@echo "  make run           - Start all services"
	@echo "  make run-fg        - Start in foreground (see logs)"
	@echo "  make start-regtest - Start services via regtest script"
	@echo "  make start-testnet - Start services via testnet script"
	@echo "  make stop          - Stop all services"
	@echo "  make logs          - View logs"
	@echo "  make clean         - Clean up everything"
	@echo ""
	@echo "Wallets (requires running services):"
	@echo "  make create-btc    - Create a SegWit Bitcoin wallet (CGGMP24)"
	@echo "  make create-taproot - Create a Taproot wallet (FROST)"
	@echo "  make info          - Get system info"
	@echo ""
	@echo "Debug:"
	@echo "  make curl-health   - Check coordinator health"
	@echo "  make curl-info     - Get system info"
