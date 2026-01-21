#!/bin/bash
#
# renew-certs.sh - Renew node certificates for MPC Wallet
#
# Usage: ./renew-certs.sh [node_id|all] [--backup]
#
# This script renews node certificates while keeping the CA unchanged.
# It creates backups of old certificates before renewal.
#
# Arguments:
#   node_id  - Specific node ID to renew (e.g., 1, 2, 3) or "all"
#   --backup - Create backups in certs/backups/ directory (default: yes)
#
# Example:
#   ./renew-certs.sh 1           # Renew node-1 certificate
#   ./renew-certs.sh all         # Renew all node certificates
#   ./renew-certs.sh 3 --backup  # Renew node-3 with explicit backup
#

set -e  # Exit on error

# Source common functions
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/certs-common.sh"

# Default options
RENEW_MODE="all"
CREATE_BACKUP=true

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --backup)
            CREATE_BACKUP=true
            shift
            ;;
        --no-backup)
            CREATE_BACKUP=false
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [node_id|all] [--backup|--no-backup]"
            echo ""
            echo "Arguments:"
            echo "  node_id      - Specific node ID to renew (e.g., 1, 2, 3)"
            echo "  all          - Renew all node certificates (default)"
            echo "  --backup     - Create backups (default)"
            echo "  --no-backup  - Skip creating backups"
            echo ""
            echo "Example:"
            echo "  $0 1              # Renew node-1"
            echo "  $0 all            # Renew all nodes"
            echo "  $0 3 --no-backup  # Renew node-3 without backup"
            exit 0
            ;;
        all)
            RENEW_MODE="all"
            shift
            ;;
        [0-9]*)
            RENEW_MODE="$1"
            shift
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
print_header "🔄 MPC Wallet Certificate Renewal"
print_header "==================================="
echo ""

# Check OpenSSL
if ! check_openssl; then
    exit 1
fi

echo ""

# Check if CA exists
CA_KEY="$CERTS_DIR/ca.key"
CA_CERT="$CERTS_DIR/ca.crt"

if [ ! -f "$CA_CERT" ] || [ ! -f "$CA_KEY" ]; then
    print_error "CA certificate or key not found!"
    print_info "Please run generate-certs.sh first to create the CA"
    exit 1
fi

# Verify CA certificate
print_info "Verifying CA certificate..."
CA_EXPIRY=$(get_cert_expiry "$CA_CERT")
print_success "CA certificate valid until: $CA_EXPIRY"

# Check if CA is expired
if check_expired "$CA_CERT"; then
    print_error "CA certificate has expired!"
    print_info "You need to regenerate all certificates using generate-certs.sh"
    exit 1
fi

# Warn if CA is expiring soon
if check_expiry_soon "$CA_CERT" 90; then
    print_warning "CA certificate expires in less than 90 days!"
    print_info "Consider regenerating all certificates soon"
fi

echo ""

# Create backup directory
BACKUP_DIR="$CERTS_DIR/backups"
if [ "$CREATE_BACKUP" = true ]; then
    mkdir -p "$BACKUP_DIR"
    BACKUP_TIMESTAMP=$(date +%Y%m%d_%H%M%S)
    BACKUP_SUBDIR="$BACKUP_DIR/$BACKUP_TIMESTAMP"
    mkdir -p "$BACKUP_SUBDIR"
    print_success "Created backup directory: $BACKUP_SUBDIR"
fi

# Function to renew a single node certificate
renew_node_cert() {
    local node_id=$1

    print_info "Renewing node-$node_id certificate..."

    NODE_KEY="$CERTS_DIR/node$node_id.key"
    NODE_CSR="$CERTS_DIR/node$node_id.csr"
    NODE_CERT="$CERTS_DIR/node$node_id.crt"
    NODE_SUBJECT=$(get_subject "node-$node_id")

    # Check if certificate exists
    if [ ! -f "$NODE_CERT" ]; then
        print_warning "Node-$node_id certificate not found, creating new..."
    else
        # Backup old certificate
        if [ "$CREATE_BACKUP" = true ]; then
            cp "$NODE_CERT" "$BACKUP_SUBDIR/node$node_id.crt"
            if [ -f "$NODE_KEY" ]; then
                cp "$NODE_KEY" "$BACKUP_SUBDIR/node$node_id.key"
            fi
            print_info "Backed up old certificate to $BACKUP_SUBDIR"
        fi

        # Show old expiry
        OLD_EXPIRY=$(get_cert_expiry "$NODE_CERT")
        print_info "Old certificate expires: $OLD_EXPIRY"
    fi

    # Generate new private key if it doesn't exist
    if [ ! -f "$NODE_KEY" ]; then
        print_info "Generating new private key for node-$node_id..."
        openssl genrsa -out "$NODE_KEY" $NODE_KEY_SIZE 2>&1 > /dev/null
        if [ $? -ne 0 ]; then
            print_error "Failed to generate private key for node-$node_id"
            return 1
        fi
        set_permissions "$NODE_KEY" $KEY_PERMISSIONS
    fi

    # Generate certificate signing request
    openssl req -new -key "$NODE_KEY" -out "$NODE_CSR" \
        -subj "$NODE_SUBJECT" 2>&1 > /dev/null
    if [ $? -ne 0 ]; then
        print_error "Failed to generate CSR for node-$node_id"
        return 1
    fi

    # Generate new certificate signed by CA
    openssl x509 -req -in "$NODE_CSR" -CA "$CA_CERT" -CAkey "$CA_KEY" \
        -CAcreateserial -out "$NODE_CERT" -days $NODE_VALIDITY_DAYS \
        -sha256 -passin pass: 2>&1 > /dev/null
    if [ $? -ne 0 ]; then
        print_error "Failed to generate certificate for node-$node_id"
        return 1
    fi

    # Set permissions
    set_permissions "$NODE_CERT" $CERT_PERMISSIONS

    # Clean up CSR
    rm -f "$NODE_CSR"

    # Verify new certificate
    if verify_cert "$NODE_CERT" "$CA_CERT"; then
        NEW_EXPIRY=$(get_cert_expiry "$NODE_CERT")
        print_success "Node-$node_id certificate renewed (Valid until: $NEW_EXPIRY)"
        return 0
    else
        print_error "Node-$node_id certificate verification failed!"
        return 1
    fi
}

# Determine which certificates to renew
if [ "$RENEW_MODE" = "all" ]; then
    # Find all existing node certificates
    NODE_CERTS=()
    for cert in "$CERTS_DIR"/node*.crt; do
        if [ -f "$cert" ]; then
            # Extract node ID from filename
            filename=$(basename "$cert")
            node_id=${filename#node}
            node_id=${node_id%.crt}
            NODE_CERTS+=("$node_id")
        fi
    done

    if [ ${#NODE_CERTS[@]} -eq 0 ]; then
        print_error "No node certificates found to renew!"
        print_info "Use generate-certs.sh to create certificates"
        exit 1
    fi

    print_info "Found ${#NODE_CERTS[@]} node certificate(s) to renew"
    echo ""

    # Renew all certificates
    SUCCESS_COUNT=0
    FAIL_COUNT=0

    for node_id in "${NODE_CERTS[@]}"; do
        if renew_node_cert "$node_id"; then
            ((SUCCESS_COUNT++))
        else
            ((FAIL_COUNT++))
        fi
        echo ""
    done

    # Clean up CA serial file
    rm -f "$CERTS_DIR/ca.srl"

    # Print summary
    print_header "📋 Renewal Summary"
    print_header "=================="
    echo ""
    print_success "Successfully renewed: $SUCCESS_COUNT certificate(s)"
    if [ $FAIL_COUNT -gt 0 ]; then
        print_error "Failed to renew: $FAIL_COUNT certificate(s)"
    fi

else
    # Renew specific node
    if ! [[ "$RENEW_MODE" =~ ^[0-9]+$ ]]; then
        print_error "Invalid node ID: $RENEW_MODE"
        exit 1
    fi

    if renew_node_cert "$RENEW_MODE"; then
        echo ""
        print_success "Certificate renewal completed!"
    else
        echo ""
        print_error "Certificate renewal failed!"
        exit 1
    fi

    # Clean up CA serial file
    rm -f "$CERTS_DIR/ca.srl"
fi

echo ""

# Security reminder
print_header "⚠️  Security Reminder:"
echo "  • Old certificates have been backed up to: $BACKUP_SUBDIR"
echo "  • Update node configurations with new certificates"
echo "  • Restart nodes to use new certificates"
echo "  • Verify with: $SCRIPT_DIR/verify-certs.sh"

echo ""
exit 0
