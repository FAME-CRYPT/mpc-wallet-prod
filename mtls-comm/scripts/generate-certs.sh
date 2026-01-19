#!/bin/bash
set -e

echo "🔐 Generating mTLS Certificates for MPC Wallet"
echo "================================================"

# Create certificates directory
mkdir -p certs

# 1. Generate Root CA
echo "📜 Step 1: Generating Root CA..."
openssl genrsa -out certs/ca.key 4096 2>/dev/null
openssl req -new -x509 -days 3650 -key certs/ca.key \
    -out certs/ca.crt \
    -subj "/C=US/ST=State/L=City/O=MPC-Wallet/CN=RootCA" 2>/dev/null

echo "   ✓ Root CA generated (valid for 10 years)"

# 2. Generate node certificates (node-1 to node-5)
echo ""
echo "📜 Step 2: Generating node certificates..."

for i in {1..5}; do
    echo "   Generating node-$i certificate..."

    # Generate private key
    openssl genrsa -out certs/node${i}.key 2048 2>/dev/null

    # Generate CSR
    openssl req -new -key certs/node${i}.key \
        -out certs/node${i}.csr \
        -subj "/C=US/ST=State/L=City/O=MPC-Wallet/CN=node-${i}" 2>/dev/null

    # Sign with CA
    openssl x509 -req -days 365 \
        -in certs/node${i}.csr \
        -CA certs/ca.crt \
        -CAkey certs/ca.key \
        -CAcreateserial \
        -out certs/node${i}.crt 2>/dev/null

    # Clean up CSR
    rm certs/node${i}.csr

    echo "   ✓ node-$i certificate generated (valid for 1 year)"
done

echo ""
echo "================================================"
echo "✅ Certificate generation complete!"
echo ""
echo "Generated files:"
echo "  📁 certs/"
echo "     ├── ca.crt          (Root CA certificate)"
echo "     ├── ca.key          (Root CA private key)"
echo "     ├── node1.crt       (Node 1 certificate)"
echo "     ├── node1.key       (Node 1 private key)"
echo "     ├── node2.crt       (Node 2 certificate)"
echo "     ├── node2.key       (Node 2 private key)"
echo "     ├── node3.crt       (Node 3 certificate)"
echo "     ├── node3.key       (Node 3 private key)"
echo "     ├── node4.crt       (Node 4 certificate)"
echo "     ├── node4.key       (Node 4 private key)"
echo "     ├── node5.crt       (Node 5 certificate)"
echo "     └── node5.key       (Node 5 private key)"
echo ""
echo "🔒 Security notes:"
echo "   - Keep ca.key secure (can issue new node certificates)"
echo "   - Each node*.key file must be kept private"
echo "   - Certificates expire in 1 year - renewal required"
echo ""
echo "📝 Next steps:"
echo "   1. Update config/default.toml with correct certificate paths"
echo "   2. Deploy certificates to respective nodes (read-only)"
echo "   3. Start nodes with: cargo run"
echo ""
