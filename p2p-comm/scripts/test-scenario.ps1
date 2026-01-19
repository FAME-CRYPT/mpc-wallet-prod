# Windows PowerShell test scenario script
$ErrorActionPreference = "Stop"

Write-Host "=====================================" -ForegroundColor Cyan
Write-Host "Threshold Voting System Test Scenario" -ForegroundColor Cyan
Write-Host "=====================================" -ForegroundColor Cyan
Write-Host ""

Write-Host "Starting infrastructure..." -ForegroundColor Yellow
docker-compose up -d etcd1 etcd2 etcd3 postgres
Write-Host "Waiting for services to be ready..." -ForegroundColor Yellow
Start-Sleep -Seconds 10

Write-Host ""
Write-Host "Starting voting nodes..." -ForegroundColor Yellow
docker-compose up -d node1 node2 node3 node4 node5
Write-Host "Waiting for nodes to initialize..." -ForegroundColor Yellow
Start-Sleep -Seconds 15

Write-Host ""
Write-Host "=====================================" -ForegroundColor Green
Write-Host "Test Scenario 1: Successful Consensus" -ForegroundColor Green
Write-Host "=====================================" -ForegroundColor Green
Write-Host "Transaction: tx_001"
Write-Host "Expected: 4 nodes vote '42', threshold reached"
Write-Host ""

Write-Host "Node 1 votes 42..." -ForegroundColor White
docker-compose exec -T node1 /app/threshold-voting-system vote --tx-id tx_001 --value 42

Write-Host "Node 2 votes 42..." -ForegroundColor White
docker-compose exec -T node2 /app/threshold-voting-system vote --tx-id tx_001 --value 42

Write-Host "Node 3 votes 42..." -ForegroundColor White
docker-compose exec -T node3 /app/threshold-voting-system vote --tx-id tx_001 --value 42

Write-Host "Node 4 votes 42..." -ForegroundColor White
docker-compose exec -T node4 /app/threshold-voting-system vote --tx-id tx_001 --value 42

Write-Host ""
Write-Host "Checking logs for consensus..." -ForegroundColor Yellow
docker-compose logs --tail=20 node1 | Select-String -Pattern "threshold|consensus" -CaseSensitive:$false

Write-Host ""
Write-Host "=====================================" -ForegroundColor Magenta
Write-Host "Test Scenario 2: Byzantine Detection" -ForegroundColor Magenta
Write-Host "=====================================" -ForegroundColor Magenta
Write-Host "Transaction: tx_002"
Write-Host "Expected: Node 5 votes differently, detected as Byzantine"
Write-Host ""

Write-Host "Node 1 votes 100..." -ForegroundColor White
docker-compose exec -T node1 /app/threshold-voting-system vote --tx-id tx_002 --value 100

Write-Host "Node 2 votes 100..." -ForegroundColor White
docker-compose exec -T node2 /app/threshold-voting-system vote --tx-id tx_002 --value 100

Write-Host "Node 3 votes 100..." -ForegroundColor White
docker-compose exec -T node3 /app/threshold-voting-system vote --tx-id tx_002 --value 100

Write-Host "Node 4 votes 100..." -ForegroundColor White
docker-compose exec -T node4 /app/threshold-voting-system vote --tx-id tx_002 --value 100

Write-Host "Node 5 votes 999 (Byzantine)..." -ForegroundColor Red
docker-compose exec -T node5 /app/threshold-voting-system vote --tx-id tx_002 --value 999

Write-Host ""
Write-Host "Checking logs for Byzantine detection..." -ForegroundColor Yellow
docker-compose logs --tail=20 | Select-String -Pattern "byzantine|minority" -CaseSensitive:$false

Write-Host ""
Write-Host "=====================================" -ForegroundColor Red
Write-Host "Test Scenario 3: Double Voting" -ForegroundColor Red
Write-Host "=====================================" -ForegroundColor Red
Write-Host "Transaction: tx_003"
Write-Host "Expected: Node 1 tries to vote twice with different values"
Write-Host ""

Write-Host "Node 1 votes 50..." -ForegroundColor White
docker-compose exec -T node1 /app/threshold-voting-system vote --tx-id tx_003 --value 50

Write-Host "Node 1 votes 99 (Double voting attempt)..." -ForegroundColor Red
docker-compose exec -T node1 /app/threshold-voting-system vote --tx-id tx_003 --value 99

Write-Host ""
Write-Host "Checking logs for double voting detection..." -ForegroundColor Yellow
docker-compose logs --tail=20 | Select-String -Pattern "double" -CaseSensitive:$false

Write-Host ""
Write-Host "=====================================" -ForegroundColor Cyan
Write-Host "Test Complete!" -ForegroundColor Cyan
Write-Host "=====================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "To view full logs:" -ForegroundColor Yellow
Write-Host "  docker-compose logs -f"
Write-Host ""
Write-Host "To stop all services:" -ForegroundColor Yellow
Write-Host "  docker-compose down"
Write-Host ""
Write-Host "To clean up everything (including data):" -ForegroundColor Yellow
Write-Host "  docker-compose down -v"
Write-Host ""
