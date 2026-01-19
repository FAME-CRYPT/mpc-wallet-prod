# P2P Migration Roadmap

This document outlines the migration path from the current coordinator-relay architecture to a full P2P node coordination model. The migration is designed to be incremental, with each phase providing standalone value while building toward the final P2P goal.

## Current State: Hybrid Coordinator Model

The current architecture uses a **hybrid coordinator model**:

- **Coordinator**: Orchestrates protocol initiation, relays messages between nodes, aggregates results
- **Nodes**: Execute MPC protocols, verify grants, enforce session invariants
- **Transport**: HTTP-based relay through coordinator

Key properties already in place:
- Nodes are **authoritative** for authorization (grant verification)
- Protocol logic is **transport-agnostic** (via `Transport` trait)
- Session IDs are **deterministic** (derived from grants)
- Participant selection is **deterministic** (hash-based)

## Migration Phases

### Phase 1: Transport Abstraction (COMPLETE)

**Goal**: Decouple protocol logic from transport mechanism.

**Deliverables**:
- [x] `Transport` trait in `protocols` crate
- [x] `HttpTransport` implementation (coordinator relay)
- [x] `TransportConfig` for runtime configuration
- [x] Backward-compatible `RelayClient` wrapper
- [x] Node state includes transport configuration

**Status**: Complete. Nodes can be configured with different transport backends without changing protocol code.

### Phase 2: Node Discovery & Health (COMPLETE)

**Goal**: Nodes can discover and monitor each other without coordinator.

**Deliverables**:
- [x] Node registry service (coordinator-hosted endpoint)
- [x] Heartbeat protocol between nodes
- [x] Health score calculation (latency, reliability, uptime)
- [x] Node capability advertisement (supported protocols, key shares)

**Implementation Details**:
- `crates/common/src/discovery.rs`: Core types (NodeInfo, NodeCapabilities, NodeHealthMetrics)
- `crates/coordinator/src/registry.rs`: NodeRegistry with registration, heartbeat, cleanup
- `crates/coordinator/src/registry_handlers.rs`: HTTP endpoints for registry API
- `crates/coordinator/src/health_checker.rs`: Background task for health monitoring
- `crates/node/src/registration.rs`: Node-side registration and heartbeat client

**API Endpoints**:
- `POST /registry/register`: Node self-registration
- `POST /registry/heartbeat`: Periodic heartbeat from nodes
- `GET /registry/nodes`: List all registered nodes
- `GET /registry/node/{party_index}`: Get specific node info
- `GET /registry/stats`: Registry statistics

**Health Score Algorithm**:
```
health_score = 0.4 * reliability + 0.3 * latency_score + 0.3 * uptime_score
```

**Status**: Complete. Nodes register on startup and send periodic heartbeats. Coordinator tracks health metrics and marks stale nodes offline.

### Phase 3: Direct P2P Messaging (COMPLETE)

**Goal**: Nodes can send messages directly without coordinator relay.

**Deliverables**:
- [x] `P2pTransport` implementation of `Transport` trait
- [x] Connection management (TCP connections to peers)
- [x] Message framing protocol (length-prefixed bincode)
- [x] Receiver task for incoming connections
- [ ] Message authentication (node-to-node signatures) - Future enhancement
- [ ] NAT traversal support (STUN/TURN) - Not needed in Docker environment

**Implementation Details**:
- `crates/protocols/src/p2p/error.rs`: P2P-specific error types
- `crates/protocols/src/p2p/protocol.rs`: Wire protocol (length-prefixed frames)
- `crates/protocols/src/p2p/connection.rs`: TCP connection manager with peer discovery
- `crates/protocols/src/p2p/receiver.rs`: Background task for incoming messages
- `crates/protocols/src/p2p/transport.rs`: P2pTransport implementing Transport trait

**Architecture**:
```
┌─────────────────────────────────────────────────────────────┐
│                        Node                                  │
│  ┌─────────────┐    ┌──────────────┐    ┌────────────────┐  │
│  │  Protocol   │───>│ P2pTransport │───>│ Connection     │──┼──> Peers
│  │             │<───│              │<───│   Manager      │  │
│  └─────────────┘    └──────────────┘    └────────────────┘  │
│                            │                    ↑            │
│                    ┌───────▼────────┐   ┌──────┴──────┐     │
│                    │ Incoming Queue │<──│   Receiver  │     │
│                    │  (per-session) │   │    Task     │     │
│                    └────────────────┘   └─────────────┘     │
└─────────────────────────────────────────────────────────────┘
```

**Design Choice**: Simple TCP instead of libp2p
- Docker environment has direct connectivity (no NAT issues)
- Uses Phase 2 node registry for peer discovery
- Can upgrade to libp2p/QUIC later if needed

**Status**: Complete. P2pTransport implements the Transport trait for direct messaging. Nodes can communicate without coordinator relay.

### Phase 3b: QUIC Transport with TLS (COMPLETE)

**Goal**: Secure P2P messaging using QUIC with TLS 1.3 and CA-signed certificates.

**Motivation**:
- **Security**: TLS 1.3 mandatory in QUIC (no unencrypted communication possible)
- **Mutual Authentication**: CA-signed certificates verify node identity
- **Performance**: QUIC's multiplexing and 0-RTT reduces latency
- **Reliability**: Built-in congestion control and stream management

**Deliverables**:
- [x] CA certificate generation and management
- [x] Node certificate signing (CA → Node certs)
- [x] QUIC connection manager with mTLS
- [x] QUIC listener for incoming connections
- [x] QuicTransport implementing Transport trait
- [x] Certificate storage/loading utilities

**Implementation Details**:
- `crates/protocols/src/p2p/certs.rs`: CA and node certificate management
- `crates/protocols/src/p2p/quic.rs`: QUIC connection manager
- `crates/protocols/src/p2p/quic_listener.rs`: Incoming connection handler
- `crates/protocols/src/p2p/quic_transport.rs`: QuicTransport implementing Transport trait

**Certificate Hierarchy**:
```
Root CA (self-signed)
    │
    ├── Node 0 Certificate (signed by CA)
    ├── Node 1 Certificate (signed by CA)
    ├── Node 2 Certificate (signed by CA)
    └── Node 3 Certificate (signed by CA)
```

**Security Model**:
- All connections use TLS 1.3 (mandatory in QUIC)
- Mutual TLS: both client and server present CA-signed certificates
- Each node's certificate contains party index in subject for identification
- Certificate chain validation ensures only nodes from same CA can communicate

**Architecture**:
```
┌─────────────────────────────────────────────────────────────┐
│                        Node                                  │
│  ┌─────────────┐    ┌──────────────┐    ┌────────────────┐  │
│  │  Protocol   │───>│QuicTransport │───>│ QUIC Connection│──┼──> Peers
│  │             │<───│   (TLS 1.3)  │<───│    Manager     │  │
│  └─────────────┘    └──────────────┘    └────────────────┘  │
│                            │                    ↑            │
│                    ┌───────▼────────┐   ┌──────┴──────┐     │
│                    │ Incoming Queue │<──│QuicListener │     │
│                    │  (per-session) │   │(mTLS auth)  │     │
│                    └────────────────┘   └─────────────┘     │
└─────────────────────────────────────────────────────────────┘
```

**Usage**:
```rust
use protocols::p2p::{QuicTransportBuilder, CaCertificate};

// Generate or load certificates
let ca = CaCertificate::generate()?;
let node_cert = ca.sign_node_cert(party_index, &["mpc-node-1".to_string()])?;

// Create QUIC transport
let transport = QuicTransportBuilder::new(party_index, "http://mpc-coordinator:3000")
    .with_cert(node_cert)
    .with_listen_addr("0.0.0.0:4001".parse()?)
    .build()?;

// Initialize and use
transport.init().await?;
transport.refresh_peers().await?;
transport.send_message(msg).await?;
```

**Design Choices**:
- QUIC over libp2p: Smaller attack surface, well-audited quinn crate
- Unidirectional streams: One stream per message for simplicity
- CA-signed certs: Enables trust hierarchy and revocation

**Status**: Complete. QuicTransport provides secure P2P messaging with mutual TLS authentication.

### Phase 4: Leader Election & Initiator Selection (COMPLETE)

**Goal**: Any node can initiate signing sessions without coordinator in the critical path.

**Approach**: Coordinator issues grants, nodes coordinate sessions via P2P.

**Deliverables**:
- [x] Deterministic initiator selection (hash-based from grant)
- [x] Grant issuance endpoint on coordinator (`POST /grant/signing`)
- [x] P2P session control messages (Proposal, Ack, Start, Abort)
- [x] P2P session coordinator for session lifecycle management
- [x] P2P signing endpoint on node (`POST /p2p/sign`)
- [x] Unified P2P message types for protocol and control messages

**Implementation Details**:
- `crates/common/src/selection.rs`: Added `select_initiator()` function
- `crates/coordinator/src/grant_handlers.rs`: Grant issuance endpoints
- `crates/protocols/src/p2p/session.rs`: Session control message types
- `crates/protocols/src/p2p/coordinator.rs`: P2pSessionCoordinator
- `crates/node/src/handlers/p2p.rs`: P2P signing endpoint

**Initiator Selection Algorithm**:
```rust
use common::selection::select_initiator_from_grant;

// Deterministic initiator based on grant hash
let initiator = select_initiator_from_grant(&grant);
// Uses SHA256(b"torcus-initiator:" || grant_id || nonce) to select
```

**New Flow**:
```
BEFORE (Phases 1-3):
CLI → Coordinator (build tx + issue grant + notify nodes + relay messages) → Nodes

AFTER (Phase 4):
CLI → Coordinator (issue grant only) → CLI → Any Node (initiate P2P session) → Nodes (P2P)
```

**Session Control Messages**:
```rust
pub enum SessionControlMessage {
    Proposal(SessionProposal),  // Initiator → Participants
    Ack(SessionAck),            // Participant → Initiator
    Start(SessionStart),        // Initiator → All (threshold reached)
    Abort(SessionAbort),        // Any → All (error occurred)
}
```

**Architecture**:
```
┌─────────────────────────────────────────────────────────────────────┐
│                           Phase 4 Architecture                       │
│                                                                       │
│  ┌─────────┐      ┌──────────────┐                                  │
│  │   CLI   │─────>│  Coordinator │  (1) Request grant               │
│  └─────────┘      │  (grant only)│                                  │
│       │           └──────────────┘                                  │
│       │                                                              │
│       │  (2) Send grant + signing request                           │
│       ▼                                                              │
│  ┌─────────┐                                                         │
│  │ Node 0  │◄────────────────────────────────────────┐              │
│  │(initiator)                                         │              │
│  └─────────┘                                          │              │
│       │                                               │              │
│       │ (3) SessionProposal (grant + participants)   │              │
│       ▼                                               │              │
│  ┌─────────┐    ┌─────────┐    ┌─────────┐          │              │
│  │ Node 1  │◄──►│ Node 2  │◄──►│ Node 3  │          │              │
│  └─────────┘    └─────────┘    └─────────┘          │              │
│       │              │              │                │              │
│       └──────────────┴──────────────┴────────────────┘              │
│                    (4) P2P Protocol Messages                         │
│                    (5) Signature returned to initiator              │
└─────────────────────────────────────────────────────────────────────┘
```

**API Endpoints**:
- `POST /grant/signing`: Issue signing grant (coordinator)
- `POST /grant/keygen`: Issue keygen grant (coordinator)
- `POST /p2p/sign`: Initiate P2P signing session (node)
- `GET /p2p/session/:id`: Get P2P session status (node)

**Current Mode**: Hybrid - P2P endpoint validates grants and redirects to correct initiator, 
but falls back to HTTP relay for protocol execution until full P2P transport integration.

**Status**: Complete. Infrastructure for P2P session coordination is in place. Nodes can receive 
grants directly and determine initiator. Full P2P protocol execution requires transport integration.

### Phase 5: Coordinator Deprecation

**Goal**: Coordinator becomes optional, only needed for external API.

**Deliverables**:
- [ ] CLI can connect directly to nodes for operations
- [ ] Nodes can form quorum and sign without coordinator
- [ ] Coordinator becomes thin API proxy (optional)
- [ ] Full observability through node metrics aggregation

**Architecture After Phase 5**:
```
External Client
      |
      v
+------------------+
|  API Gateway     |  (optional, can be any node)
+------------------+
      |
      v
+------+------+------+------+
| Node | Node | Node | Node |
|  0   |  1   |  2   |  3   |
+------+------+------+------+
   |      |      |      |
   +------+------+------+
         P2P Mesh
```

## Policy Engine & Grant Authorization

The Grant system (`crates/common/src/grant.rs`) provides cryptographic authorization for signing operations. Nodes **always** verify grants before participating in signing sessions.

### Current Implementation

```
CLI Request → Coordinator → Policy Check → Grant Issued → Nodes Verify → Sign
```

- **Grant Issuer**: Currently the coordinator (via `grant_signing_key`)
- **Grant Verification**: Each node validates signature, expiry, and participant membership
- **Session ID**: Deterministically derived from grant (enables replay protection)

### Future: Dedicated Policy Engine

In future versions, a dedicated **Policy Engine** will evaluate signing requests against configurable policies before issuing grants:

```
                                 ┌─────────────────┐
CLI Request ──────────────────>  │  Policy Engine  │
                                 │                 │
                                 │  - Rate limits  │
                                 │  - Spend limits │
                                 │  - Whitelists   │
                                 │  - Multi-sig    │
                                 │  - Time locks   │
                                 └────────┬────────┘
                                          │
                                     Grant Issued
                                          │
                                          v
                              ┌───────────────────────┐
                              │   Nodes (P2P Mesh)    │
                              │                       │
                              │  Verify Grant → Sign  │
                              └───────────────────────┘
```

**Key Design Principle**: Grant verification on nodes remains unchanged regardless of who issues grants. The Policy Engine simply replaces the coordinator as the grant issuer.

### P2P Implications

In full P2P mode (Phase 5), grant issuance options:

1. **External Policy Engine**: Separate service that clients contact for grants
2. **Embedded Policy**: Policy logic runs on nodes, quorum needed to issue grants
3. **Hybrid**: Simple policies on nodes, complex policies on external engine

The Grant system's design (Ed25519 signatures, deterministic session IDs) works with any of these approaches.

## Security Considerations

### DoS Mitigation

P2P networks require robust DoS protection:

1. **Rate Limiting**: Per-peer message rate limits
2. **Proof of Work**: Optional PoW for session initiation
3. **Reputation System**: Track peer behavior, penalize bad actors
4. **Connection Limits**: Maximum concurrent connections per peer

### Message Authentication

All P2P messages must be authenticated:

```rust
struct AuthenticatedMessage {
    /// The actual protocol message
    payload: Vec<u8>,
    /// Sender's node ID
    sender: NodeId,
    /// Ed25519 signature over (payload || recipient || timestamp)
    signature: [u8; 64],
    /// Timestamp to prevent replay
    timestamp: u64,
}
```

### Grant Verification

Grant verification remains unchanged - nodes always verify grants regardless of transport:

- Grant signature verification (coordinator's Ed25519 key)
- Participant membership check
- Expiry validation
- Replay detection via session manager

## Implementation Timeline

| Phase | Estimated Effort | Dependencies | Status |
|-------|-----------------|--------------|--------|
| Phase 1 | Complete | - | ✅ Complete |
| Phase 2 | Complete | Phase 1 | ✅ Complete |
| Phase 3 | Complete | Phase 2 | ✅ Complete |
| Phase 3b | Complete | Phase 3 | ✅ Complete |
| Phase 4 | 3-4 weeks | Phase 3/3b | Pending |
| Phase 5 | 2-3 weeks | Phase 4 | Pending |

**Remaining**: ~5-7 weeks for full P2P migration (Phases 4-5).

## Rollback Strategy

Each phase is designed to be reversible:

- **Phase 2**: Disable node-to-node heartbeats, revert to coordinator health checks
- **Phase 3**: Fall back to `HttpTransport` if TCP P2P fails
- **Phase 3b**: Fall back to TCP P2P if QUIC fails, or to HTTP if both fail
- **Phase 4**: Keep coordinator as initiator fallback
- **Phase 5**: Re-enable coordinator if needed

Configuration flag for transport selection:
```toml
[transport]
type = "http"  # or "p2p" or "quic" or "hybrid"
coordinator_url = "http://coordinator:3000"
p2p_listen = "0.0.0.0:4000"    # TCP P2P port
quic_listen = "0.0.0.0:4001"   # QUIC port
```

## Metrics & Observability

Track migration progress with these metrics:

- `p2p_messages_sent` / `relay_messages_sent`: Ratio shows P2P adoption
- `p2p_connection_failures`: Monitor P2P reliability
- `session_initiator_type`: coordinator vs. node-initiated
- `coordinator_dependency_calls`: Should decrease over phases

## Testing Strategy

1. **Unit Tests**: Each transport implementation independently
2. **Integration Tests**: Mixed transport scenarios
3. **Chaos Testing**: Network partitions, node failures
4. **Load Testing**: P2P performance under high message volume
5. **Security Testing**: Attempt unauthorized session initiation

## Open Questions

1. **NAT Traversal**: What percentage of deployments need NAT traversal?
2. **Node Identity**: Use existing Ed25519 keys or separate P2P identity?
3. **Session Recovery**: How to handle in-flight sessions during P2P failures?
4. **Backward Compatibility**: How long to support coordinator-only mode?

## Appendix: Transport Trait (Current Implementation)

```rust
#[async_trait]
pub trait Transport: Send + Sync {
    fn party_index(&self) -> u16;
    fn transport_type(&self) -> TransportType;
    
    async fn send_message(&self, msg: RelayMessage) -> Result<(), TransportError>;
    async fn poll_messages(&self, session_id: &str) -> Result<PollMessagesResponse, TransportError>;
    async fn notify_aux_info_complete(...) -> Result<(), TransportError>;
}

pub enum TransportType {
    Http,      // Coordinator relay
    P2p,       // Direct peer messaging (TCP or QUIC)
    Hybrid,    // P2P with HTTP fallback
    InMemory,  // Testing: in-process messaging
}
```

**Available Transport Implementations**:
- `HttpTransport`: Coordinator relay (production default)
- `P2pTransport`: Direct TCP messaging between nodes
- `QuicTransport`: Secure QUIC messaging with mTLS (recommended for P2P)

The `Transport` trait provides the abstraction that enables seamless switching between transport backends.
