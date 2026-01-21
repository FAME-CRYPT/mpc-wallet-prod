# Demo script for threshold-wallet CLI (PowerShell)
# This script demonstrates common CLI operations

$ErrorActionPreference = "Stop"

Write-Host "=== Threshold Wallet CLI Demo ===" -ForegroundColor Cyan
Write-Host ""

# Set the CLI binary path
$CLI = "threshold-wallet.exe"

# Check if API server is running
Write-Host "1. Checking API server health..." -ForegroundColor Yellow
try {
    $response = Invoke-RestMethod -Uri "http://localhost:8080/health" -ErrorAction SilentlyContinue
    Write-Host "   ✓ API server is running" -ForegroundColor Green
} catch {
    Write-Host "   ✗ API server is not running. Start it first with:" -ForegroundColor Red
    Write-Host "     cargo run --package threshold-api"
    exit 1
}
Write-Host ""

# Configure CLI
Write-Host "2. Configuring CLI..." -ForegroundColor Yellow
& $CLI config show
Write-Host ""

# Get wallet address
Write-Host "3. Getting wallet address..." -ForegroundColor Yellow
& $CLI wallet address
Write-Host ""

# Get wallet balance
Write-Host "4. Checking wallet balance..." -ForegroundColor Yellow
& $CLI wallet balance
Write-Host ""

# List transactions
Write-Host "5. Listing transactions..." -ForegroundColor Yellow
& $CLI tx list
Write-Host ""

# Check cluster status
Write-Host "6. Checking cluster status..." -ForegroundColor Yellow
& $CLI cluster status
Write-Host ""

# List cluster nodes
Write-Host "7. Listing cluster nodes..." -ForegroundColor Yellow
& $CLI cluster nodes
Write-Host ""

# Example: Send transaction (commented out - requires manual confirmation)
# Write-Host "8. Sending transaction..." -ForegroundColor Yellow
# & $CLI send `
#   --to tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx `
#   --amount 50000 `
#   --metadata "Test transaction"
# Write-Host ""

# JSON output example
Write-Host "8. Getting balance in JSON format..." -ForegroundColor Yellow
$balance = & $CLI wallet balance --json | ConvertFrom-Json
Write-Host "   Total: $($balance.total) sats" -ForegroundColor Green
Write-Host ""

Write-Host "=== Demo Complete ===" -ForegroundColor Cyan
Write-Host ""
Write-Host "Available commands:" -ForegroundColor Yellow
Write-Host "  wallet balance        - Check wallet balance"
Write-Host "  wallet address        - Get receiving address"
Write-Host "  send                  - Send Bitcoin"
Write-Host "  tx status <txid>      - Check transaction status"
Write-Host "  tx list               - List all transactions"
Write-Host "  cluster status        - View cluster health"
Write-Host "  cluster nodes         - List cluster nodes"
Write-Host "  dkg start             - Start DKG ceremony"
Write-Host "  presig generate       - Generate presignatures"
Write-Host ""
Write-Host "For more help: threshold-wallet --help" -ForegroundColor Cyan
