#!/usr/bin/env pwsh
# Full rebuild script - cleans everything and rebuilds from scratch

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Docker Full Rebuild - No Cache" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Stop and remove all containers
Write-Host "[1/6] Stopping containers..." -ForegroundColor Yellow
docker-compose down -v 2>&1 | Out-Null

# Remove all images related to this project
Write-Host "[2/6] Removing old images..." -ForegroundColor Yellow
docker images | Select-String "mtls-sharedmem" | ForEach-Object {
    $imageId = ($_ -split '\s+')[2]
    docker rmi -f $imageId 2>&1 | Out-Null
}

# Clean Docker build cache
Write-Host "[3/6] Cleaning Docker build cache..." -ForegroundColor Yellow
docker builder prune -af 2>&1 | Out-Null

# Build with no cache
Write-Host "[4/6] Building images (no cache)..." -ForegroundColor Yellow
docker-compose build --no-cache --progress=plain

if ($LASTEXITCODE -ne 0) {
    Write-Host "" -ForegroundColor Red
    Write-Host "Build FAILED!" -ForegroundColor Red
    exit 1
}

# Start services
Write-Host "[5/6] Starting services..." -ForegroundColor Yellow
docker-compose up -d

# Wait a bit for services to start
Start-Sleep -Seconds 5

# Check status
Write-Host "[6/6] Checking status..." -ForegroundColor Yellow
Write-Host ""
docker-compose ps

Write-Host ""
Write-Host "========================================" -ForegroundColor Green
Write-Host "  Build Complete!" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Green
Write-Host ""
Write-Host "View logs with:" -ForegroundColor Cyan
Write-Host "  docker logs threshold-node1 --follow" -ForegroundColor White
Write-Host ""
Write-Host "Check all services:" -ForegroundColor Cyan
Write-Host "  docker-compose ps" -ForegroundColor White
Write-Host ""
