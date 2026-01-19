# Windows PowerShell script to install protobuf
$ErrorActionPreference = "Stop"

Write-Host "====================================="
Write-Host "Protobuf Compiler Setup for Windows"
Write-Host "====================================="
Write-Host ""

# Check if protoc already exists
if (Get-Command protoc -ErrorAction SilentlyContinue) {
    Write-Host "protoc is already installed:"
    protoc --version
    exit 0
}

Write-Host "Downloading pre-built protobuf compiler..."

$version = "28.3"
$url = "https://github.com/protocolbuffers/protobuf/releases/download/v$version/protoc-$version-win64.zip"
$downloadPath = "$env:TEMP\protoc.zip"
$installPath = "C:\protobuf"

try {
    # Download
    Write-Host "Downloading from: $url"
    Invoke-WebRequest -Uri $url -OutFile $downloadPath -UseBasicParsing
    
    # Extract
    Write-Host "Extracting to: $installPath"
    if (Test-Path $installPath) {
        Remove-Item $installPath -Recurse -Force
    }
    Expand-Archive -Path $downloadPath -DestinationPath $installPath -Force
    
    # Add to PATH
    Write-Host "Adding to system PATH..."
    $binPath = "$installPath\bin"
    $currentPath = [Environment]::GetEnvironmentVariable("Path", "Machine")
    
    if ($currentPath -notlike "*$binPath*") {
        [Environment]::SetEnvironmentVariable(
            "Path",
            "$currentPath;$binPath",
            "Machine"
        )
        $env:Path = "$env:Path;$binPath"
    }
    
    # Cleanup
    Remove-Item $downloadPath -Force
    
    Write-Host ""
    Write-Host "====================================="
    Write-Host "Installation Complete!"
    Write-Host "====================================="
    Write-Host ""
    Write-Host "protoc installed at: $installPath"
    Write-Host ""
    Write-Host "IMPORTANT: Close this PowerShell window and open a NEW one"
    Write-Host "Then run: protoc --version"
    Write-Host ""
    
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
    exit 1
}
