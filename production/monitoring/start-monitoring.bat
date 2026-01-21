@echo off
REM MPC Wallet Monitoring Stack Startup Script for Windows

echo [INFO] Starting MPC Wallet Monitoring Stack...
echo ===============================================

REM Check if .env file exists
if not exist .env (
    echo [ERROR] .env file not found!
    echo [INFO] Creating .env from .env.example...
    copy .env.example .env
    echo [WARNING] Please edit .env file with your actual credentials before continuing!
    pause
    exit /b 1
)

REM Check if Docker is running
docker info >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Docker is not running! Please start Docker Desktop.
    pause
    exit /b 1
)

REM Pull latest images
echo [INFO] Pulling latest Docker images...
docker-compose -f docker-compose.monitoring.yml pull

REM Start monitoring stack
echo [INFO] Starting monitoring services...
docker-compose -f docker-compose.monitoring.yml up -d

REM Wait for services
echo [INFO] Waiting for services to start...
timeout /t 10 /nobreak >nul

REM Check service status
echo [INFO] Checking service status...
docker-compose -f docker-compose.monitoring.yml ps

echo.
echo ===============================================
echo [INFO] Monitoring stack started successfully!
echo.
echo Access Points:
echo   - Grafana:    http://localhost:3000
echo   - Prometheus: http://localhost:9090
echo   - cAdvisor:   http://localhost:8081
echo.
echo Default Grafana Credentials:
echo   - Username: admin
echo   - Password: (set in .env file)
echo.
echo Available Dashboards:
echo   1. MPC Cluster Overview
echo   2. Byzantine Consensus Monitoring
echo   3. Signature Performance
echo   4. Infrastructure Monitoring
echo   5. Network Monitoring
echo.
echo To view logs:
echo   docker-compose -f docker-compose.monitoring.yml logs -f
echo.
echo To stop monitoring:
echo   docker-compose -f docker-compose.monitoring.yml down
echo ===============================================
echo.
echo [INFO] Setup complete! Visit http://localhost:3000 to access Grafana.
echo.
pause
