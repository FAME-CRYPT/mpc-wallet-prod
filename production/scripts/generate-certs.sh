#!/bin/bash
#
# generate-certs.sh - Generate certificates for MPC Wallet nodes
#
# Usage: ./generate-certs.sh [num_nodes] [--encrypt-ca]
#
# This script generates:
#   - Root CA certificate and private key
#   - Individual node certificates signed by the CA
#   - Proper CN format: node-{id}
#
# Arguments:
#   num_nodes    - Number of node certificates to generate (default: 5)
#   --encrypt-ca - Encrypt the CA private key with a passphrase (optional)
#
# Example:
#   ./generate-certs.sh 5
#   ./generate-certs.sh 10 --encrypt-ca
#

set -e  # Exit on error

# Source common functions
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/certs-common.sh"

# Default number of nodes
NUM_NODES=5
ENCRYPT_CA=false

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --encrypt-ca)
            ENCRYPT_CA=true
            shift
            ;;
        [0-9]*)
            NUM_NODES=$1
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [num_nodes] [--encrypt-ca]"
            echo ""
            echo "Arguments:"
            echo "  num_nodes    - Number of node certificates to generate (default: 5)"
            echo "  --encrypt-ca - Encrypt the CA private key with a passphrase"
            echo ""
            echo "Example:"
            echo "  $0 5"
            echo "  $0 10 --encrypt-ca"
            exit 0
            ;;
        *)
            print_error "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Validate number of nodes
if [ "$NUM_NODES" -lt 1 ]; then
    print_error "Number of nodes must be at least 1"
    exit 1
fi

if [ "$NUM_NODES" -gt 100 ]; then
    print_error "Number of nodes cannot exceed 100"
    exit 1
fi

# Print banner
echo ""
print_header "🔐 MPC Wallet Certificate Generator"
print_header "====================================="
echo ""

# Check OpenSSL
if ! check_openssl; then
    exit 1
fi

echo ""

# Create certs directory
print_info "Creating certs directory..."
ensure_certs_dir

# Check if certificates already exist
if [ -f "$CERTS_DIR/ca.crt" ] || [ -f "$CERTS_DIR/ca.key" ]; then
    print_warning "CA certificate or key already exists!"
    read -p "Do you want to overwrite? This will invalidate all existing node certificates (y/N): " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        print_info "Aborting. Use renew-certs.sh to renew node certificates."
        exit 0
    fi
fi

echo ""
print_success "Creating certs directory..."

# Generate CA certificate
print_info "Generating Root CA certificate (${CA_KEY_SIZE}-bit RSA)..."

CA_KEY="$CERTS_DIR/ca.key"
CA_CERT="$CERTS_DIR/ca.crt"
CA_SUBJECT=$(get_subject "MPC-Wallet-Root-CA")

# Generate CA private key
if [ "$ENCRYPT_CA" = true ]; then
    print_info "CA key will be encrypted with a passphrase"
    openssl genrsa -aes256 -out "$CA_KEY" $CA_KEY_SIZE 2>&1
    if [ $? -ne 0 ]; then
        print_error "Failed to generate CA private key"
        exit 1
    fi
else
    openssl genrsa -out "$CA_KEY" $CA_KEY_SIZE 2>&1 > /dev/null
    if [ $? -ne 0 ]; then
        print_error "Failed to generate CA private key"
        exit 1
    fi
fi

# Generate CA certificate
openssl req -new -x509 -days $CA_VALIDITY_DAYS -key "$CA_KEY" -out "$CA_CERT" \
    -subj "$CA_SUBJECT" -sha256 2>&1 > /dev/null
if [ $? -ne 0 ]; then
    print_error "Failed to generate CA certificate"
    exit 1
fi

# Set permissions
set_permissions "$CA_KEY" $KEY_PERMISSIONS
set_permissions "$CA_CERT" $CERT_PERMISSIONS

print_success "Generating Root CA certificate (${CA_KEY_SIZE}-bit RSA)..."

# Generate node certificates
echo ""
for i in $(seq 1 $NUM_NODES); do
    print_info "Generating node-$i certificate..."

    NODE_KEY="$CERTS_DIR/node$i.key"
    NODE_CSR="$CERTS_DIR/node$i.csr"
    NODE_CERT="$CERTS_DIR/node$i.crt"
    NODE_SUBJECT=$(get_subject "node-$i")

    # Generate node private key
    openssl genrsa -out "$NODE_KEY" $NODE_KEY_SIZE 2>&1 > /dev/null
    if [ $? -ne 0 ]; then
        print_error "Failed to generate private key for node-$i"
        continue
    fi

    # Generate certificate signing request
    openssl req -new -key "$NODE_KEY" -out "$NODE_CSR" \
        -subj "$NODE_SUBJECT" 2>&1 > /dev/null
    if [ $? -ne 0 ]; then
        print_error "Failed to generate CSR for node-$i"
        continue
    fi

    # Generate node certificate signed by CA
    openssl x509 -req -in "$NODE_CSR" -CA "$CA_CERT" -CAkey "$CA_KEY" \
        -CAcreateserial -out "$NODE_CERT" -days $NODE_VALIDITY_DAYS \
        -sha256 -passin pass: 2>&1 > /dev/null
    if [ $? -ne 0 ]; then
        print_error "Failed to generate certificate for node-$i"
        continue
    fi

    # Set permissions
    set_permissions "$NODE_KEY" $KEY_PERMISSIONS
    set_permissions "$NODE_CERT" $CERT_PERMISSIONS

    # Clean up CSR
    rm -f "$NODE_CSR"

    print_success "Generating node-$i certificate..."
done

# Clean up CA serial file
rm -f "$CERTS_DIR/ca.srl"

# Verify certificates
echo ""
print_info "Verifying certificates..."

ALL_VALID=true
for i in $(seq 1 $NUM_NODES); do
    NODE_CERT="$CERTS_DIR/node$i.crt"

    if verify_cert "$NODE_CERT" "$CA_CERT"; then
        print_success "node-$i certificate verified"
    else
        print_error "node-$i certificate verification failed"
        ALL_VALID=false
    fi
done

if [ "$ALL_VALID" = false ]; then
    print_error "Some certificates failed verification!"
    exit 1
fi

echo ""
print_success "Verifying certificates..."

# Set file permissions
echo ""
print_info "Setting file permissions..."
print_success "Setting file permissions..."

# Create .gitignore
create_gitignore

# Print summary
echo ""
print_header "📋 Certificate Summary"
print_header "======================="
echo ""

# CA certificate details
CA_EXPIRY=$(get_cert_expiry "$CA_CERT")
echo "CA Certificate:    $CA_CERT"
echo "  Valid until: $CA_EXPIRY"
echo ""

# Node certificates
echo "Node Certificates: $NUM_NODES generated"
for i in $(seq 1 $NUM_NODES); do
    NODE_CERT="$CERTS_DIR/node$i.crt"
    NODE_EXPIRY=$(get_cert_expiry "$NODE_CERT")

    if [ $i -eq $NUM_NODES ]; then
        echo "  └─ node-$i: $NODE_CERT (Valid until: $NODE_EXPIRY)"
    else
        echo "  ├─ node-$i: $NODE_CERT (Valid until: $NODE_EXPIRY)"
    fi
done

echo ""
print_success "All certificates generated successfully!"
echo ""

# Security warnings
print_header "⚠️  IMPORTANT SECURITY NOTES:"
echo "  • Keep ca.key secure and backed up"
echo "  • Never commit .key files to git"
echo "  • Rotate node certificates yearly"
echo "  • Use ./verify-certs.sh to check validity"
if [ "$ENCRYPT_CA" = false ]; then
    echo "  • Consider encrypting ca.key with --encrypt-ca flag"
fi

echo ""
print_header "Next steps:"
echo "  1. Back up ca.key to secure location"
echo "  2. Copy certificates to nodes:"
echo "     docker cp $CERTS_DIR/ca.crt node-1:/certs/"
echo "     docker cp $CERTS_DIR/node1.crt node-1:/certs/"
echo "     docker cp $CERTS_DIR/node1.key node-1:/certs/"
echo "  3. Verify: $SCRIPT_DIR/verify-certs.sh"
echo ""

# Print file locations
print_info "Certificate files created in: $CERTS_DIR"
echo ""

exit 0
