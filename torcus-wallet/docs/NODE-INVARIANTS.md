# Node-Side Invariants

This document defines the invariants that MPC nodes must enforce regardless of transport (coordinator-relayed or future P2P). These invariants ensure that the coordinator remains non-authoritative and that nodes maintain security guarantees independently.

---

## 1. Grant Verification

### 1.1 Signature Verification
Every signing request MUST include a valid `SigningGrant`. Nodes MUST:

- Verify the Ed25519 signature against the grant verifying key
- Reject requests with invalid or missing signatures
- Fail to start if the grant verifying key cannot be fetched from a trusted source

**Implementation:** `crates/node/src/state.rs:verify_grant()`

### 1.2 Grant Expiry
Nodes MUST check `expires_at` against current time:

```
if current_time > grant.expires_at {
    reject("Grant expired")
}
```

**Default validity:** 5 minutes (`DEFAULT_GRANT_VALIDITY_SECS = 300`)

### 1.3 Participant Membership
Nodes MUST verify they are listed in `grant.participants`:

```
if !grant.participants.contains(my_party_index) {
    reject("Not a participant in this grant")
}
```

### 1.4 Duplicate Participant Check
Nodes MUST reject grants with duplicate participant indices to prevent share contribution manipulation.

### 1.5 Request-Grant Consistency
Nodes MUST verify that request parameters match the grant:

| Field | Validation |
|-------|------------|
| `wallet_id` | `request.wallet_id == grant.wallet_id` |
| `message_hash` | `request.message_hash == grant.message_hash` |
| `participants` | `sorted(request.parties) == grant.participants` (canonical order) |

### 1.6 Canonical Participant Order
Grant participants are stored in sorted (canonical) order. Nodes MUST use `grant.participants` for protocol execution, not `request.parties`, to ensure the signed authorization matches the actual protocol roles.

---

## 2. Replay and Idempotency

### 2.1 Session Uniqueness
Each grant produces a deterministic session ID:

```
session_id = hash(grant_id || nonce)
```

Nodes SHOULD track active/completed session IDs to detect:
- Duplicate session initiation attempts
- Replay attacks using old grants

### 2.2 Grant Nonce
Each grant contains a random `nonce` (u64). Combined with `grant_id`, this ensures uniqueness even for identical signing parameters.

### 2.3 Replay Cache (Recommended)
Nodes SHOULD maintain a cache of recently seen `grant_id` values:

```rust
struct ReplayCache {
    seen_grants: HashMap<Uuid, u64>,  // grant_id -> expires_at
    max_entries: usize,
}
```

Reject if `grant_id` was already processed and hasn't expired.

### 2.4 Idempotency Behavior
If a node receives a signing request for an already-completed session:
- Return the cached signature (if available)
- OR reject with "session already completed"

Do NOT re-execute the signing protocol.

---

## 3. Session State Management

### 3.1 Session States
```
┌─────────┐    start    ┌────────────┐   complete   ┌───────────┐
│ (none)  │ ──────────> │ in_progress│ ───────────> │ completed │
└─────────┘             └────────────┘              └───────────┘
                              │
                              │ timeout/error
                              v
                        ┌──────────┐
                        │  failed  │
                        └──────────┘
```

### 3.2 Session Timeout
Nodes MUST enforce session timeouts to prevent resource exhaustion:

| Phase | Recommended Timeout |
|-------|---------------------|
| Total session | 120 seconds |
| Per-round | 30 seconds |
| Idle (no messages) | 60 seconds |

### 3.3 Concurrent Session Limits
Nodes SHOULD limit concurrent active sessions per wallet and globally:

```rust
const MAX_SESSIONS_PER_WALLET: usize = 3;
const MAX_TOTAL_SESSIONS: usize = 10;
```

### 3.4 Session Cleanup
Nodes MUST clean up session state after:
- Successful completion
- Timeout expiry
- Explicit abort

Retain minimal audit data (session_id, grant_id, result, timestamp) for observability.

---

## 4. Protocol Message Validation

### 4.1 Message Authentication
All protocol round messages MUST be validated:
- `session_id` matches active session
- `from` party is a valid participant
- `round` is expected (no skipping, no regression)
- Message format is valid for the protocol

### 4.2 Message Ordering
Nodes MUST handle out-of-order message delivery gracefully:
- Buffer early messages for future rounds
- Discard messages for past rounds
- Apply per-round deduplication

### 4.3 Byzantine Behavior Detection
Nodes SHOULD detect and log:
- Duplicate messages from same party for same round
- Invalid cryptographic proofs
- Protocol-specific equivocation

---

## 5. Threshold Enforcement

### 5.1 Minimum Participants
Nodes MUST verify:

```
grant.participants.len() >= grant.threshold
```

### 5.2 Share Availability
Before participating, nodes MUST verify they have the required key share for the wallet.

### 5.3 Protocol-Specific Checks
- **CGGMP24:** Verify aux_info is available and matches key share
- **FROST:** Verify key share is valid for the signing group

---

## 6. Failure Handling

### 6.1 Fail-Safe Defaults
When in doubt, nodes MUST fail closed:
- Missing grant → reject
- Invalid signature → reject
- Expired grant → reject
- Unknown session → reject
- Missing key share → reject

### 6.2 Error Reporting
Nodes SHOULD return structured errors:

```rust
enum SigningError {
    GrantMissing,
    GrantInvalid { reason: String },
    GrantExpired { expired_at: u64 },
    NotParticipant { party_index: u16 },
    SessionNotFound { session_id: String },
    KeyShareMissing { wallet_id: String },
    ProtocolError { round: u16, details: String },
    Timeout { phase: String },
}
```

### 6.3 Audit Logging
All grant validations and session state transitions MUST be logged with:
- Timestamp
- Session ID
- Grant ID
- Party index
- Result (success/failure)
- Error details (if failed)

---

## 7. Security Boundaries

### 7.1 Trust Model
Nodes trust ONLY:
1. Their local key share storage
2. The grant verifying key (fetched at startup)
3. Cryptographic proofs in protocol messages

Nodes do NOT trust:
- The coordinator (for authorization)
- Other nodes (beyond protocol guarantees)
- Request parameters (must match grant)

### 7.2 Key Material Protection
- Key shares MUST be stored encrypted at rest
- Key shares MUST NOT be logged or included in error messages
- Memory containing key material SHOULD be zeroed after use

### 7.3 Side-Channel Considerations
- Use constant-time comparisons for signatures and hashes
- Avoid branching on secret data
- Consider timing attack mitigations for cryptographic operations

---

## 8. Implementation Checklist

### Currently Implemented ✅
- [x] Grant signature verification
- [x] Grant expiry check
- [x] Participant membership check
- [x] Duplicate participant detection
- [x] Request-grant consistency (wallet_id, message_hash, participants)
- [x] Canonical participant ordering
- [x] Deterministic session ID from grant
- [x] Fail if grant verifying key not available
- [x] Replay cache for grant_id deduplication (`session.rs:replay_cache`)
- [x] Session state tracking (in_progress, completed, failed) (`session.rs:SessionState`)
- [x] Session timeout enforcement (`SESSION_TIMEOUT_SECS = 120`)
- [x] Concurrent session limits (`MAX_SESSIONS_PER_WALLET = 3`, `MAX_TOTAL_SESSIONS = 10`)
- [x] Structured error responses (`session.rs:SessionError`)
- [x] Session cleanup task (background task every 30s)
- [x] Session retention timing (based on completion time, not creation)
- [x] Audit logging (LogEvents emitted for grant validation, session lifecycle, messages)
- [x] Metrics collection (ProtocolMetrics for sessions, grants, message throughput)

### Partially Implemented ⚠️
- [~] Per-round message tracking (RoundState infrastructure exists in `session.rs`, but protocol
      messages always set `round: 0` - actual round info is encoded in message payload by `round_based` crate)
- [~] Out-of-order message buffering (logic in `record_message()` exists but is ineffective without
      real round numbers; session-level timeout still protects against stalled sessions)
- [~] Per-round timeout enforcement (infrastructure exists but requires actual round extraction
      from protocol-specific message types)

---

## References

- `crates/common/src/grant.rs` - Grant definition and validation
- `crates/node/src/state.rs` - Node state and grant verification
- `crates/node/src/handlers/cggmp24.rs` - CGGMP24 signing handler
- `crates/node/src/handlers/frost.rs` - FROST signing handler
- `docs/COORDINATOR-NODE-ARCHITECTURE.md` - Architecture overview
