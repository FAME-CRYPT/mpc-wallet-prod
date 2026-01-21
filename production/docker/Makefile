# Makefile for MPC Wallet Docker Deployment
# Simplifies common Docker operations

.PHONY: help build up down logs restart clean health status certs test prod dev

# Default target
.DEFAULT_GOAL := help

# Variables
COMPOSE_FILE := docker-compose.yml
COMPOSE_DEV := docker-compose.dev.yml
COMPOSE_PROD := docker-compose.prod.yml
PROJECT_NAME := mpc-wallet

help: ## Show this help message
	@echo "MPC Wallet Docker Deployment Commands:"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}'
	@echo ""

build: ## Build Docker images
	docker-compose -f $(COMPOSE_FILE) build

up: ## Start all services
	docker-compose -f $(COMPOSE_FILE) up -d

down: ## Stop all services
	docker-compose -f $(COMPOSE_FILE) down

logs: ## Show logs (use SERVICE=node-1 for specific service)
ifdef SERVICE
	docker-compose -f $(COMPOSE_FILE) logs -f $(SERVICE)
else
	docker-compose -f $(COMPOSE_FILE) logs -f
endif

restart: ## Restart all services (or specific SERVICE=node-1)
ifdef SERVICE
	docker-compose -f $(COMPOSE_FILE) restart $(SERVICE)
else
	docker-compose -f $(COMPOSE_FILE) restart
endif

clean: ## Stop and remove all containers, networks, and volumes
	docker-compose -f $(COMPOSE_FILE) down -v
	docker system prune -f

health: ## Check health of all services
	@echo "Checking service health..."
	@docker-compose -f $(COMPOSE_FILE) ps
	@echo ""
	@echo "etcd cluster health:"
	@docker exec mpc-etcd-1 etcdctl endpoint health --cluster || true
	@echo ""
	@echo "PostgreSQL health:"
	@docker exec mpc-postgres pg_isready -U mpc || true
	@echo ""
	@echo "Node health checks:"
	@for i in 1 2 3 4 5; do \
		echo -n "Node $$i: "; \
		curl -s http://localhost:808$$i/health || echo "FAILED"; \
	done

status: ## Show status of all services
	docker-compose -f $(COMPOSE_FILE) ps

certs: ## Generate TLS certificates
	cd ../scripts && ./generate-certs.sh

test: ## Run deployment tests
	@echo "Testing deployment..."
	@echo "1. Checking etcd cluster..."
	@docker exec mpc-etcd-1 etcdctl endpoint health --cluster
	@echo "2. Checking PostgreSQL..."
	@docker exec mpc-postgres psql -U mpc -d mpc_wallet -c "SELECT 1;"
	@echo "3. Checking node APIs..."
	@for i in 1 2 3 4 5; do \
		echo "Testing node $$i..."; \
		curl -f http://localhost:808$$i/health; \
	done
	@echo "All tests passed!"

prod: ## Start with production configuration
	docker-compose -f $(COMPOSE_FILE) -f $(COMPOSE_PROD) up -d

dev: ## Start with development configuration
	docker-compose -f $(COMPOSE_FILE) -f $(COMPOSE_DEV) up -d

pull: ## Pull latest images
	docker-compose -f $(COMPOSE_FILE) pull

rebuild: ## Rebuild and restart services
	docker-compose -f $(COMPOSE_FILE) build --no-cache
	docker-compose -f $(COMPOSE_FILE) up -d

backup: ## Backup PostgreSQL and etcd data
	@echo "Backing up PostgreSQL..."
	@mkdir -p backups
	@docker exec mpc-postgres pg_dump -U mpc -d mpc_wallet > backups/postgres-$$(date +%Y%m%d-%H%M%S).sql
	@echo "Backing up etcd..."
	@docker exec mpc-etcd-1 etcdctl snapshot save /tmp/etcd-backup.db
	@docker cp mpc-etcd-1:/tmp/etcd-backup.db backups/etcd-$$(date +%Y%m%d-%H%M%S).db
	@echo "Backup complete!"

restore-postgres: ## Restore PostgreSQL from backup (BACKUP=path/to/backup.sql)
ifndef BACKUP
	@echo "Error: Please specify BACKUP=path/to/backup.sql"
	@exit 1
endif
	@echo "Restoring PostgreSQL from $(BACKUP)..."
	@cat $(BACKUP) | docker exec -i mpc-postgres psql -U mpc -d mpc_wallet
	@echo "Restore complete!"

shell-node: ## Open shell in node container (NODE=1)
	docker exec -it mpc-node-$(NODE) /bin/sh

shell-postgres: ## Open PostgreSQL shell
	docker exec -it mpc-postgres psql -U mpc -d mpc_wallet

shell-etcd: ## Open etcd shell (NODE=1)
	docker exec -it mpc-etcd-$(NODE) /bin/sh

stats: ## Show resource usage statistics
	docker stats --no-stream

prune: ## Remove unused Docker resources
	docker system prune -a -f --volumes

upgrade: ## Upgrade to latest version
	@echo "Pulling latest images..."
	docker-compose -f $(COMPOSE_FILE) pull
	@echo "Building new images..."
	docker-compose -f $(COMPOSE_FILE) build --no-cache
	@echo "Performing rolling upgrade..."
	@for i in 1 2 3 4 5; do \
		echo "Upgrading node $$i..."; \
		docker-compose -f $(COMPOSE_FILE) stop node-$$i; \
		docker-compose -f $(COMPOSE_FILE) up -d node-$$i; \
		sleep 30; \
	done
	@echo "Upgrade complete!"

validate: ## Validate docker-compose configuration
	docker-compose -f $(COMPOSE_FILE) config

init: ## Initialize new deployment (certs + env + build + up)
	@echo "Initializing MPC Wallet deployment..."
	@if [ ! -f .env ]; then \
		echo "Creating .env from template..."; \
		cp .env.example .env; \
		echo "Please edit .env and set POSTGRES_PASSWORD!"; \
		exit 1; \
	fi
	@if [ ! -d ../certs ]; then \
		echo "Generating certificates..."; \
		make certs; \
	fi
	@echo "Building images..."
	@make build
	@echo "Starting services..."
	@make up
	@echo "Waiting for services to be healthy..."
	@sleep 30
	@make health
	@echo "Initialization complete!"
