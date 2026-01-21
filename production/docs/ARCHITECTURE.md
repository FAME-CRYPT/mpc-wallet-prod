# System Architecture

**Version**: 0.1.0
**Last Updated**: 2026-01-20

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Component Descriptions](#component-descriptions)
3. [Data Flow](#data-flow)
4. [Byzantine Consensus Mechanism](#byzantine-consensus-mechanism)
5. [Threshold Signature Protocols](#threshold-signature-protocols)
6. [Network Topology](#network-topology)
7. [Storage Architecture](#storage-architecture)
8. [Security Model](#security-model)

## Architecture Overview

The Production MPC Threshold Wallet is a distributed system implementing threshold cryptography for secure Bitcoin transaction signing. The architecture is designed for Byzantine fault tolerance, high availability, and production-grade security.

### System Diagram

See [diagrams/system-overview.txt](diagrams/system-overview.txt) for the complete architecture diagram.

### Key Architectural Principles

1. **No Single Point of Failure**: Threshold (4-of-5) requires collaboration of multiple nodes
2. **Byzantine Fault Tolerance**: System tolerates up to 1 malicious node
3. **Defense in Depth**: Multiple security layers (network, application, cryptographic)
4. **Separation of Concerns**: Clean separation between consensus, cryptography, network, and storage
5. **Observability**: Comprehensive logging, metrics, and tracing at all layers

### High-Level Architecture

```
┌───────────────────────────────────────────────────────────────┐
│                        Client Layer                            │
│  (REST API clients, CLI, Web UI, Custom applications)         │
└───────────────────────────┬───────────────────────────────────┘
                            │ HTTP/HTTPS
┌───────────────────────────▼───────────────────────────────────┐
│                    MPC Node Cluster (5 nodes)                  │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │ Application Layer: API, Consensus, Protocols, Bitcoin   │  │
│  ├─────────────────────────────────────────────────────────┤  │
│  │ Network Layer: QUIC + mTLS P2P Mesh                     │  │
│  ├─────────────────────────────────────────────────────────┤  │
│  │ Storage Layer: PostgreSQL (persistent), etcd (state)    │  │
│  └─────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────┘
```

## Component Descriptions

The system consists of 11 Rust crates organized as a Cargo workspace:

### 1. types (crate: threshold-types)

**Purpose**: Shared type definitions used across all crates

**Key Types**:
- `NodeId`, `PeerId`, `TransactionId`: Unique identifiers
- `Vote`: Consensus vote structure with cryptographic signature
- `Transaction`: Bitcoin transaction representation
- `TransactionState`: Finite state machine states
- `ByzantineViolation`: Byzantine fault evidence
- `ByzantineViolationType`: Enumeration of fault types

**Design**:
- Zero-copy serialization with `serde`
- Strong typing prevents ID confusion
- Newtype pattern for type safety

### 2. common (crate: threshold-common)

**Purpose**: Shared utilities and helper functions

**Modules**:
- `bitcoin_tx`: Bitcoin transaction construction utilities
- `bitcoin_utils`: Address validation, script utilities
- `discovery`: Node discovery and peer management
- `observability`: Tracing and logging configuration
- `protocol`: Protocol message definitions
- `selection`: Node selection algorithms for signing
- `storage`: Storage trait abstractions

**Key Features**:
- Centralized logging configuration with `tracing`
- Bitcoin network abstraction (testnet/mainnet)
- Reusable validation logic

### 3. crypto (crate: threshold-crypto)

**Purpose**: Cryptographic primitives and key management

**Functions**:
- `sign_vote()`: Ed25519 signature generation for votes
- `verify_vote()`: Ed25519 signature verification
- `generate_keypair()`: Ed25519 keypair generation
- `hash_tx()`: Transaction hashing for signing

**Dependencies**:
- `ed25519-dalek`: Edwards-curve signatures
- `sha2`: SHA-256 hashing
- `rand`: Cryptographically secure random number generation

**Security**:
- Constant-time operations to prevent timing attacks
- No private key logging
- Secure memory zeroization

### 4. network (crate: threshold-network)

**Purpose**: QUIC-based peer-to-peer networking with mTLS

**Components**:
- `QuicEngine`: Main networking engine
- `QuicListener`: Accepts incoming connections
- `QuicSession`: Manages individual peer connections
- `PeerRegistry`: Tracks active peers

**Features**:
- Full-mesh topology (each node connects to all others)
- Mutual TLS authentication with certificate verification
- Multiplexed streams over single QUIC connection
- Automatic reconnection on failures
- Congestion control (BBR/Cubic)

**Protocol**:
- Transport: QUIC over UDP (RFC 9000)
- Encryption: TLS 1.3 with rustls
- Ports: 9000 (default, configurable)

### 5. security (crate: threshold-security)

**Purpose**: Certificate management and TLS configuration

**Components**:
- `CertManager`: Certificate loading and validation
- `TlsConfig`: rustls configuration builder
- Integration helpers for network layer

**Certificate Requirements**:
- CA certificate for signing node certificates
- Per-node certificate with unique CN
- Private keys protected with file permissions (0600)

**Validation**:
- Certificate expiration checking
- CA signature verification
- CN matching for peer authentication

### 6. storage (crate: threshold-storage)

**Purpose**: Data persistence and distributed coordination

**Components**:

#### PostgresStorage
- **Purpose**: Persistent audit logs and transaction history
- **Tables**:
  - `transactions`: Bitcoin transaction records
  - `votes`: Individual consensus votes
  - `voting_rounds`: Voting round state
  - `byzantine_violations`: Fault detection audit trail
  - `presignature_usage`: Signature pool tracking
  - `node_status`: Node health monitoring
  - `audit_log`: Immutable compliance log
- **Features**:
  - Connection pooling (deadpool)
  - Prepared statements
  - Automatic retry logic
  - Transaction isolation

#### EtcdStorage
- **Purpose**: Distributed state coordination and real-time data
- **Data Stored**:
  - Vote counts (per transaction)
  - Transaction state (finite state machine)
  - Node ban status
  - Configuration (threshold, total nodes)
  - Peer connectivity status
- **Features**:
  - Watch API for state change notifications
  - Lease-based timeouts
  - Compare-and-swap for atomic updates
  - Leader election for coordinator selection

### 7. consensus (crate: threshold-consensus)

**Purpose**: Byzantine fault-tolerant consensus implementation

**Components**:

#### VoteProcessor
- Orchestrates the complete voting process
- Receives votes from network layer
- Delegates to Byzantine detector
- Updates transaction state

#### ByzantineDetector
- **Violation Type 1: DoubleVote**
  - Same node submits conflicting votes
  - Detection: etcd stores first vote, rejects second
  - Action: Ban node, abort transaction

- **Violation Type 2: InvalidSignature**
  - Vote signature verification fails
  - Detection: Ed25519 signature check
  - Action: Ban node, reject vote

- **Violation Type 3: MinorityVote**
  - Node votes against consensus after threshold reached
  - Detection: Compare vote to majority value
  - Action: Ban node, record violation

- **Violation Type 4: Timeout**
  - Node doesn't respond within deadline
  - Detection: Network layer timeout
  - Action: Exclude from signing round

#### VoteFSM (Finite State Machine)
States:
1. `Initial`: Transaction created
2. `Collecting`: Gathering votes
3. `ThresholdReached`: Sufficient votes received
4. `Submitted`: Sent for signing
5. `Confirmed`: On Bitcoin blockchain
6. `AbortedByzantine`: Byzantine violation detected
7. `AbortedTimeout`: Consensus timeout expired

### 8. protocols (crate: threshold-protocols)

**Purpose**: Threshold signature protocol implementations

**Protocols**:

#### CGGMP24 (Threshold ECDSA for SegWit)
- **Use Case**: P2WPKH (native SegWit) addresses
- **Phases**:
  1. **Key Generation (DKG)**: Distributed key setup (one-time)
  2. **Auxiliary Info**: Additional parameters (one-time)
  3. **Presignature Generation**: Pre-computed signature material (background)
  4. **Fast Signing**: 2-round signing using presignature (150-300ms)
  5. **Standard Signing**: 5-round signing without presignature (~2s)

- **Performance**:
  - DKG: ~2.3s (one-time setup)
  - Presignature: ~1.5s (background generation)
  - Fast signing: ~180ms (with presignature pool)
  - Throughput: ~200 signatures/min (with pool)

- **Implementation**: Based on `cggmp24` crate (v0.7.0-alpha.3)

#### FROST (Threshold Schnorr for Taproot)
- **Use Case**: P2TR (Taproot) addresses with key path spending
- **Phases**:
  1. **Key Generation**: Distributed Schnorr key setup
  2. **Signing**: 2-round Schnorr signature generation

- **Performance**:
  - DKG: ~1.8s
  - Signing: ~900ms
  - Simpler protocol than ECDSA

- **Implementation**: Based on `givre` crate (v0.2)

**Protocol Selection**:
- CGGMP24: Default for maximum Bitcoin compatibility
- FROST: For Taproot-only deployments (future)

### 9. bitcoin (crate: threshold-bitcoin)

**Purpose**: Bitcoin transaction construction and blockchain interaction

**Components**:
- `TxBuilder`: Construct unsigned Bitcoin transactions
- `BitcoinClient`: Query blockchain via Esplora API
- Address generation and validation
- Fee estimation
- UTXO management

**Features**:
- Support for SegWit (P2WPKH) and Taproot (P2TR)
- Automatic UTXO selection
- Fee rate configuration
- Testnet and mainnet support

**External Dependencies**:
- Esplora API for blockchain queries
- No direct Bitcoin Core RPC required

### 10. api (crate: threshold-api)

**Purpose**: REST API server

**Framework**: Axum (Tokio-based web framework)

**Endpoints**:

Health:
- `GET /health`: Server health check

Transactions:
- `POST /api/v1/transactions`: Create transaction
- `GET /api/v1/transactions/:txid`: Get transaction status
- `GET /api/v1/transactions`: List all transactions

Wallet:
- `GET /api/v1/wallet/balance`: Get wallet balance
- `GET /api/v1/wallet/address`: Get receiving address

Cluster:
- `GET /api/v1/cluster/status`: Get cluster status
- `GET /api/v1/cluster/nodes`: List all nodes

**Middleware**:
- CORS for cross-origin requests
- Request logging with tracing
- Error handling with custom error types

**Port**: 8080 (default, configurable)

### 11. cli (crate: threshold-cli)

**Purpose**: Command-line interface for wallet operations

**Commands**:

Wallet:
- `wallet balance`: Get wallet balance
- `wallet address`: Get receiving address

Transactions:
- `send --to <address> --amount <sats>`: Send Bitcoin
- `tx status <txid>`: Get transaction status
- `tx list`: List all transactions

Cluster:
- `cluster status`: Get cluster health
- `cluster nodes`: List all nodes

DKG:
- `dkg start --protocol <cggmp24|frost> --threshold <N> --total <M>`: Start DKG
- `dkg status`: Get DKG ceremony status

Presignatures:
- `presig generate --count <N>`: Generate presignatures
- `presig list`: List available presignatures
- `presig status`: Get pool status

Config:
- `config show`: Show configuration
- `config set-endpoint <url>`: Set API endpoint

**Output Formats**:
- Table (default, colorized)
- JSON (machine-readable)

## Data Flow

### Transaction Lifecycle

See [diagrams/consensus-flow.txt](diagrams/consensus-flow.txt) for detailed consensus flow.

#### Phase 1: Creation (Client → Node)

```
Client sends: POST /api/v1/transactions
{
  "recipient": "tb1q...",
  "amount_sats": 50000
}

Node 1:
1. Validate address format
2. Check balance (query Esplora)
3. Build unsigned transaction
4. Store in PostgreSQL (state='pending')
5. Return txid to client
6. Broadcast to all nodes via QUIC
```

#### Phase 2: Consensus Voting (All Nodes)

```
Each node independently:
1. Receive transaction broadcast
2. Validate transaction (balance, address, policy)
3. Generate vote decision (approve/reject)
4. Sign vote with Ed25519 key
5. Broadcast vote to all nodes

Vote message:
{
  "tx_id": "abc123",
  "node_id": 1,
  "value": 1,  // 1=approve, 0=reject
  "signature": [...]
}
```

#### Phase 3: Byzantine Detection (Each Node)

```
For each received vote:
1. Verify Ed25519 signature
2. Check for double voting (etcd lookup)
3. Store vote in etcd
4. Increment vote count
5. Check for minority voting attack
6. Record vote in PostgreSQL
7. Update node status

If threshold reached (4 votes):
1. Mark state as ThresholdReached in etcd
2. Update PostgreSQL state to 'approved'
3. Trigger signature generation
```

#### Phase 4: Threshold Signing (4+ Nodes)

```
CGGMP24 Fast Signing (with presignature):

Coordinator (Node 1):
1. Select 4 signers (exclude banned nodes)
2. Fetch presignature from pool
3. Initiate signing protocol

Round 1 (50ms):
- Each node commits to partial signature
- Broadcast commitments

Round 2 (100ms):
- Each node reveals partial signature
- Broadcast partial signatures

Combine (30ms):
- Coordinator combines partial signatures
- Verify full signature
- Attach to transaction

Total: ~180ms with presignature
```

#### Phase 5: Broadcast to Bitcoin (Any Node)

```
Node 1:
1. Serialize signed transaction
2. Broadcast to Bitcoin network via Esplora
3. Update state to 'broadcasting'
4. Monitor for confirmations

Background polling (every 30s):
1. Query Esplora for confirmation count
2. Update PostgreSQL when confirmed
3. Update state to 'confirmed'
```

### Message Flow Patterns

#### Broadcast Pattern (Vote Distribution)

```
Node 1 → All nodes:
- Used for: Votes, transaction announcements
- Transport: QUIC multicast (parallel sends)
- Latency: Single RTT (~20-30ms)
- Reliability: Each node ACKs independently
```

#### Request-Response Pattern (Signing Protocol)

```
Coordinator → Signers → Coordinator:
- Used for: CGGMP24/FROST multi-round protocols
- Transport: QUIC bidirectional streams
- Latency: Multiple RTTs per round
- Reliability: Coordinator tracks responses, retries failures
```

## Byzantine Consensus Mechanism

### Threat Model

The system defends against:

1. **Malicious Minority**: Up to 1 Byzantine (arbitrarily malicious) node
2. **Crash Failures**: Nodes going offline unexpectedly
3. **Network Partitions**: Temporary loss of connectivity
4. **Timing Attacks**: Delayed responses to influence consensus

### Byzantine Fault Detection

#### Detection Algorithm

```rust
fn check_vote(vote: &Vote) -> Result<ByzantineCheckResult> {
    // 1. Check node ban status
    if etcd.is_banned(vote.node_id)? {
        return Err(NodeBanned);
    }

    // 2. Verify cryptographic signature
    if !verify_vote(vote)? {
        ban_node(vote.node_id, InvalidSignature);
        return Err(InvalidSignature);
    }

    // 3. Check for double voting
    if let Some(existing) = etcd.get_vote(vote.tx_id, vote.node_id)? {
        if existing.value != vote.value {
            ban_node(vote.node_id, DoubleVote);
            abort_transaction(vote.tx_id, AbortedByzantine);
            return Err(DoubleVote);
        }
    }

    // 4. Store vote and increment count
    let count = etcd.increment_vote_count(vote.tx_id, vote.value)?;
    postgres.record_vote(vote)?;

    // 5. Check for minority attack
    let all_counts = etcd.get_all_vote_counts(vote.tx_id)?;
    let (max_value, max_count) = all_counts.max_by_count();
    let threshold = config.threshold;

    if max_count >= threshold && vote.value != max_value {
        ban_node(vote.node_id, MinorityVote);
        return Err(MinorityVote);
    }

    // 6. Check if threshold reached
    if count >= threshold {
        etcd.set_state(vote.tx_id, ThresholdReached)?;
        return Ok(ThresholdReached { count });
    }

    Ok(Accepted { count })
}
```

#### Ban Mechanism

```
Node ban triggered by Byzantine violation:

1. etcd: Set banned:node_id = true (TTL: 24 hours)
2. PostgreSQL: INSERT INTO byzantine_violations
3. Broadcast ban to all nodes
4. Exclude from future signing rounds
5. Alert operators via monitoring

Auto-unban after:
- 24 hours for first violation
- 7 days for second violation
- Permanent ban after 3rd violation
```

### Consensus Safety Properties

**Safety**: Never approve conflicting transactions
- Proof: Threshold (4) > Total/2 (2.5), ensuring single majority
- Byzantine nodes (≤1) cannot create conflicting majorities

**Liveness**: Eventually approves valid transactions
- Requirement: ≥4 honest nodes online
- With 5 nodes and ≤1 Byzantine, 4 honest nodes remain

**Byzantine Tolerance**: Tolerates f = 1 Byzantine node
- Formula: threshold = ⌊2n/3⌋ + 1 = ⌊10/3⌋ + 1 = 4
- With n=5, tolerates f = (n-1)/3 = 1

## Threshold Signature Protocols

### CGGMP24 Protocol Details

#### Distributed Key Generation (DKG)

```
One-time setup to create threshold key shares:

Round 1: Commitment Phase
- Each node generates random polynomial
- Commits to polynomial coefficients
- Broadcasts commitments

Round 2: Share Distribution
- Each node evaluates polynomial at other nodes' IDs
- Encrypts and sends shares
- Broadcasts proof of correct sharing

Round 3: Share Verification
- Each node verifies received shares
- Computes public key share
- Broadcasts complaints if verification fails

Output:
- Each node has: secret key share, public key share
- Aggregate public key known to all
- No single node knows complete private key
```

#### Presignature Generation

```
Background process to pre-compute signature material:

Round 1-3: Similar to full signing rounds 1-3
- Generate ephemeral keys and commitments
- Share proofs and partial computations

Output (stored in pool):
- Presignature material valid for one future signature
- Includes: nonce, R value, Lagrange coefficients
- Size: ~1KB per presignature

Pool management:
- Target size: 50 presignatures
- Generate when pool < 20
- Batch generation: 10 at a time
- Background thread runs every 5 minutes
```

#### Fast Signing (with Presignature)

```
2-round protocol using pre-computed presignature:

Round 1 (80ms):
- Coordinator selects presignature from pool
- Each signer computes partial signature using:
  - Message hash
  - Private key share
  - Presignature material
- Broadcast partial signature

Round 2 (70ms):
- Coordinator collects ≥threshold partial signatures
- Combines using Lagrange interpolation
- Verifies complete signature
- Deletes used presignature from pool

Total latency: ~150-180ms
```

### FROST Protocol Details

#### Key Generation

```
Simpler than CGGMP24 (Schnorr vs ECDSA):

Round 1:
- Each node generates secret polynomial
- Computes and broadcasts commitments

Round 2:
- Distribute shares to other nodes
- Verify received shares

Output:
- Schnorr key shares
- Aggregate public key (X coordinate only)
```

#### Signing

```
2-round Schnorr threshold signature:

Round 1: Commitment
- Each signer generates random nonce
- Computes commitment R_i = r_i * G
- Broadcasts commitment

Round 2: Response
- All compute R = Σ R_i
- Each computes partial signature:
  s_i = r_i + H(R || m || X) * secret_share_i
- Broadcast s_i

Combine:
- Compute s = Σ s_i
- Signature = (R, s)
- Verify: s*G = R + H(R||m||X)*X

Total latency: ~800-900ms
```

## Network Topology

See [diagrams/network-topology.txt](diagrams/network-topology.txt) for detailed network architecture.

### Full Mesh P2P Network

```
Topology: Complete graph K₅
- Each node connects to all others
- Total connections: 10 bidirectional
- Per node: 4 outbound + 4 inbound
```

### QUIC Transport Characteristics

- **Protocol**: QUIC (RFC 9000) over UDP
- **Encryption**: TLS 1.3 with AES-256-GCM or ChaCha20-Poly1305
- **Streams**: Multiplexed bidirectional streams per connection
- **Congestion Control**: BBR or Cubic (adaptive)
- **0-RTT**: Connection resumption for reduced latency
- **MTU**: 1500 bytes (Ethernet standard)

### mTLS Authentication Flow

```
1. TCP handshake (if needed)
2. QUIC + TLS 1.3 handshake:
   - Client sends: supported ciphers, client cert
   - Server sends: server cert, requests client cert
   - Mutual verification:
     * Check certificate signed by CA
     * Verify CN matches expected peer
     * Check expiration date
3. Encrypted stream established
```

## Storage Architecture

### PostgreSQL Schema

Primary tables:

```sql
-- Transaction records
transactions (
  id BIGSERIAL PRIMARY KEY,
  txid TEXT UNIQUE,
  state transaction_state,  -- enum
  unsigned_tx BYTEA,
  signed_tx BYTEA,
  recipient TEXT,
  amount_sats BIGINT,
  fee_sats BIGINT,
  metadata TEXT,
  created_at TIMESTAMPTZ,
  confirmed_at TIMESTAMPTZ,
  bitcoin_txid TEXT
)

-- Consensus votes
votes (
  id BIGSERIAL PRIMARY KEY,
  round_id BIGINT REFERENCES voting_rounds(id),
  node_id BIGINT,
  tx_id TEXT,
  approve BOOLEAN,
  value BIGINT,
  signature BYTEA,
  received_at TIMESTAMPTZ,
  UNIQUE(round_id, node_id)
)

-- Byzantine violations (audit trail)
byzantine_violations (
  id BIGSERIAL PRIMARY KEY,
  node_id BIGINT,
  violation_type violation_type,  -- enum
  tx_id TEXT,
  evidence JSONB,
  detected_at TIMESTAMPTZ,
  action_taken TEXT
)
```

### etcd Key-Value Structure

```
Namespaces:

/config/
  threshold: 4
  total_nodes: 5
  bitcoin_network: testnet

/tx/{txid}/
  state: ThresholdReached
  votes/approve: 4
  votes/reject: 0

/votes/{txid}/{node_id}:
  {
    "value": 1,
    "timestamp": "2026-01-20T...",
    "signature": "..."
  }

/bans/{node_id}:
  true  (TTL: 24 hours)

/peers/:
  {
    "node1": {"last_seen": "...", "status": "online"},
    "node2": {"last_seen": "...", "status": "online"}
  }
```

### Data Consistency Model

**PostgreSQL**:
- ACID guarantees for transactions
- Read Committed isolation level
- Write-ahead logging for durability

**etcd**:
- Linearizable reads and writes (Raft consensus)
- Watch API for real-time notifications
- Lease-based distributed locks

**Combined Model**:
- etcd for real-time state (votes, locks, bans)
- PostgreSQL for historical audit trail
- Eventual consistency between systems acceptable
  (etcd is source of truth for current state)

## Security Model

### Trust Boundaries

**Zone 1: Untrusted (Internet)**
- Client applications
- Bitcoin network
- Esplora API

**Zone 2: Semi-Trusted (MPC Cluster)**
- MPC nodes (up to 1 may be Byzantine)
- mTLS authenticated
- Byzantine detection active

**Zone 3: Trusted (Infrastructure)**
- PostgreSQL
- etcd cluster
- Internal network only

### Cryptographic Guarantees

1. **Key Security**: Private key distributed across ≥4 nodes, no single node compromise breaks security
2. **Vote Integrity**: Ed25519 signatures prevent vote forgery
3. **Network Security**: mTLS prevents MITM and eavesdropping
4. **Byzantine Resistance**: Threshold majority prevents malicious minority

### Attack Resistance

| Attack | Mitigation |
|--------|-----------|
| Key theft | Threshold (need 4+ nodes) |
| Double voting | etcd detection, automatic ban |
| Signature forgery | Cryptographic verification |
| MITM | mTLS with certificate pinning |
| DoS | Rate limiting, resource limits |
| Network partition | etcd Raft quorum requirement |
| Replay attacks | Nonce in signatures |
| Timing attacks | Constant-time crypto operations |

---

**Document Version**: 1.0
**Architecture Status**: Production-ready
**Last Review**: 2026-01-20
