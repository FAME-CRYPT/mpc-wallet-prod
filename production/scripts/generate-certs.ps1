#
# generate-certs.ps1 - Generate certificates for MPC Wallet nodes (Windows)
#
# Usage: .\generate-certs.ps1 [-NumNodes <int>] [-EncryptCA]
#
# This script generates:
#   - Root CA certificate and private key
#   - Individual node certificates signed by the CA
#   - Proper CN format: node-{id}
#
# Parameters:
#   -NumNodes    - Number of node certificates to generate (default: 5)
#   -EncryptCA   - Encrypt the CA private key with a passphrase (optional)
#
# Example:
#   .\generate-certs.ps1
#   .\generate-certs.ps1 -NumNodes 10
#   .\generate-certs.ps1 -NumNodes 5 -EncryptCA
#

param(
    [Parameter(Position=0)]
    [ValidateRange(1, 100)]
    [int]$NumNodes = 5,

    [switch]$EncryptCA = $false,

    [switch]$Help = $false
)

# Show help
if ($Help) {
    Write-Host "Usage: .\generate-certs.ps1 [-NumNodes <int>] [-EncryptCA]"
    Write-Host ""
    Write-Host "Parameters:"
    Write-Host "  -NumNodes    - Number of node certificates to generate (default: 5)"
    Write-Host "  -EncryptCA   - Encrypt the CA private key with a passphrase"
    Write-Host ""
    Write-Host "Example:"
    Write-Host "  .\generate-certs.ps1"
    Write-Host "  .\generate-certs.ps1 -NumNodes 10"
    Write-Host "  .\generate-certs.ps1 -NumNodes 5 -EncryptCA"
    exit 0
}

# Configuration
$DEFAULT_COUNTRY = "US"
$DEFAULT_STATE = "State"
$DEFAULT_CITY = "City"
$DEFAULT_ORG = "MPC-Wallet"

$CA_VALIDITY_DAYS = 3650   # 10 years
$NODE_VALIDITY_DAYS = 365  # 1 year

$CA_KEY_SIZE = 4096
$NODE_KEY_SIZE = 2048

# Paths
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $ScriptDir
$CertsDir = Join-Path $ProjectRoot "certs"

# Helper functions
function Write-ColorOutput {
    param(
        [string]$Message,
        [string]$Type = "Info"
    )

    switch ($Type) {
        "Success" { Write-Host "✓ $Message" -ForegroundColor Green }
        "Error"   { Write-Host "✗ $Message" -ForegroundColor Red }
        "Warning" { Write-Host "⚠  $Message" -ForegroundColor Yellow }
        "Info"    { Write-Host "ℹ  $Message" -ForegroundColor Cyan }
        "Header"  { Write-Host $Message -ForegroundColor Blue }
        default   { Write-Host $Message }
    }
}

function Test-OpenSSL {
    try {
        $version = openssl version 2>$null
        if ($LASTEXITCODE -eq 0) {
            Write-ColorOutput "Using OpenSSL: $version" -Type Info
            return $true
        }
    } catch {
        # OpenSSL not found
    }

    Write-ColorOutput "OpenSSL is not installed or not in PATH" -Type Error
    Write-ColorOutput "Please install OpenSSL:" -Type Info
    Write-ColorOutput "  • Download from: https://slproweb.com/products/Win32OpenSSL.html" -Type Info
    Write-ColorOutput "  • Or use Chocolatey: choco install openssl" -Type Info
    return $false
}

function Get-Subject {
    param([string]$CN)
    return "/C=$DEFAULT_COUNTRY/ST=$DEFAULT_STATE/L=$DEFAULT_CITY/O=$DEFAULT_ORG/CN=$CN"
}

function Get-CertExpiry {
    param([string]$CertFile)

    if (-not (Test-Path $CertFile)) {
        return "NOT FOUND"
    }

    try {
        $output = openssl x509 -in $CertFile -noout -enddate 2>$null
        if ($output -match "notAfter=(.+)$") {
            return $matches[1]
        }
    } catch {
        return "ERROR"
    }

    return "UNKNOWN"
}

function Test-Certificate {
    param(
        [string]$CertFile,
        [string]$CAFile
    )

    if (-not (Test-Path $CertFile)) {
        return $false
    }

    if (-not (Test-Path $CAFile)) {
        return $false
    }

    try {
        openssl verify -CAfile $CAFile $CertFile 2>&1 | Out-Null
        return $LASTEXITCODE -eq 0
    } catch {
        return $false
    }
}

# Main script
Write-Host ""
Write-ColorOutput "🔐 MPC Wallet Certificate Generator" -Type Header
Write-ColorOutput "=====================================" -Type Header
Write-Host ""

# Check OpenSSL
if (-not (Test-OpenSSL)) {
    exit 1
}

Write-Host ""

# Create certs directory
Write-ColorOutput "Creating certs directory..." -Type Info

if (-not (Test-Path $CertsDir)) {
    New-Item -ItemType Directory -Path $CertsDir | Out-Null
    Write-ColorOutput "Created certs directory: $CertsDir" -Type Success
}

# Check if certificates already exist
$CAKey = Join-Path $CertsDir "ca.key"
$CACert = Join-Path $CertsDir "ca.crt"

if ((Test-Path $CACert) -or (Test-Path $CAKey)) {
    Write-ColorOutput "CA certificate or key already exists!" -Type Warning
    $response = Read-Host "Do you want to overwrite? This will invalidate all existing node certificates (y/N)"

    if ($response -ne "y" -and $response -ne "Y") {
        Write-ColorOutput "Aborting. Use renew-certs.ps1 to renew node certificates." -Type Info
        exit 0
    }
}

Write-Host ""
Write-ColorOutput "Creating certs directory..." -Type Success

# Generate CA certificate
Write-ColorOutput "Generating Root CA certificate ($CA_KEY_SIZE-bit RSA)..." -Type Info

$CASubject = Get-Subject "MPC-Wallet-Root-CA"

# Generate CA private key
if ($EncryptCA) {
    Write-ColorOutput "CA key will be encrypted with a passphrase" -Type Info
    openssl genrsa -aes256 -out $CAKey $CA_KEY_SIZE 2>$null
} else {
    openssl genrsa -out $CAKey $CA_KEY_SIZE 2>$null
}

if ($LASTEXITCODE -ne 0) {
    Write-ColorOutput "Failed to generate CA private key" -Type Error
    exit 1
}

# Generate CA certificate
openssl req -new -x509 -days $CA_VALIDITY_DAYS -key $CAKey -out $CACert -subj $CASubject -sha256 2>$null

if ($LASTEXITCODE -ne 0) {
    Write-ColorOutput "Failed to generate CA certificate" -Type Error
    exit 1
}

Write-ColorOutput "Generating Root CA certificate ($CA_KEY_SIZE-bit RSA)..." -Type Success

# Generate node certificates
Write-Host ""
for ($i = 1; $i -le $NumNodes; $i++) {
    Write-ColorOutput "Generating node-$i certificate..." -Type Info

    $NodeKey = Join-Path $CertsDir "node$i.key"
    $NodeCSR = Join-Path $CertsDir "node$i.csr"
    $NodeCert = Join-Path $CertsDir "node$i.crt"
    $NodeSubject = Get-Subject "node-$i"

    # Generate node private key
    openssl genrsa -out $NodeKey $NODE_KEY_SIZE 2>$null

    if ($LASTEXITCODE -ne 0) {
        Write-ColorOutput "Failed to generate node-$i private key" -Type Error
        continue
    }

    # Generate certificate signing request
    openssl req -new -key $NodeKey -out $NodeCSR -subj $NodeSubject 2>$null

    if ($LASTEXITCODE -ne 0) {
        Write-ColorOutput "Failed to generate node-$i CSR" -Type Error
        continue
    }

    # Generate node certificate signed by CA
    openssl x509 -req -in $NodeCSR -CA $CACert -CAkey $CAKey -CAcreateserial -out $NodeCert -days $NODE_VALIDITY_DAYS -sha256 -passin pass: 2>$null

    if ($LASTEXITCODE -ne 0) {
        Write-ColorOutput "Failed to generate node-$i certificate" -Type Error
        continue
    }

    # Clean up CSR
    Remove-Item -Path $NodeCSR -Force -ErrorAction SilentlyContinue

    Write-ColorOutput "Generating node-$i certificate..." -Type Success
}

# Clean up CA serial file
$CASerial = Join-Path $CertsDir "ca.srl"
Remove-Item -Path $CASerial -Force -ErrorAction SilentlyContinue

# Verify certificates
Write-Host ""
Write-ColorOutput "Verifying certificates..." -Type Info

$AllValid = $true
for ($i = 1; $i -le $NumNodes; $i++) {
    $NodeCert = Join-Path $CertsDir "node$i.crt"

    if (Test-Certificate -CertFile $NodeCert -CAFile $CACert) {
        Write-ColorOutput "node-$i certificate verified" -Type Success
    } else {
        Write-ColorOutput "node-$i certificate verification failed" -Type Error
        $AllValid = $false
    }
}

if (-not $AllValid) {
    Write-ColorOutput "Some certificates failed verification!" -Type Error
    exit 1
}

Write-Host ""
Write-ColorOutput "Verifying certificates..." -Type Success

# Create .gitignore
$GitIgnorePath = Join-Path $CertsDir ".gitignore"
$GitIgnoreContent = @"
# Ignore all certificate and key files for security
*.key
*.crt
*.csr
*.pem
*.p12
*.pfx

# Keep the README
!README.md

# Keep backup directory structure but not contents
backups/
*.backup.*
"@

Set-Content -Path $GitIgnorePath -Value $GitIgnoreContent
Write-ColorOutput "Created .gitignore in certs directory" -Type Success

# Print summary
Write-Host ""
Write-ColorOutput "📋 Certificate Summary" -Type Header
Write-ColorOutput "=======================" -Type Header
Write-Host ""

# CA certificate details
$CAExpiry = Get-CertExpiry $CACert
Write-Host "CA Certificate:    $CACert"
Write-Host "  Valid until: $CAExpiry"
Write-Host ""

# Node certificates
Write-Host "Node Certificates: $NumNodes generated"
for ($i = 1; $i -le $NumNodes; $i++) {
    $NodeCert = Join-Path $CertsDir "node$i.crt"
    $NodeExpiry = Get-CertExpiry $NodeCert

    if ($i -eq $NumNodes) {
        Write-Host "  └─ node-$i`: $NodeCert (Valid until: $NodeExpiry)"
    } else {
        Write-Host "  ├─ node-$i`: $NodeCert (Valid until: $NodeExpiry)"
    }
}

Write-Host ""
Write-ColorOutput "All certificates generated successfully!" -Type Success
Write-Host ""

# Security warnings
Write-ColorOutput "⚠️  IMPORTANT SECURITY NOTES:" -Type Header
Write-Host "  • Keep ca.key secure and backed up"
Write-Host "  • Never commit .key files to git"
Write-Host "  • Rotate node certificates yearly"
Write-Host "  • Use .\verify-certs.ps1 to check validity"
if (-not $EncryptCA) {
    Write-Host "  • Consider encrypting ca.key with -EncryptCA flag"
}

Write-Host ""
Write-ColorOutput "Next steps:" -Type Header
Write-Host "  1. Back up ca.key to secure location"
Write-Host "  2. Copy certificates to nodes:"
Write-Host "     docker cp $CertsDir\ca.crt node-1:/certs/"
Write-Host "     docker cp $CertsDir\node1.crt node-1:/certs/"
Write-Host "     docker cp $CertsDir\node1.key node-1:/certs/"
Write-Host "  3. Verify: .\verify-certs.ps1"
Write-Host ""

Write-ColorOutput "Certificate files created in: $CertsDir" -Type Info
Write-Host ""

exit 0
