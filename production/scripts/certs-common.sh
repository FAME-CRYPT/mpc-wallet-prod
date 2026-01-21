#!/bin/bash
#
# certs-common.sh - Shared functions for certificate management scripts
# Part of the MPC Wallet production certificate infrastructure
#

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Default values
DEFAULT_COUNTRY="US"
DEFAULT_STATE="State"
DEFAULT_CITY="City"
DEFAULT_ORG="MPC-Wallet"

# Certificate validity periods
CA_VALIDITY_DAYS=3650   # 10 years
NODE_VALIDITY_DAYS=365  # 1 year

# Key sizes
CA_KEY_SIZE=4096
NODE_KEY_SIZE=2048

# File permissions
KEY_PERMISSIONS=600
CERT_PERMISSIONS=644

# Get the script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CERTS_DIR="$PROJECT_ROOT/certs"

# Print colored message
print_success() {
    echo -e "${GREEN}✓${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1" >&2
}

print_warning() {
    echo -e "${YELLOW}⚠${NC}  $1"
}

print_info() {
    echo -e "${CYAN}ℹ${NC}  $1"
}

print_header() {
    echo -e "${BLUE}$1${NC}"
}

# Check if OpenSSL is installed
check_openssl() {
    if ! command -v openssl &> /dev/null; then
        print_error "OpenSSL is not installed or not in PATH"
        print_info "Please install OpenSSL:"
        print_info "  • Ubuntu/Debian: sudo apt-get install openssl"
        print_info "  • macOS: brew install openssl"
        print_info "  • Windows: Download from https://slproweb.com/products/Win32OpenSSL.html"
        return 1
    fi

    local version=$(openssl version)
    print_info "Using OpenSSL: $version"
    return 0
}

# Create certs directory if it doesn't exist
ensure_certs_dir() {
    if [ ! -d "$CERTS_DIR" ]; then
        mkdir -p "$CERTS_DIR"
        print_success "Created certs directory: $CERTS_DIR"
    fi
}

# Get certificate subject string
get_subject() {
    local cn=$1
    # Use // prefix to prevent Git Bash path conversion on Windows
    echo "//C=$DEFAULT_COUNTRY/ST=$DEFAULT_STATE/L=$DEFAULT_CITY/O=$DEFAULT_ORG/CN=$cn"
}

# Get certificate expiry date
get_cert_expiry() {
    local cert_file=$1
    if [ ! -f "$cert_file" ]; then
        echo "NOT FOUND"
        return 1
    fi

    openssl x509 -in "$cert_file" -noout -enddate 2>/dev/null | cut -d= -f2
}

# Get certificate subject CN
get_cert_cn() {
    local cert_file=$1
    if [ ! -f "$cert_file" ]; then
        echo "NOT FOUND"
        return 1
    fi

    # Try both "CN = " (with spaces) and "CN=" (without spaces) formats
    local cn=$(openssl x509 -in "$cert_file" -noout -subject 2>/dev/null | sed -n 's/.*CN[[:space:]]*=[[:space:]]*\([^,]*\).*/\1/p')
    echo "$cn"
}

# Verify certificate against CA
verify_cert() {
    local cert_file=$1
    local ca_file=$2

    if [ ! -f "$cert_file" ]; then
        print_error "Certificate not found: $cert_file"
        return 1
    fi

    if [ ! -f "$ca_file" ]; then
        print_error "CA certificate not found: $ca_file"
        return 1
    fi

    openssl verify -CAfile "$ca_file" "$cert_file" &>/dev/null
    return $?
}

# Check if certificate is expiring soon (within days)
check_expiry_soon() {
    local cert_file=$1
    local days=$2

    if [ ! -f "$cert_file" ]; then
        return 2  # File not found
    fi

    # Get expiry date in seconds since epoch
    local expiry_date=$(openssl x509 -in "$cert_file" -noout -enddate 2>/dev/null | cut -d= -f2)
    local expiry_epoch=$(date -d "$expiry_date" +%s 2>/dev/null || date -j -f "%b %d %T %Y %Z" "$expiry_date" +%s 2>/dev/null)

    if [ -z "$expiry_epoch" ]; then
        return 2  # Could not parse date
    fi

    local current_epoch=$(date +%s)
    local days_until_expiry=$(( ($expiry_epoch - $current_epoch) / 86400 ))

    if [ $days_until_expiry -le $days ]; then
        return 0  # Expiring soon
    else
        return 1  # Not expiring soon
    fi
}

# Check if certificate is expired
check_expired() {
    local cert_file=$1

    if [ ! -f "$cert_file" ]; then
        return 2  # File not found
    fi

    # Get expiry date in seconds since epoch
    local expiry_date=$(openssl x509 -in "$cert_file" -noout -enddate 2>/dev/null | cut -d= -f2)
    local expiry_epoch=$(date -d "$expiry_date" +%s 2>/dev/null || date -j -f "%b %d %T %Y %Z" "$expiry_date" +%s 2>/dev/null)

    if [ -z "$expiry_epoch" ]; then
        return 2  # Could not parse date
    fi

    local current_epoch=$(date +%s)

    if [ $expiry_epoch -lt $current_epoch ]; then
        return 0  # Expired
    else
        return 1  # Not expired
    fi
}

# Set proper file permissions
set_permissions() {
    local file=$1
    local perms=$2

    if [ -f "$file" ]; then
        chmod "$perms" "$file"
    fi
}

# Backup file if it exists
backup_file() {
    local file=$1

    if [ -f "$file" ]; then
        local backup="${file}.backup.$(date +%Y%m%d_%H%M%S)"
        cp "$file" "$backup"
        print_info "Backed up $file to $backup"
        return 0
    fi
    return 1
}

# Create .gitignore in certs directory
create_gitignore() {
    local gitignore="$CERTS_DIR/.gitignore"

    cat > "$gitignore" << 'EOF'
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
EOF

    print_success "Created .gitignore in certs directory"
}

# Print certificate details
print_cert_details() {
    local cert_file=$1
    local label=$2

    if [ ! -f "$cert_file" ]; then
        echo "  $label: NOT FOUND"
        return 1
    fi

    local cn=$(get_cert_cn "$cert_file")
    local expiry=$(get_cert_expiry "$cert_file")

    echo "  $label:"
    echo "    CN: $cn"
    echo "    Expiry: $expiry"

    # Check if expiring soon
    if check_expired "$cert_file"; then
        print_error "    Status: EXPIRED"
    elif check_expiry_soon "$cert_file" 30; then
        print_warning "    Status: Expiring soon (<30 days)"
    else
        echo "    Status: Valid"
    fi
}

# Generate a random serial number for certificates
generate_serial() {
    openssl rand -hex 16
}

# Export functions for use in other scripts
export -f print_success
export -f print_error
export -f print_warning
export -f print_info
export -f print_header
export -f check_openssl
export -f ensure_certs_dir
export -f get_subject
export -f get_cert_expiry
export -f get_cert_cn
export -f verify_cert
export -f check_expiry_soon
export -f check_expired
export -f set_permissions
export -f backup_file
export -f create_gitignore
export -f print_cert_details
export -f generate_serial
