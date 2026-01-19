# MPC Threshold Wallet Architecture Comparison
## Coordinator-Orchestrated vs Fully P2P Node Coordination

This document compares two production-minded architectures for running an MPC threshold wallet (e.g., threshold ECDSA / FROST / CGGMP-family schemes) in Rust. It focuses on design tradeoffs, security, operations, failure modes, and decision criteria.

---

## Table of Contents
1. [The two architectures at a glance](#1-the-two-architectures-at-a-glance)  
2. [Core question: who is the “leader” of a signing session?](#2-core-question-who-is-the-leader-of-a-signing-session)  
3. [Design sketches](#3-design-sketches)  
4. [Pros and cons](#4-pros-and-cons)  
5. [Security analysis criteria](#5-security-analysis-criteria)  
6. [Operational criteria](#6-operational-criteria)  
7. [Performance and latency](#7-performance-and-latency-criteria)  
8. [Decision criteria checklist](#8-decision-criteria-checklist)  
9. [Recommended hybrid](#9-recommended-hybrid-often-best-in-practice)  
10. [Summary](#10-summary-is-the-coordinator-unnecessary)  
11. [Implementation-plan seed](#11-implementation-plan-seed-what-to-build-first)

---

## 1. The two architectures at a glance

### A) Coordinator-Orchestrated MPC

A dedicated service (the coordinator) runs MPC sessions: selects participants, drives protocol rounds, routes messages, handles timeouts/retries, and returns a final signature.

**Sketch**
```text
User/CLI
  |
API Gateway (auth, rate limit, TLS)
  |
Policy Engine (Raft)  -- produces "approval/grant" artifact
  |
Coordinator (session orchestration, routing)
  |
MPC Nodes (key shares, crypto rounds)
```

### B) Fully P2P Node-Coordinated MPC

No coordinator. Nodes discover each other and run MPC sessions among themselves. An external layer submits an approved signing job; nodes coordinate execution and return the result.

**Sketch**
```text
User/CLI
  |
API Gateway
  |
Policy Engine (Raft)  -- produces "approval/grant" artifact
  |
Job topic / request channel (queue, pubsub, gossipsub, etc.)
  |
MPC Nodes <-----> MPC Nodes (P2P orchestration + crypto)
        \-----> Result channel back to policy/gateway
```

---

## 2. Core question: who is the “leader” of a signing session?

MPC signing is a multi-round distributed protocol. Someone has to do the unglamorous work:

- create/track session IDs
- choose participants (or accept a quorum)
- route protocol messages to the right party at the right round
- apply timeouts + retries
- deal with node dropouts and byzantine behavior
- finalize + publish the signature/result

**Coordinator model:** the coordinator is the explicit leader (at least for orchestration).  
**P2P model:** nodes elect a leader per session, or use a leaderless protocol (more complexity).

---

## 3. Design sketches

### A) Coordinator-Orchestrated Design Sketch

#### Main components
- **Coordinator:** stateless-ish orchestrator with a session store (in-memory + optional persistence)
- **Node registry:** mapping `wallet/key -> eligible nodes`
- **Node client:** transport abstraction (HTTP/gRPC/libp2p)
- **Session manager:** state machine  
  `created -> round1 -> round2 -> ... -> finalized -> done/failed`

#### Session flow
1. Policy Engine issues a signed **grant**:  
   `(wallet_id, key_id, message_digest/tx, threshold, participants, expiry, nonce, grant_id)`
2. Coordinator validates the grant, assigns `session_id`, chooses participants if not fixed.
3. Coordinator contacts nodes: `start_session(session_id, grant, ...)`.
4. Coordinator drives rounds: collects messages and forwards to peers.
5. Coordinator finalizes signature and returns it  
   *(or nodes finalize and report back; coordinator still orchestrates).*

#### Key security principle
- Coordinator must be **non-authoritative**: nodes won’t sign without a valid grant.

---

### B) Fully P2P Node-Coordinated Design Sketch

#### Main components
- **Job distribution:** queue/topic or direct request to one node that then spreads it (e.g., gossipsub)
- **Membership & discovery:** static config or discovery protocol
- **Leader election / initiator:** per-session initiator (first node that sees the job) or deterministic leader (lowest ID, hash-based, etc.)
- **Distributed session store:** each node maintains local session state; relies on session uniqueness + timeouts (or adds consensus)

#### Session flow
1. Policy Engine publishes a signed grant into a job channel.
2. Nodes observe the job and coordinate:
   - leader elected deterministically, **or**
   - first receiver becomes initiator and recruits participants
3. Nodes run MPC among themselves, routing messages P2P.
4. Nodes publish final signature to a result channel.

#### Key security principle
- Nodes require a valid policy grant; otherwise anyone can spam the job channel.

---

## 4. Pros and cons

### A) Coordinator-Orchestrated MPC

#### Pros
- **Operational simplicity**
  - One place to implement/debug: session lifecycle, routing, retry policy
  - Better observability: coordinator can emit a full trace for “why signature failed”
- **Separation of concerns**
  - Policy decides *whether* to sign; coordinator decides *how* to execute signing
  - Nodes remain “crypto workers” rather than distributed systems engines
- **Upgrade velocity**
  - Swap protocol versions / curves / transports in coordinator + node client
- **Predictable latency**
  - Less peer discovery overhead; coordinator can optimize fan-out/fan-in routing
- **Independent scaling**
  - Coordinator scales mostly CPU/network; nodes scale crypto/secure storage

#### Cons
- **Single orchestration chokepoint**
  - If coordinator is down, signing halts (unless HA)
  - HA requires: multi-coordinator, idempotency, session ownership, shared store
- **Attack surface concentration**
  - Coordinator is a juicy target (traffic concentration + metadata exposure)
  - If nodes trust coordinator too much, compromise becomes catastrophic
- **State coordination complexity in HA**
  - Need idempotency keys, session locking/ownership, dedup/replay protection
- **Hub bottleneck risk**
  - Routing bandwidth can bottleneck at high throughput / many rounds

---

### B) Fully P2P Node-Coordinated MPC

#### Pros
- **No single orchestrator to take down**
  - Removes one class of single-point failure *(still need job channel reliability)*
- **Alignment with “nodes hold shares, nodes run protocol”**
  - Strong conceptual purity: keys never depend on a coordinator role
- **Potential resilience under partial failures**
  - If one node is unhealthy, others may still coordinate sessions
- **Lower central trust**
  - No “coordinator compromise” scenario because there isn’t one

#### Cons
- **Distributed systems complexity explodes**
  - Nodes must implement: session initiation rules, leader election/conflict resolution,
    membership management, backpressure/DoS handling, retries, state consistency
- **Harder debugging and incident response**
  - Failure truth scattered across nodes; log correlation is painful without tracing
- **More complex security perimeter**
  - More exposed interfaces per node; subtle liveness/consensus bugs become incidents
- **Upgrade & compatibility burden**
  - Rolling upgrades harder; mixed versions can deadlock sessions
- **Policy enforcement becomes trickier**
  - Every node must validate grants + enforce anti-replay
  - Job/result channels must resist cheap flooding

---

## 5. Security analysis criteria

### 5.1 Trust boundaries

**Coordinator model**
- Coordinator is not trusted to authorize signing (if built correctly).
- Nodes trust only:
  - local share store
  - policy grant signatures
  - protocol correctness checks

**P2P model**
- Authorization still relies on policy grant validation.
- Nodes must also trust session formation logic (election/selection), which is harder to harden.

### 5.2 Compromise impact (“blast radius”)

**Coordinator compromised**
- Best case: DoS, metadata leakage, traffic reordering; cannot sign without valid grants.
- Worst case (bad design): nodes sign unauthorized messages if grants aren’t verified strictly.

**Single node compromised**
- Should not enable signing alone if threshold is enforced.
- Compromised nodes can:
  - leak shares (depending on storage/HSM/TEE)
  - sabotage liveness
  - attempt protocol attacks (nonce bias, equivocation, etc.)

### 5.3 Replay and idempotency (both designs)
Require:
- unique `grant_id` and derived/unique `session_id`
- expiry timestamps
- nonce/sequence numbers
- replay cache (coordinator-side or node-side)

### 5.4 DoS resistance

**Coordinator model**
- Gateway can protect coordinator; nodes can live on private network.

**P2P model**
- Nodes often need broader connectivity; DoS protection becomes distributed and expensive.

---

## 6. Operational criteria

### Observability
- **Coordinator:** easiest (single orchestrator can emit span-per-round).
- **P2P:** requires disciplined distributed tracing across nodes.

### Incident response
- **Coordinator:** isolate issues centrally; faster.
- **P2P:** “failed somewhere in the swarm” unless tracing is first-class.

### Deployment & upgrades
- **Coordinator:** rolling upgrades easier; coordinator can support multiple versions.
- **P2P:** all nodes must remain compatible; heterogeneity is risky.

### Scaling
- **Coordinator:** scale coordinators horizontally; nodes scale by adding crypto capacity.
- **P2P:** scaling increases membership/session formation overhead.

---

## 7. Performance and latency criteria

### Latency
Coordinator often wins:
- hub for routing; less peer-discovery cost
- can minimize hops and optimize fan-out/fan-in routing

P2P can be fast too, but typically requires:
- stable membership
- optimized routing
- minimal election overhead

### Throughput
Coordinator can bottleneck if:
- many rounds and large messages
- coordinator bandwidth becomes the choke point

P2P distributes bandwidth better, but usually increases total chatter and complexity.

---

## 8. Decision criteria checklist

### Choose Coordinator-Orchestrated if you care most about:
- fast time-to-production with fewer distributed-systems risks
- separation of concerns: policy vs orchestration vs crypto workers
- strong observability and audit trails
- simpler rolling upgrades + version negotiation
- keeping nodes minimal and hardened

### Choose Fully P2P Node Coordination if you care most about:
- eliminating an orchestration chokepoint entirely
- strong decentralization story (organizational/regulatory)
- a team experienced in distributed consensus / P2P operations
- willingness to invest heavily in tracing/debugging/upgrade discipline

---

## 9. Recommended hybrid (often best in practice)

A practical sweet spot:
- Keep a coordinator role, but make it **replaceable and non-authoritative**.
- Nodes validate policy grants directly (signed by Raft policy cluster).
- Run multiple coordinators behind the gateway (stateless-ish), with:
  - idempotency keys
  - shared session store (Redis/Postgres) **or** deterministic session ownership

This gets you:
- coordinator simplicity + observability
- no “coordinator can steal funds” trust assumption
- HA without turning nodes into distributed orchestration engines

---

## 10. Summary: Is the coordinator unnecessary?

Not unnecessary—just optional.

- For a maintainable, auditable, evolvable product: **coordinator is usually the sane default**, provided it cannot authorize signing.
- Fully P2P orchestration is possible, but it’s “distributed systems on hard mode” and often costs more engineering than expected.

---

## 11. Implementation-plan seed: what to build first

### Recommended baseline
**Hybrid Coordinator-Orchestrated MPC**:
- multiple coordinators (HA)
- coordinator is non-authoritative
- nodes verify signed policy grants
- shared session store or deterministic ownership

### Hard requirements (turn into milestones)
- **Grant format + verification** (policy-signed, replay-safe, expiring)
- **Session ID scheme** (deterministic from `grant_id` + participants + protocol version)
- **Coordinator session state machine** (round routing, retries, timeouts)
- **Node runtime** (crypto worker: share store + protocol rounds + strict validation)
- **Transport** (gRPC/HTTP/libp2p) with authentication + rate limits
- **Observability** (trace per session + per round, structured logs, metrics)
- **DoS controls** (gateway throttling + node-side validation and caps)
- **Upgrade strategy** (version negotiation; coordinator may bridge versions)

### Key interfaces (define early)
- `Grant` (signed by policy cluster)
- `StartSession(session_id, grant, role, participants, protocol_version)`
- `RoundMessage(session_id, round, from, to, payload)`
- `Finalize(session_id) -> signature | failure_reason`
- `ResultPublish(grant_id, session_id, signature, tx_hash?)`

### Failure modes (test plan inputs)
- node dropout mid-round
- coordinator crash mid-session
- duplicate grant submission (idempotency)
- malicious/invalid round messages
- replayed grant/session messages
- mixed protocol versions during rolling upgrades
- partitioned network and delayed messages

### Open questions (force explicit decisions)
- participant selection policy (fixed set vs coordinator-chosen vs node-chosen)
- quorum behavior when some nodes are slow/unavailable
- what data coordinators are allowed to see (metadata minimization)
- persistence strategy for sessions (in-memory + durable store)
- how to handle partial signatures / abort semantics
- storage model for shares (software, HSM, enclave) and recovery/rotation
