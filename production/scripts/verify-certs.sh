#!/bin/bash
#
# verify-certs.sh - Verify certificates for MPC Wallet
#
# Usage: ./verify-certs.sh [--verbose] [--warn-days N]
#
# This script verifies:
#   - Certificate validity (not expired)
#   - Certificate signatures against CA
#   - CN format correctness
#   - Expiry warnings
#
# Arguments:
#   --verbose    - Show detailed certificate information
#   --warn-days  - Days before expiry to warn (default: 30)
#
# Example:
#   ./verify-certs.sh
#   ./verify-certs.sh --verbose
#   ./verify-certs.sh --warn-days 60
#

set -e  # Exit on error

# Source common functions
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/certs-common.sh"

# Default options
VERBOSE=false
WARN_DAYS=30

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --verbose|-v)
            VERBOSE=true
            shift
            ;;
        --warn-days)
            if [[ -n "$2" && "$2" =~ ^[0-9]+$ ]]; then
                WARN_DAYS=$2
                shift 2
            else
                print_error "Invalid value for --warn-days"
                exit 1
            fi
            ;;
        -h|--help)
            echo "Usage: $0 [--verbose] [--warn-days N]"
            echo ""
            echo "Arguments:"
            echo "  --verbose    - Show detailed certificate information"
            echo "  --warn-days  - Days before expiry to warn (default: 30)"
            echo ""
            echo "Example:"
            echo "  $0"
            echo "  $0 --verbose"
            echo "  $0 --warn-days 60"
            exit 0
            ;;
        *)
            print_error "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Print banner
echo ""
print_header "🔍 MPC Wallet Certificate Verification"
print_header "========================================"
echo ""

# Check OpenSSL
if ! check_openssl; then
    exit 1
fi

echo ""

# Check if certificates directory exists
if [ ! -d "$CERTS_DIR" ]; then
    print_error "Certificates directory not found: $CERTS_DIR"
    print_info "Run generate-certs.sh first to create certificates"
    exit 1
fi

# Counters
TOTAL_CERTS=0
VALID_CERTS=0
INVALID_CERTS=0
EXPIRED_CERTS=0
EXPIRING_SOON=0

# Verify CA certificate
print_header "CA Certificate"
print_header "---------------"

CA_KEY="$CERTS_DIR/ca.key"
CA_CERT="$CERTS_DIR/ca.crt"

if [ ! -f "$CA_CERT" ]; then
    print_error "CA certificate not found: $CA_CERT"
    exit 1
fi

if [ ! -f "$CA_KEY" ]; then
    print_warning "CA private key not found: $CA_KEY"
else
    print_success "CA private key found"
fi

# Get CA details
CA_CN=$(get_cert_cn "$CA_CERT")
CA_EXPIRY=$(get_cert_expiry "$CA_CERT")

echo "  Subject CN: $CA_CN"
echo "  Valid until: $CA_EXPIRY"

# Check CA expiry
if check_expired "$CA_CERT"; then
    print_error "  Status: EXPIRED"
    EXPIRED_CERTS=$((EXPIRED_CERTS + 1))
    echo ""
    print_error "CA certificate has expired! All node certificates are invalid."
    print_info "Run generate-certs.sh to create new certificates"
    exit 1
elif check_expiry_soon "$CA_CERT" $WARN_DAYS; then
    print_warning "  Status: Expiring soon (< $WARN_DAYS days)"
    EXPIRING_SOON=$((EXPIRING_SOON + 1))
else
    print_success "  Status: Valid"
fi

# Verbose CA details
if [ "$VERBOSE" = true ]; then
    echo ""
    echo "  Detailed Information:"
    openssl x509 -in "$CA_CERT" -noout -text | grep -E "Subject:|Issuer:|Not Before|Not After|Public-Key:" | sed 's/^/    /'
fi

echo ""

# Find all node certificates
print_header "Node Certificates"
print_header "-----------------"
echo ""

NODE_CERTS=()
for cert in "$CERTS_DIR"/node*.crt; do
    if [ -f "$cert" ]; then
        NODE_CERTS+=("$cert")
    fi
done

if [ ${#NODE_CERTS[@]} -eq 0 ]; then
    print_warning "No node certificates found"
    exit 0
fi

print_info "Found ${#NODE_CERTS[@]} node certificate(s)"
echo ""

# Verify each node certificate
for cert in "${NODE_CERTS[@]}"; do
    filename=$(basename "$cert")
    node_id=${filename#node}
    node_id=${node_id%.crt}

    TOTAL_CERTS=$((TOTAL_CERTS + 1))

    print_header "Node $node_id"

    # Get certificate details
    NODE_CN=$(get_cert_cn "$cert")
    NODE_EXPIRY=$(get_cert_expiry "$cert")

    echo "  Certificate: $cert"
    echo "  Subject CN: $NODE_CN"
    echo "  Valid until: $NODE_EXPIRY"

    # Check CN format
    EXPECTED_CN="node-$node_id"
    if [ "$NODE_CN" != "$EXPECTED_CN" ]; then
        print_warning "  CN mismatch: Expected '$EXPECTED_CN', got '$NODE_CN'"
    fi

    # Check if private key exists
    NODE_KEY="${cert%.crt}.key"
    if [ ! -f "$NODE_KEY" ]; then
        print_warning "  Private key not found: $NODE_KEY"
    else
        print_success "  Private key found"
    fi

    # Verify signature
    if verify_cert "$cert" "$CA_CERT"; then
        print_success "  Signature verification: PASSED"
    else
        print_error "  Signature verification: FAILED"
        INVALID_CERTS=$((INVALID_CERTS + 1))
        echo ""
        continue
    fi

    # Check expiry
    if check_expired "$cert"; then
        print_error "  Status: EXPIRED"
        EXPIRED_CERTS=$((EXPIRED_CERTS + 1))
    elif check_expiry_soon "$cert" $WARN_DAYS; then
        print_warning "  Status: Expiring soon (< $WARN_DAYS days)"
        EXPIRING_SOON=$((EXPIRING_SOON + 1))
        VALID_CERTS=$((VALID_CERTS + 1))
    else
        print_success "  Status: Valid"
        VALID_CERTS=$((VALID_CERTS + 1))
    fi

    # Verbose certificate details
    if [ "$VERBOSE" = true ]; then
        echo ""
        echo "  Detailed Information:"
        openssl x509 -in "$cert" -noout -text | grep -E "Subject:|Issuer:|Not Before|Not After|Public-Key:" | sed 's/^/    /'
    fi

    echo ""
done

# Print summary
print_header "📋 Verification Summary"
print_header "======================="
echo ""

echo "Total certificates checked: $TOTAL_CERTS"
print_success "Valid certificates: $VALID_CERTS"

if [ $INVALID_CERTS -gt 0 ]; then
    print_error "Invalid/Failed verification: $INVALID_CERTS"
fi

if [ $EXPIRED_CERTS -gt 0 ]; then
    print_error "Expired certificates: $EXPIRED_CERTS"
fi

if [ $EXPIRING_SOON -gt 0 ]; then
    print_warning "Expiring soon (< $WARN_DAYS days): $EXPIRING_SOON"
fi

echo ""

# Recommendations
if [ $EXPIRED_CERTS -gt 0 ] || [ $INVALID_CERTS -gt 0 ]; then
    print_header "⚠️  Action Required:"
    if [ $EXPIRED_CERTS -gt 0 ]; then
        echo "  • Expired certificates found"
        echo "  • Run: $SCRIPT_DIR/renew-certs.sh all"
    fi
    if [ $INVALID_CERTS -gt 0 ]; then
        echo "  • Invalid certificates found"
        echo "  • Check certificate files and CA"
        echo "  • May need to regenerate: $SCRIPT_DIR/generate-certs.sh"
    fi
    echo ""
    exit 1
fi

if [ $EXPIRING_SOON -gt 0 ]; then
    print_header "⚠️  Recommended Actions:"
    echo "  • Certificates expiring soon"
    echo "  • Plan to renew: $SCRIPT_DIR/renew-certs.sh all"
    echo "  • Or renew specific nodes: $SCRIPT_DIR/renew-certs.sh <node_id>"
    echo ""
fi

if [ $VALID_CERTS -eq $TOTAL_CERTS ] && [ $EXPIRING_SOON -eq 0 ]; then
    print_success "All certificates are valid and not expiring soon!"
    echo ""
fi

# File permissions check
print_header "🔒 Security Check"
print_header "=================="
echo ""

PERMISSION_ISSUES=0

# Check CA key permissions
if [ -f "$CA_KEY" ]; then
    CA_KEY_PERMS=$(stat -c %a "$CA_KEY" 2>/dev/null || stat -f %A "$CA_KEY" 2>/dev/null)
    if [ "$CA_KEY_PERMS" != "600" ]; then
        print_warning "CA key has insecure permissions: $CA_KEY_PERMS (should be 600)"
        print_info "Fix with: chmod 600 $CA_KEY"
        PERMISSION_ISSUES=$((PERMISSION_ISSUES + 1))
    else
        print_success "CA key permissions: OK"
    fi
fi

# Check node key permissions
for cert in "${NODE_CERTS[@]}"; do
    NODE_KEY="${cert%.crt}.key"
    if [ -f "$NODE_KEY" ]; then
        NODE_KEY_PERMS=$(stat -c %a "$NODE_KEY" 2>/dev/null || stat -f %A "$NODE_KEY" 2>/dev/null)
        if [ "$NODE_KEY_PERMS" != "600" ]; then
            print_warning "$(basename $NODE_KEY) has insecure permissions: $NODE_KEY_PERMS (should be 600)"
            PERMISSION_ISSUES=$((PERMISSION_ISSUES + 1))
        fi
    fi
done

if [ $PERMISSION_ISSUES -eq 0 ]; then
    print_success "All private keys have secure permissions"
fi

echo ""

# Check .gitignore
if [ ! -f "$CERTS_DIR/.gitignore" ]; then
    print_warning ".gitignore not found in certs directory"
    print_info "Private keys might be accidentally committed!"
    print_info "Run generate-certs.sh to create .gitignore"
else
    print_success ".gitignore present in certs directory"
fi

echo ""

exit 0
