# mTLS Threshold Voting System

Byzantine Fault Tolerant threshold signature voting system using **mutual TLS (mTLS)** instead of libp2p for secure peer-to-peer communication.

## Architecture Overview

This system replaces the libp2p networking layer from `p2p-comm` with industry-standard **mTLS** using **rustls** and **TLS 1.3**.

### Key Components

- **mTLS Networking**: Replaces libp2p (GossipSub + Kademlia + Noise Protocol)
- **Byzantine Consensus**: Unchanged from p2p-comm (proven correct)
- **Storage Layer**: etcd (coordination) + PostgreSQL (audit trail)
- **Cryptography**: Ed25519 signatures for vote verification

### Networking Comparison

| Component | p2p-comm (libp2p) | mtls-comm (mTLS) |
|-----------|------------------------|----------------------|
| Encryption | Noise Protocol XX | TLS 1.3 mutual auth |
| Broadcast | GossipSub | Custom mesh broadcast |
| Discovery | Kademlia DHT | Static bootstrap peers |
| Transport | TCP + libp2p | TCP + mTLS |

## Setup

### 1. Generate Certificates

```bash
cd mtls-comm
chmod +x scripts/generate-certs.sh
./scripts/generate-certs.sh
```

This creates:
- Root CA certificate (`certs/ca.crt`)
- Node certificates (`certs/node1.crt` to `certs/node5.crt`)
- Private keys (`certs/node*.key`)

### 2. Build

```bash
cargo build --release
```

### 3. Run Single Node (Development)

```bash
# Start infrastructure
docker-compose up -d etcd-1 etcd-2 etcd-3 postgres

# Run node
RUST_LOG=info cargo run -- run
```

### 4. Run Full Cluster (Docker)

```bash
# Build and start all services
docker-compose up -d

# Check logs
docker logs mtls-node-1
docker logs mtls-node-2
docker logs mtls-node-3
```

## Configuration

Edit `config/default.toml`:

```toml
[node]
node_id = 1
listen_addr = "0.0.0.0:9000"

[mtls]
ca_cert_path = "certs/ca.crt"
node_cert_path = "certs/node1.crt"
node_key_path = "certs/node1.key"
tls_version = "1.3"  # Enforced

[network]
bootstrap_peers = [
    "172.18.0.3:9000",  # node-2
    "172.18.0.4:9000",  # node-3
]

[consensus]
total_nodes = 5
threshold = 4
```

## CLI Commands

```bash
# Run node
cargo run -- run

# Submit vote
cargo run -- vote --tx-id "tx_001" --value 42

# Query transaction status
cargo run -- status --tx-id "tx_001"

# Show node info
cargo run -- info

# List peers
cargo run -- peers

# Test Byzantine detection
cargo run -- test-byzantine --test-type double-vote

# Monitor network
cargo run -- monitor --interval 5
```

## Testing

### Test Vote Broadcasting

```bash
# Terminal 1: Start node-1
NODE_ID=1 cargo run -- run

# Terminal 2: Submit vote
cargo run -- vote --tx-id "test_001" --value 42

# Check logs for vote propagation
docker logs mtls-node-2 | grep "Received vote"
```

### Test Byzantine Detection

```bash
# Submit conflicting votes (double-vote)
cargo run -- vote --tx-id "test_002" --value 100
cargo run -- vote --tx-id "test_002" --value 200

# Check PostgreSQL for violations
docker exec mtls-postgres psql -U mpc -d mpc_wallet \
  -c "SELECT * FROM byzantine_violations WHERE tx_id='test_002';"
```

### Verify TLS 1.3

```bash
# Test TLS connection
openssl s_client -connect localhost:9001 -tls1_3

# Should succeed with client certificate
openssl s_client -connect localhost:9001 \
  -cert certs/node2.crt \
  -key certs/node2.key \
  -CAfile certs/ca.crt
```

## Security Features

### mTLS Benefits

- ✅ **Industry Standard**: TLS 1.3 with extensive tooling
- ✅ **Mutual Authentication**: Both client and server verify certificates
- ✅ **Certificate Revocation**: Standard PKI infrastructure
- ✅ **Audit Tools**: Compatible with standard TLS monitoring tools
- ✅ **Compliance**: Meets regulatory requirements (PCI DSS, HIPAA, etc.)

### Byzantine Fault Tolerance

4 types of violations detected:

1. **Double Voting**: Same node votes different values
2. **Minority Attack**: Node votes against majority
3. **Invalid Signature**: Cryptographic verification fails
4. **Silent Failure**: Node doesn't vote within timeout

## Monitoring

### Check Node Status

```bash
docker ps --filter "name=mtls-node"
```

### View Logs

```bash
# All nodes
docker-compose logs -f

# Specific node
docker logs -f mtls-node-1

# Filter for votes
docker logs mtls-node-1 | grep "vote"
```

### Query etcd

```bash
docker exec mtls-etcd-1 etcdctl get "" --prefix
```

### Query PostgreSQL

```bash
docker exec -it mtls-postgres psql -U mpc -d mpc_wallet

# Check Byzantine violations
SELECT * FROM byzantine_violations;

# Check reputation scores
SELECT * FROM reputation_scores;
```

## Performance

Expected metrics:
- Vote propagation: <100ms
- TLS handshake: ~50ms
- Byzantine detection: ~10ms
- etcd CAS operation: ~20ms

## Troubleshooting

### Certificate Errors

```bash
# Verify certificate
openssl x509 -in certs/node1.crt -text -noout

# Check expiry
openssl x509 -in certs/node1.crt -noout -dates

# Verify chain
openssl verify -CAfile certs/ca.crt certs/node1.crt
```

### Connection Issues

```bash
# Check if port is listening
netstat -tulpn | grep 9000

# Test connectivity
telnet node-1 9000

# Check Docker network
docker network inspect mtls-comm_mpc-network
```

### etcd Issues

```bash
# Check cluster health
docker exec mtls-etcd-1 etcdctl endpoint health

# Check members
docker exec mtls-etcd-1 etcdctl member list
```

## Development

### Project Structure

```
mtls-comm/
├── crates/
│   ├── types/          # Copied from p2p-comm
│   ├── crypto/         # Copied from p2p-comm
│   ├── storage/        # Copied from p2p-comm
│   ├── consensus/      # Copied from p2p-comm
│   └── network/        # NEW: mTLS implementation
│       ├── mtls_node.rs      # Replaces P2PNode
│       ├── mesh.rs           # Mesh topology
│       ├── broadcast.rs      # Custom broadcast
│       ├── cert_manager.rs   # Certificate loading
│       └── messages.rs       # Network messages
├── src/
│   ├── main.rs         # Entry point
│   ├── app.rs          # Application logic
│   ├── config.rs       # Configuration
│   └── cli.rs          # CLI commands
├── config/
│   └── default.toml    # Default configuration
├── certs/              # mTLS certificates
├── scripts/
│   └── generate-certs.sh
└── docker/
    └── Dockerfile.node
```

### Dependencies

Key crates:
- `rustls` 0.23 - Pure Rust TLS implementation
- `tokio-rustls` 0.26 - Async TLS
- `x509-parser` 0.16 - Certificate parsing
- `ed25519-dalek` 2.1 - Signature verification
- `etcd-client` 0.13 - Distributed coordination
- `tokio-postgres` 0.7 - PostgreSQL client

## References

- [Plan Document](../plans/lively-sniffing-engelbart.md)
- [p2p-comm](../p2p-comm/) - Original libp2p implementation
- [rustls Documentation](https://docs.rs/rustls/)
- [TLS 1.3 RFC 8446](https://www.rfc-editor.org/rfc/rfc8446)

## License

MIT
