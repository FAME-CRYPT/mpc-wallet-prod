# 🚀 MPC Wallet - Future Improvements & Remaining Work

**Date:** 2026-01-22
**Version:** 2.0
**Status:** Complete Implementation Roadmap
**Target:** Transition from Mock/Dummy to Production-Ready Bitcoin MPC Wallet

---

## 📑 Table of Contents

### Core Documentation
- [📋 Executive Summary](#-executive-summary)
- [🎯 Protocol Architecture Clarification](#-protocol-architecture-clarification)
- [📊 Protocol Comparison Table](#-protocol-comparison-table)

### Implementation Plan (Priority Order)
- [🔧 PRIORITY 0: Distributed Key Generation (DKG)](#priority-0-distributed-key-generation-dkg---critical) - **CRITICAL** (3-4 days)
  - CGGMP24 DKG Integration
  - FROST DKG Integration (Givre)
- [🔧 PRIORITY 1: Presignature Pool Management](#priority-1-presignature-pool-management---critical) - **CRITICAL** (3-4 days)
- [🔧 PRIORITY 2: CGGMP24 Signing Integration](#priority-2-cggmp24-signing-integration-remove-mock---critical) - **CRITICAL** (4-5 days)
- [🔧 PRIORITY 2.5: Intelligent Protocol Selection](#priority-25-intelligent-protocol-selection--multi-protocol-support---critical) - **CRITICAL** (2-3 days)
- [🔧 PRIORITY 3: FROST Signing Integration](#priority-3-frost-signing-integration---high) - HIGH (3-4 days)
- [🔧 PRIORITY 4: QUIC Vote Broadcasting](#priority-4-quic-vote-broadcasting---medium) - MEDIUM (2-3 days)
- [🔧 PRIORITY 5: Signature Verification](#priority-5-signature-verification---medium) - MEDIUM (1-2 days)
- [🔧 PRIORITY 6: mTLS Certificate Validation](#priority-6-mtls-certificate-validation---low) - LOW (1 day)

### Project Management
- [📦 Source Code Mapping](#-source-code-mapping)
- [🗺️ Implementation Roadmap (Timeline)](#️-implementation-roadmap-timeline)
- [📊 Work Estimate Summary](#-work-estimate-summary)
- [🎯 Minimum Viable Product (MVP) Definition](#-minimum-viable-product-mvp-definition)
- [🚨 Critical Dependencies](#-critical-dependencies)
- [📝 Files Summary](#-files-summary)
- [✅ What's Already Done](#-whats-already-done-dont-redo)

### Reference & Support
- [🔧 Cargo Dependencies Reference](#-cargo-dependencies-reference)
- [🐛 Troubleshooting Guide](#-troubleshooting-guide)
- [📚 API Examples](#-api-examples)
  - Complete DKG Flow (CGGMP24)
  - Complete FROST DKG Flow (Givre)
  - FROST Signing with Givre
- [🎓 Learning Resources](#-learning-resources)
- [🔍 Testing Strategy](#-testing-strategy)
- [📞 Support & Questions](#-support--questions)
- [🎯 Quick Start Checklist](#-quick-start-checklist)

---

## 📋 Executive Summary

This document provides a **complete roadmap** for implementing all missing/placeholder functionality in the MPC wallet system. The system currently has excellent architecture and infrastructure, but uses **mock signatures and incomplete protocol integration**.

### Current System Status

✅ **Working Infrastructure:**
- 5-node cluster with QUIC+mTLS transport
- PostgreSQL + etcd coordination
- Transaction lifecycle orchestrator
- Byzantine fault detection
- REST API + CLI tools
- Docker deployment

❌ **Missing/Mock Components:**
- **Cryptographic protocols not integrated** (FROST, CGGMP24)
- **Presignature pool empty** (no background generation)
- **DKG not implemented** (no distributed key generation)
- **Mock signatures** (not real Bitcoin signatures)
- **No QUIC vote broadcasting** (manual SQL insertion only)

### 📚 Cryptography Libraries Used

| Protocol | Library | Version | Repository | Purpose | Source Code |
|----------|---------|---------|------------|---------|-------------|
| **CGGMP24** (ECDSA) | `cggmp24` | 0.7.0-alpha.3 | [ZenGo-X/cggmp24](https://github.com/ZenGo-X/cggmp24) | SegWit Bitcoin signing | `threshold-signing (Copy)/` |
| **FROST** (Schnorr) | `givre` | 0.2 | [LFDT-Lockness/givre](https://github.com/LFDT-Lockness/givre) | Taproot Bitcoin signing | `torcus-wallet/` |

**⚠️ IMPORTANT - DO NOT MIX LIBRARIES:**
- **CGGMP24 (SegWit)**: Use `cggmp24` library from `threshold-signing (Copy)` folder
  - DKG: `cggmp24::keygen`
  - Signing: `cggmp24::signing` + `cggmp24::signing_fast`
  - Presignatures: `cggmp24::presignature`

- **FROST (Taproot)**: Use `givre` library from `torcus-wallet` folder
  - DKG: `givre::keygen` (FROST-native, NOT CGGMP21)
  - Signing: `givre::signing`
  - No presignatures (not needed - faster protocol)

**Givre Features (FROST only):**
- `full-signing`: Complete FROST DKG + signing protocol
- `serde`: Network serialization
- `ciphersuite-bitcoin`: Bitcoin secp256k1 curve support

---

## 🎯 Protocol Architecture Clarification

### Question: "3 Protocol Var mı? FROST, CGGMP24, Schnorr?"

**ANSWER: HAYIR - 2 PROTOCOL VAR:**

#### 1. **CGGMP24** = ECDSA Threshold Signing
- **Use Case:** SegWit Bitcoin addresses (P2WPKH, P2WSH)
- **Signature Type:** ECDSA (Elliptic Curve Digital Signature Algorithm)
- **Curve:** secp256k1 (Bitcoin standard)
- **Library:** `cggmp24` crate (version 0.7.0-alpha.3)
- **Repository:** https://github.com/ZenGo-X/cggmp24
- **Output:** 64-byte ECDSA signature `(r, s)`

**Source Location:** ✅ `threshold-signing (Copy)/node/src/` folder
**Already Copied To:** ✅ `production/crates/protocols/src/cggmp24/`

```
threshold-signing (Copy)/node/src/    →    production/crates/protocols/src/cggmp24/
├── keygen.rs                          →    ├── keygen.rs              ✅ COPIED
├── presignature.rs                    →    ├── presignature.rs        ✅ COPIED
├── presignature_pool.rs               →    ├── presig_pool.rs         ✅ COPIED
├── signing.rs                         →    ├── signing.rs             ✅ COPIED
└── signing_fast.rs                    →    └── signing_fast.rs        ✅ COPIED
```

**Integration Status:** ❌ Kod kopyalandı ama orchestrator'a entegre EDİLMEDİ (Priority 0-2'de yapılacak)

#### 2. **FROST** = Schnorr Threshold Signing
- **Use Case:** Taproot Bitcoin addresses (P2TR) - BIP-340 compliant
- **Signature Type:** Schnorr signatures
- **Curve:** secp256k1 (Bitcoin standard)
- **Library:** `givre` crate (version 0.2)
  - **Repository:** https://github.com/LFDT-Lockness/givre
  - **Maintainer:** LFDT-Lockness (Lattice-Based Fully Distributed Threshold)
  - **Features Used:**
    - `full-signing`: Complete FROST DKG + signing protocol
    - `serde`: Serialization support for network transmission
    - `ciphersuite-bitcoin`: Bitcoin secp256k1 curve support
- **Output:** 64-byte Schnorr signature `(R || s)`
- **BIP Compliance:** BIP-340 (Taproot Schnorr signatures)

**Why Givre?**
- Production-ready FROST implementation by LFDT team
- Built-in FROST-native DKG (not CGGMP-based)
- Bitcoin-specific ciphersuite for secp256k1
- Full serialization support for distributed operations
- Well-maintained and actively developed
- Simpler and faster than CGGMP24 (2-3 rounds vs 5-6)

**Source Location:** ✅ `torcus-wallet/crates/protocols/src/frost/` folder
**Already Copied To:** ✅ `production/crates/protocols/src/frost/`

```
torcus-wallet/crates/protocols/src/frost/    →    production/crates/protocols/src/frost/
├── keygen.rs                                 →    ├── keygen.rs              ✅ COPIED
└── signing.rs                                →    └── signing.rs             ✅ COPIED
```

**Integration Status:** ❌ Kod kopyalandı ama orchestrator'a entegre EDİLMEDİ (Priority 0 & 3'te yapılacak)

**IMPORTANT:** FROST için Givre kullanıyoruz, CGGMP24 için `cggmp24` kütüphanesi kullanıyoruz. KARIŞMASALAR!

### IMPORTANT: Schnorr ≠ Separate Protocol

**Schnorr** is the **signature algorithm** used by **FROST**:
- FROST protocol → produces → Schnorr signatures (for Taproot)
- CGGMP24 protocol → produces → ECDSA signatures (for SegWit)

So we have **2 protocols (FROST + CGGMP24)**, not 3.

---

## 📊 Protocol Comparison Table

| Feature | CGGMP24 (ECDSA) | FROST (Schnorr) |
|---------|----------------|-----------------|
| **Bitcoin Address Type** | SegWit (P2WPKH, P2WSH) | Taproot (P2TR) |
| **Signature Type** | ECDSA | Schnorr (BIP-340) |
| **Presignature Pool** | ✅ Supported | ⚠️ Not yet implemented |
| **Signing Speed (with pool)** | <500ms | ~800ms (no pool yet) |
| **Signing Speed (full)** | ~2-3 seconds | ~800ms (simpler protocol) |
| **Setup Complexity** | High (aux info needed) | Medium (simpler DKG) |
| **Library** | `cggmp24` v0.7.0-alpha.3 | `givre` v0.2 ([GitHub](https://github.com/LFDT-Lockness/givre)) |
| **Source Code Location** | `threshold-signing (Copy)` | `torcus-wallet` |
| **Production Crate Status** | ✅ Code copied to `production/crates/protocols/src/cggmp24/` | ✅ Code copied to `production/crates/protocols/src/frost/` |
| **Integration Status** | ❌ Not integrated (mock signatures used) | ❌ Not integrated |

---

## 🔧 Implementation Plan

### **PRIORITY 0: Distributed Key Generation (DKG)** - CRITICAL

**Effort:** 3-4 days
**Complexity:** High
**Blocking:** ALL signature operations

#### Why Critical?
Without DKG, there are **NO key shares** to sign with. Currently, no keys exist in the system.

#### What Needs to Be Done?

##### A. **CGGMP24 DKG Integration**

**Source Code:** `threshold-signing (Copy)/node/src/keygen.rs`
**Target:** `production/crates/orchestrator/src/dkg_service.rs` (NEW FILE)

**Implementation Steps:**

1. **Create DKG Orchestration Service**
   ```rust
   // production/crates/orchestrator/src/dkg_service.rs
   pub struct DkgService {
       postgres: Arc<PostgresStorage>,
       etcd: Arc<EtcdStorage>,
       quic: Arc<QuicEngine>,
       protocol_type: ProtocolType, // CGGMP24 or FROST
   }

   impl DkgService {
       /// Run distributed key generation for 4-of-5 threshold
       pub async fn run_dkg(&self, participants: Vec<NodeId>) -> Result<DkgResult> {
           // 1. Acquire etcd lock /locks/dkg
           // 2. Broadcast DKG initiation to all nodes via QUIC
           // 3. Coordinate DKG rounds (5-6 rounds for CGGMP24)
           // 4. Collect key shares from all nodes
           // 5. Store shared public key in etcd /cluster/public_key
           // 6. Each node stores encrypted key share in PostgreSQL
       }
   }
   ```

2. **QUIC Message Types**
   ```rust
   // production/crates/types/src/messages.rs
   pub enum DkgMessage {
       InitiateDkg { session_id: String, participants: Vec<u16> },
       DkgRound1 { party_index: u16, commitments: Vec<u8> },
       DkgRound2 { party_index: u16, shares: HashMap<u16, Vec<u8>> },
       DkgRound3 { party_index: u16, verification: Vec<u8> },
       DkgComplete { public_key: Vec<u8> },
       DkgFailed { error: String },
   }
   ```

3. **API Endpoint**
   ```rust
   // production/crates/api/src/handlers/dkg.rs (NEW FILE)
   pub async fn initiate_dkg(
       State(state): State<AppState>,
   ) -> ApiResult<Json<DkgResponse>> {
       // Trigger DKG across all 5 nodes
       let result = state.orchestrator.dkg_service.run_dkg().await?;

       Ok(Json(DkgResponse {
           success: result.success,
           public_key: hex::encode(&result.public_key),
           address: bitcoin_address_from_pubkey(&result.public_key)?,
       }))
   }
   ```

4. **CLI Command**
   ```bash
   # Start DKG ceremony
   mpc-wallet-cli dkg start --threshold 4 --total 5

   # Check DKG status
   mpc-wallet-cli dkg status
   ```

5. **Storage Schema**
   ```sql
   -- New table for DKG state
   CREATE TABLE dkg_ceremonies (
       id BIGSERIAL PRIMARY KEY,
       session_id TEXT NOT NULL UNIQUE,
       protocol TEXT NOT NULL, -- 'cggmp24' or 'frost'
       threshold INTEGER NOT NULL,
       total_nodes INTEGER NOT NULL,
       status TEXT NOT NULL, -- 'running', 'completed', 'failed'
       public_key BYTEA,
       started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
       completed_at TIMESTAMPTZ
   );

   -- Store encrypted key shares per node
   CREATE TABLE key_shares (
       id BIGSERIAL PRIMARY KEY,
       ceremony_id BIGINT REFERENCES dkg_ceremonies(id),
       node_id BIGINT NOT NULL,
       encrypted_share BYTEA NOT NULL,
       created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
       UNIQUE(ceremony_id, node_id)
   );
   ```

##### B. **FROST DKG Integration**

**Source Code:** `torcus-wallet/crates/protocols/src/frost/keygen.rs`
**Target:** `production/crates/orchestrator/src/dkg_service.rs` (same file, different method)
**Library:** `givre` v0.2 (https://github.com/LFDT-Lockness/givre)

**IMPORTANT:** Givre is **ONLY for FROST** (Schnorr/Taproot). CGGMP24 (ECDSA/SegWit) uses the separate `cggmp24` library.

**Implementation:**
- Use `givre::keygen` module for FROST distributed key generation
- **Different from CGGMP24:** FROST uses its own DKG protocol (not CGGMP21)
- Simpler protocol: 2-3 rounds (vs 5-6 rounds for CGGMP24)
- Faster completion: ~500ms (vs ~8s for CGGMP24)
- Produces BIP-340 compliant Schnorr keys for Taproot

**Givre Features Configuration:**
```toml
# production/Cargo.toml
givre = {
    version = "0.2",
    features = [
        "full-signing",          # Complete FROST signing implementation (includes DKG)
        "serde",                 # Network serialization support
        "ciphersuite-bitcoin"    # Bitcoin secp256k1 curve
    ]
}
```

**Note:** We do NOT use `cggmp21-keygen` feature. Givre has its own FROST-native DKG.

**Integration:**
```rust
use givre::ciphersuite::Secp256k1;
use givre::keygen::{KeygenRound1, KeygenRound2};

impl DkgService {
    pub async fn run_frost_dkg(&self, participants: Vec<NodeId>) -> Result<DkgResult> {
        // 1. Use givre library for FROST-native DKG
        // 2. Produces Schnorr-compatible key shares
        // 3. Store x-only public key (32 bytes) for Taproot (BIP-340)
        // 4. Key shares are compatible with givre::signing module

        // Round 1: Generate and broadcast commitments
        let (round1_secret, round1_broadcast) = givre::keygen::round1::<Secp256k1>(
            self.party_index,
            self.threshold,
            participants.len() as u16,
        )?;

        self.broadcast_frost_message(round1_broadcast).await?;

        // Round 2: Collect commitments and generate key share
        let round1_messages = self.collect_frost_round1().await?;
        let key_share = givre::keygen::round2(
            round1_secret,
            round1_messages,
        )?;

        // Extract BIP-340 compatible x-only public key for Taproot
        let x_only_pubkey = key_share.public_key().x_only();

        Ok(DkgResult {
            public_key: x_only_pubkey.serialize(),
            protocol: ProtocolType::FROST,
        })
    }
}
```

**Why Givre for FROST?**
- **Single Library Solution:** Same library handles both DKG and signing
- **FROST-Native DKG:** Built-in FROST DKG (not adapted from CGGMP)
- **Bitcoin Native:** Built-in Bitcoin secp256k1 support via `ciphersuite-bitcoin`
- **BIP-340 Compliant:** Produces x-only public keys for Taproot
- **Network Ready:** Full `serde` support for distributed message passing
- **Faster & Simpler:** 2-3 rounds vs 5-6 rounds for CGGMP24

#### Testing DKG

**E2E Test:**
```rust
#[tokio::test]
async fn test_cggmp24_dkg() {
    let cluster = setup_5_node_cluster().await;

    // Initiate DKG on node 1
    let response = cluster.nodes[0]
        .post("/api/v1/dkg/initiate")
        .json(&json!({ "protocol": "cggmp24", "threshold": 4, "total": 5 }))
        .send()
        .await?;

    assert_eq!(response.status(), 200);

    // Wait for DKG to complete
    tokio::time::sleep(Duration::from_secs(15)).await;

    // Verify all nodes have key shares
    for node in &cluster.nodes {
        let keys = node.get("/api/v1/dkg/keys").await?;
        assert!(keys["has_key_share"].as_bool().unwrap());
    }

    // Verify shared public key
    let pubkey1 = cluster.nodes[0].get("/api/v1/dkg/public_key").await?;
    let pubkey2 = cluster.nodes[1].get("/api/v1/dkg/public_key").await?;
    assert_eq!(pubkey1["public_key"], pubkey2["public_key"]); // Same public key
}
```

**Files to Modify:**
```
✏️ production/crates/orchestrator/src/dkg_service.rs (NEW)
✏️ production/crates/api/src/handlers/dkg.rs (NEW)
✏️ production/crates/api/src/routes/dkg.rs (NEW)
✏️ production/crates/types/src/messages.rs (add DkgMessage)
✏️ production/crates/cli/src/commands/dkg.rs (remove TODO, implement real calls)
✏️ production/docker/init-db/01_schema.sql (add dkg_ceremonies, key_shares tables)
```

---

### **PRIORITY 1: Presignature Pool Management** - CRITICAL

**Effort:** 3-4 days
**Complexity:** Medium-High
**Blocking:** Fast signing (<500ms)

#### Why Critical?
Without presignatures, **EVERY signature takes 2-3 seconds**. Presignatures reduce this to **<500ms**.

#### What Needs to Be Done?

**Source Code:** `threshold-signing (Copy)/node/src/presignature_pool.rs`
**Current Status:** ✅ Pool structure exists in `production/crates/protocols/src/cggmp24/presig_pool.rs`, but **EMPTY** (no background generation)

**Implementation:**

1. **Background Presignature Generation Service**
   ```rust
   // production/crates/orchestrator/src/presig_service.rs (NEW FILE)
   pub struct PresignatureService {
       pool: PresignaturePoolHandle<Secp256k1, SecurityLevel128>,
       quic: Arc<QuicEngine>,
       postgres: Arc<PostgresStorage>,
       key_share: Arc<Mutex<Option<KeyShare<Secp256k1, SecurityLevel128>>>>,
       target_size: usize, // 100
       min_size: usize,    // 20
   }

   impl PresignatureService {
       /// Background task that runs continuously
       pub async fn run_generation_loop(&self) {
           loop {
               // Check pool size every 10 seconds
               tokio::time::sleep(Duration::from_secs(10)).await;

               let stats = self.pool.stats().await;
               info!("Presignature pool: {}/{} ({}%)",
                   stats.current_size, stats.target_size, stats.utilization);

               // Refill if below minimum
               if stats.current_size < self.min_size {
                   let batch_size = self.target_size - stats.current_size;
                   info!("Pool low, generating {} presignatures...", batch_size);

                   match self.generate_batch(batch_size).await {
                       Ok(count) => info!("Generated {} presignatures", count),
                       Err(e) => error!("Presig generation failed: {}", e),
                   }
               }
           }
       }

       /// Generate a batch of presignatures
       async fn generate_batch(&self, count: usize) -> Result<usize> {
           // 1. Acquire etcd lock /locks/presig-generation
           // 2. For each presignature:
           //    - Run CGGMP presigning protocol (2 rounds)
           //    - Store in pool (in-memory)
           //    - Save to PostgreSQL (encrypted)
           // 3. Release lock
       }
   }
   ```

2. **Start Presig Service in Server**
   ```rust
   // production/crates/api/src/bin/server.rs
   #[tokio::main]
   async fn main() -> Result<()> {
       // ... existing setup ...

       // Start presignature generation service
       let presig_service = PresignatureService::new(
           presig_pool.clone(),
           quic_engine.clone(),
           postgres.clone(),
           key_share.clone(),
       );

       tokio::spawn(async move {
           presig_service.run_generation_loop().await;
       });

       // ... start API server ...
   }
   ```

3. **API Endpoints**
   ```rust
   // GET /api/v1/presignatures/status
   pub async fn presignature_status(
       State(state): State<AppState>,
   ) -> ApiResult<Json<PresigStatusResponse>> {
       let stats = state.presig_pool.stats().await;
       Ok(Json(PresigStatusResponse {
           current_size: stats.current_size,
           target_size: stats.target_size,
           max_size: stats.max_size,
           utilization: stats.utilization,
           is_healthy: stats.is_healthy(),
           is_critical: stats.is_critical(),
       }))
   }

   // POST /api/v1/presignatures/generate
   pub async fn generate_presignatures(
       State(state): State<AppState>,
       Json(req): Json<GeneratePresigRequest>,
   ) -> ApiResult<Json<GeneratePresigResponse>> {
       // Manually trigger presignature generation
       let count = state.presig_service.generate_batch(req.count).await?;
       Ok(Json(GeneratePresigResponse { generated: count }))
   }
   ```

4. **CLI Commands**
   ```bash
   # Check presignature pool status
   mpc-wallet-cli presig status

   # Manually generate presignatures
   mpc-wallet-cli presig generate --count 10
   ```

5. **Database Schema**
   ```sql
   -- Presignature storage (already exists, enhance it)
   ALTER TABLE presignature_usage ADD COLUMN is_used BOOLEAN DEFAULT FALSE;
   ALTER TABLE presignature_usage ADD COLUMN pool_generation_batch UUID;

   -- Add index for fast lookup
   CREATE INDEX idx_presignature_usage_is_used ON presignature_usage(is_used)
       WHERE is_used = FALSE;
   ```

**Performance Targets:**
- Pool target size: **100 presignatures**
- Pool minimum size: **20 presignatures** (trigger refill)
- Pool maximum size: **150 presignatures**
- Generation rate: **~5 presignatures/minute** (parallelized)
- Generation time per presignature: **~400ms** (with 5 nodes)

**Files to Modify:**
```
✏️ production/crates/orchestrator/src/presig_service.rs (NEW)
✏️ production/crates/api/src/handlers/presig.rs (NEW)
✏️ production/crates/api/src/routes/presig.rs (NEW)
✏️ production/crates/api/src/bin/server.rs (add presig service startup)
✏️ production/crates/cli/src/commands/presig.rs (remove TODO, implement real calls)
```

---

### **PRIORITY 2: CGGMP24 Signing Integration (REMOVE MOCK)** - CRITICAL

**Effort:** 4-5 days
**Complexity:** Very High
**Blocking:** Real Bitcoin transactions

#### Why Critical?
Currently using **MOCK signatures** (hardcoded bytes). These are **NOT valid Bitcoin signatures**.

**Current Mock Code:** `production/crates/orchestrator/src/service.rs:398-410`
```rust
// CURRENT (MOCK):
let mock_signed_tx = vec![
    0x02, 0x00, 0x00, 0x00, // version
    0x01, // input count
    0xde, 0xad, 0xbe, 0xef, // dummy data
    ...
];
```

**Source Code:** `threshold-signing (Copy)/node/src/signing_fast.rs`
**Target:** `production/crates/orchestrator/src/service.rs:388-533`

#### Implementation Steps

##### Step 1: Replace Mock with Real CGGMP24 Fast Signing

```rust
// production/crates/orchestrator/src/service.rs
async fn transition_approved_to_signing(&self, tx: &Transaction) -> Result<()> {
    // Step 1: Transition to 'signing' state
    self.postgres
        .update_transaction_state(&tx.txid, TransactionState::Signing)
        .await?;

    // Step 2: Get presignature from pool
    let presignature = self.presig_pool.take().await
        .ok_or(OrchestrationError::PresignaturePoolEmpty)?;

    info!("Using presignature for fast signing: {:?}", presignature.id);

    // Step 3: Create signing request
    let signing_request = SigningRequest {
        tx_id: tx.txid.clone(),
        unsigned_tx: tx.unsigned_tx.clone(),
        presignature_id: presignature.id,
    };

    // Step 4: Broadcast signing request via QUIC to all nodes
    let signing_msg = ProtocolMessage::SigningRequest(signing_request);
    self.quic_engine.broadcast_to_all(signing_msg).await?;

    // Step 5: Wait for signature shares from threshold nodes (4-of-5)
    let signature_shares = self.collect_signature_shares(&tx.txid, Duration::from_secs(30)).await?;

    // Step 6: Combine signature shares into final signature
    let final_signature = combine_cggmp_signatures(signature_shares, &presignature)?;

    // Step 7: Construct signed Bitcoin transaction
    let signed_tx = construct_signed_bitcoin_tx(&tx.unsigned_tx, &final_signature)?;

    // Step 8: Verify signature
    verify_bitcoin_signature(&signed_tx)?;

    // Step 9: Store signed transaction
    self.postgres.set_signed_transaction(&tx.txid, &signed_tx).await?;

    // Step 10: Transition to 'signed' state
    self.postgres
        .update_transaction_state(&tx.txid, TransactionState::Signed)
        .await?;

    info!("CGGMP signing completed for: {:?}", tx.txid);
    Ok(())
}
```

##### Step 2: Add Signature Share Collection

```rust
impl TransactionOrchestrator {
    async fn collect_signature_shares(
        &self,
        tx_id: &TxId,
        timeout: Duration,
    ) -> Result<Vec<SignatureShare>> {
        let mut shares = Vec::new();
        let start = Instant::now();

        while shares.len() < self.threshold && start.elapsed() < timeout {
            // Poll QUIC for incoming SignatureShare messages
            if let Some(msg) = self.quic_engine.recv_message().await? {
                if let ProtocolMessage::SignatureShare(share) = msg {
                    if share.tx_id == *tx_id {
                        shares.push(share);
                        info!("Received signature share {}/{}", shares.len(), self.threshold);
                    }
                }
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        if shares.len() < self.threshold {
            return Err(OrchestrationError::InsufficientSignatureShares {
                received: shares.len(),
                required: self.threshold,
            });
        }

        Ok(shares)
    }
}
```

##### Step 3: Node-Side Signing Handler

```rust
// production/crates/protocols/src/p2p/message_handler.rs
async fn handle_signing_request(
    &self,
    request: SigningRequest,
) -> Result<SignatureShare> {
    // 1. Get our key share
    let key_share = self.key_share_manager.get_key_share().await?;

    // 2. Get our presignature
    let our_presignature = self.presig_pool.get_presignature(&request.presignature_id).await?;

    // 3. Compute message hash from unsigned_tx
    let message_hash = compute_sighash(&request.unsigned_tx)?;

    // 4. Issue partial signature using CGGMP
    let data_to_sign = DataToSign::digest::<sha2::Sha256>(&message_hash);
    let partial_sig = cggmp24::signing::issue_partial_signature(
        our_presignature,
        data_to_sign,
    );

    // 5. Return signature share
    Ok(SignatureShare {
        tx_id: request.tx_id,
        node_id: self.node_id,
        partial_signature: partial_sig,
        presignature_id: request.presignature_id,
    })
}
```

##### Step 4: Signature Combination

```rust
fn combine_cggmp_signatures(
    shares: Vec<SignatureShare>,
    presignature: &StoredPresignature,
) -> Result<EcdsaSignature> {
    // Extract partial signatures
    let partial_sigs: Vec<PartialSignature> = shares
        .iter()
        .map(|s| s.partial_signature.clone())
        .collect();

    // Combine using CGGMP presignature combine function
    let signature = cggmp24::signing::combine_partial_signatures(
        &partial_sigs,
        &presignature.public_data,
        data_to_sign,
    )?;

    Ok(signature)
}
```

**Files to Modify:**
```
✏️ production/crates/orchestrator/src/service.rs (lines 388-533: replace mock)
✏️ production/crates/types/src/messages.rs (add SigningRequest, SignatureShare)
✏️ production/crates/protocols/src/p2p/message_handler.rs (add handle_signing_request)
✏️ production/crates/common/src/bitcoin_utils.rs (add Bitcoin signature helpers)
✏️ production/crates/crypto/src/lib.rs (add signature verification)
```

---

### **PRIORITY 2.5: Intelligent Protocol Selection & Multi-Protocol Support** - CRITICAL

**Effort:** 2-3 days
**Complexity:** Medium
**Blocking:** Proper Bitcoin address type handling
**Integration Point:** Between Transaction Creation and Signing

#### Why Critical?

Bitcoin has **multiple address types** requiring **different signature algorithms**:
- **SegWit (P2WPKH, P2WSH)** → Requires **ECDSA signatures** (CGGMP24)
- **Taproot (P2TR)** → Requires **Schnorr signatures** (FROST)
- **Legacy (P2PKH, P2SH)** → Also requires ECDSA (CGGMP24)

**The system MUST automatically detect the recipient address type and select the correct protocol.** Using the wrong signature type will result in **invalid transactions rejected by Bitcoin nodes**.

---

#### 🎯 SOTA (State-of-the-Art) Design Goals

1. **Zero User Intervention**: User sends to any Bitcoin address, system auto-detects type
2. **Multi-DKG Support**: Maintain both CGGMP24 and FROST key shares simultaneously
3. **Performance Optimization**: Use presignature pools for both protocols
4. **Future-Proof**: Easy to add new protocols (e.g., MuSig2, ROAST)
5. **Validation**: Verify signature type matches address type before broadcast

---

#### 🏗️ Architecture: Dual-Protocol System

**System State:**
```rust
pub struct WalletState {
    // Dual key shares (both generated during initial DKG)
    cggmp24_key_share: Option<KeyShare<Secp256k1, SecurityLevel128>>,
    frost_key_share: Option<FrostKeyShare>,

    // Dual presignature pools
    cggmp24_presig_pool: PresignaturePool<Secp256k1, SecurityLevel128>,
    frost_presig_pool: FrostPresignaturePool, // Future enhancement

    // Derived addresses (one wallet = multiple address types)
    segwit_address: String,    // bc1q... (P2WPKH from CGGMP24 pubkey)
    taproot_address: String,   // bc1p... (P2TR from FROST pubkey)

    // Protocol capabilities
    supported_protocols: Vec<ProtocolType>,
}
```

**Key Insight:** One MPC wallet generates **TWO sets of keys** (CGGMP24 + FROST), allowing it to:
- **Receive** funds at both SegWit and Taproot addresses
- **Send** to any Bitcoin address type
- **Switch protocols** automatically based on recipient

---

#### 📊 Protocol Selection Logic (Automatic Detection)

**Detection Flow:**
```
User Input: Recipient Address
       ↓
[1] Parse Address String
       ↓
[2] Detect Address Type (P2WPKH/P2WSH/P2TR/P2PKH/P2SH)
       ↓
[3] Select Protocol:
    - Taproot (P2TR) → FROST
    - SegWit/Legacy → CGGMP24
       ↓
[4] Verify We Have Key Share for Selected Protocol
       ↓
[5] Route to Correct Signing Engine
       ↓
[6] Generate Signature
       ↓
[7] Verify Signature Type Matches Address Type
       ↓
[8] Broadcast Transaction
```

---

#### 🔧 Implementation: Smart Protocol Router

**Step 1: Address Type Detection**

```rust
// production/crates/common/src/bitcoin_address.rs (NEW FILE)

use bitcoin::{Address, AddressType};
use anyhow::{Result, bail};

/// Detect Bitcoin address type and required signature algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitcoinAddressClass {
    /// Native SegWit (bech32): bc1q... - Requires ECDSA
    NativeSegWit,
    /// Taproot (bech32m): bc1p... - Requires Schnorr
    Taproot,
    /// Legacy P2PKH: 1... - Requires ECDSA
    LegacyP2PKH,
    /// Legacy P2SH: 3... - Requires ECDSA
    LegacyP2SH,
    /// Nested SegWit: 3... (P2WPKH-in-P2SH) - Requires ECDSA
    NestedSegWit,
}

impl BitcoinAddressClass {
    /// Parse address and determine class
    pub fn detect(address_str: &str, network: bitcoin::Network) -> Result<Self> {
        let address = Address::from_str(address_str)
            .map_err(|e| anyhow::anyhow!("Invalid Bitcoin address: {}", e))?
            .require_network(network)?;

        match address.address_type()? {
            AddressType::P2wpkh => Ok(Self::NativeSegWit),
            AddressType::P2wsh => Ok(Self::NativeSegWit),
            AddressType::P2tr => Ok(Self::Taproot),
            AddressType::P2pkh => Ok(Self::LegacyP2PKH),
            AddressType::P2sh => {
                // Could be nested SegWit (P2WPKH-in-P2SH) or pure P2SH
                // For simplicity, treat as legacy P2SH (both use ECDSA)
                Ok(Self::LegacyP2SH)
            }
        }
    }

    /// Get required signature protocol
    pub fn required_protocol(&self) -> ProtocolType {
        match self {
            Self::Taproot => ProtocolType::FROST,
            Self::NativeSegWit | Self::LegacyP2PKH | Self::LegacyP2SH | Self::NestedSegWit => {
                ProtocolType::CGGMP24
            }
        }
    }

    /// Get human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::NativeSegWit => "Native SegWit (P2WPKH/P2WSH)",
            Self::Taproot => "Taproot (P2TR)",
            Self::LegacyP2PKH => "Legacy (P2PKH)",
            Self::LegacyP2SH => "Pay-to-Script-Hash (P2SH)",
            Self::NestedSegWit => "Nested SegWit (P2WPKH-in-P2SH)",
        }
    }
}

/// Protocol selection result with metadata
pub struct ProtocolSelection {
    pub protocol: ProtocolType,
    pub address_class: BitcoinAddressClass,
    pub requires_presignature_pool: bool,
    pub estimated_signing_time_ms: u64,
}

impl ProtocolSelection {
    pub fn select(recipient_address: &str, network: bitcoin::Network) -> Result<Self> {
        let address_class = BitcoinAddressClass::detect(recipient_address, network)?;
        let protocol = address_class.required_protocol();

        let (requires_presignature_pool, estimated_signing_time_ms) = match protocol {
            ProtocolType::CGGMP24 => (true, 450),  // With pool: <500ms
            ProtocolType::FROST => (false, 800),   // No pool yet: ~800ms
        };

        Ok(Self {
            protocol,
            address_class,
            requires_presignature_pool,
            estimated_signing_time_ms,
        })
    }
}
```

---

**Step 2: Protocol Router in Orchestrator**

```rust
// production/crates/orchestrator/src/protocol_router.rs (NEW FILE)

use crate::types::{ProtocolType, Transaction};
use crate::common::bitcoin_address::{ProtocolSelection, BitcoinAddressClass};
use anyhow::{Result, bail};

pub struct ProtocolRouter {
    network: bitcoin::Network,
    cggmp24_available: bool,
    frost_available: bool,
}

impl ProtocolRouter {
    pub fn new(network: bitcoin::Network) -> Self {
        Self {
            network,
            cggmp24_available: true,  // Set based on DKG completion
            frost_available: true,
        }
    }

    /// Automatic protocol selection with validation
    pub async fn select_protocol_for_transaction(
        &self,
        tx: &Transaction,
    ) -> Result<ProtocolSelection> {
        // Step 1: Detect recipient address type
        let selection = ProtocolSelection::select(&tx.recipient, self.network)?;

        // Step 2: Verify we have required key share
        match selection.protocol {
            ProtocolType::CGGMP24 => {
                if !self.cggmp24_available {
                    bail!(
                        "Transaction requires CGGMP24 (ECDSA) for {} address, but CGGMP24 key share not available. Run DKG first.",
                        selection.address_class.description()
                    );
                }
            }
            ProtocolType::FROST => {
                if !self.frost_available {
                    bail!(
                        "Transaction requires FROST (Schnorr) for Taproot address, but FROST key share not available. Run FROST DKG first."
                    );
                }
            }
        }

        // Step 3: Log selection for observability
        info!(
            "Protocol selected: {:?} for {} address type (recipient: {})",
            selection.protocol,
            selection.address_class.description(),
            tx.recipient
        );

        Ok(selection)
    }
}
```

---

**Step 3: Integration into Transaction Orchestrator**

```rust
// production/crates/orchestrator/src/service.rs (MODIFY)

impl TransactionOrchestrator {
    async fn transition_approved_to_signing(&self, tx: &Transaction) -> Result<()> {
        // Step 1: Automatic protocol selection
        let protocol_selection = self.protocol_router
            .select_protocol_for_transaction(tx)
            .await?;

        info!(
            "Selected protocol: {:?}, estimated signing time: {}ms",
            protocol_selection.protocol,
            protocol_selection.estimated_signing_time_ms
        );

        // Step 2: Route to correct signing engine
        match protocol_selection.protocol {
            ProtocolType::CGGMP24 => {
                self.sign_with_cggmp24(tx, protocol_selection).await?
            }
            ProtocolType::FROST => {
                self.sign_with_frost(tx, protocol_selection).await?
            }
        }

        Ok(())
    }

    async fn sign_with_cggmp24(
        &self,
        tx: &Transaction,
        selection: ProtocolSelection,
    ) -> Result<()> {
        // Step 1: Get presignature from pool
        let presignature = if selection.requires_presignature_pool {
            self.cggmp24_presig_pool.take().await
                .ok_or(OrchestrationError::PresignaturePoolEmpty)?
        } else {
            // Fallback: Generate presignature on-demand (slower)
            self.generate_presignature_on_demand().await?
        };

        // Step 2: Perform CGGMP24 signing
        let signature = self.cggmp24_signing_engine
            .sign(tx, presignature)
            .await?;

        // Step 3: Verify signature type matches address
        verify_signature_type(&signature, &selection.address_class)?;

        // Step 4: Construct signed transaction
        let signed_tx = construct_signed_bitcoin_tx(
            &tx.unsigned_tx,
            &signature,
            SignatureType::ECDSA,
        )?;

        // Step 5: Store and transition state
        self.finalize_signing(tx, signed_tx).await
    }

    async fn sign_with_frost(
        &self,
        tx: &Transaction,
        selection: ProtocolSelection,
    ) -> Result<()> {
        // Step 1: FROST signing (no presignature pool yet)
        let signature = self.frost_signing_engine
            .sign(tx)
            .await?;

        // Step 2: Verify signature type matches address
        verify_signature_type(&signature, &selection.address_class)?;

        // Step 3: Construct signed transaction
        let signed_tx = construct_signed_bitcoin_tx(
            &tx.unsigned_tx,
            &signature,
            SignatureType::Schnorr,
        )?;

        // Step 4: Store and transition state
        self.finalize_signing(tx, signed_tx).await
    }
}

/// Verify signature algorithm matches address type
fn verify_signature_type(
    signature: &Signature,
    address_class: &BitcoinAddressClass,
) -> Result<()> {
    let expected_type = match address_class {
        BitcoinAddressClass::Taproot => SignatureType::Schnorr,
        _ => SignatureType::ECDSA,
    };

    let actual_type = signature.signature_type();

    if actual_type != expected_type {
        bail!(
            "Signature type mismatch: address requires {:?} but got {:?}",
            expected_type,
            actual_type
        );
    }

    Ok(())
}
```

---

#### 🚀 Advanced Feature: Multi-DKG Initialization

**Problem:** Users need to run **TWO separate DKG ceremonies** (CGGMP24 + FROST) to support all address types.

**Solution:** Parallel DKG initialization with both protocols.

```rust
// production/crates/api/src/handlers/dkg.rs

/// Initialize wallet with both CGGMP24 and FROST key shares
pub async fn initiate_multi_protocol_dkg(
    State(state): State<AppState>,
    Json(req): Json<MultiProtocolDkgRequest>,
) -> ApiResult<Json<MultiProtocolDkgResponse>> {
    info!("Starting multi-protocol DKG (CGGMP24 + FROST)...");

    // Step 1: Run CGGMP24 DKG
    info!("Running CGGMP24 DKG for ECDSA key generation...");
    let cggmp24_result = state.dkg_service
        .run_cggmp24_dkg(req.threshold, req.total_nodes)
        .await?;

    info!("CGGMP24 DKG completed: public_key={}", hex::encode(&cggmp24_result.public_key));

    // Step 2: Run FROST DKG (in parallel or sequential)
    info!("Running FROST DKG for Schnorr key generation...");
    let frost_result = state.dkg_service
        .run_frost_dkg(req.threshold, req.total_nodes)
        .await?;

    info!("FROST DKG completed: public_key={}", hex::encode(&frost_result.public_key));

    // Step 3: Derive Bitcoin addresses from both public keys
    let segwit_address = derive_p2wpkh_address(&cggmp24_result.public_key, req.network)?;
    let taproot_address = derive_p2tr_address(&frost_result.public_key, req.network)?;

    info!("Wallet initialized with dual addresses:");
    info!("  SegWit (P2WPKH):  {}", segwit_address);
    info!("  Taproot (P2TR):   {}", taproot_address);

    Ok(Json(MultiProtocolDkgResponse {
        success: true,
        cggmp24_public_key: hex::encode(&cggmp24_result.public_key),
        frost_public_key: hex::encode(&frost_result.public_key),
        segwit_address,
        taproot_address,
        supported_address_types: vec![
            "P2WPKH (Native SegWit)".to_string(),
            "P2TR (Taproot)".to_string(),
        ],
    }))
}

#[derive(Deserialize)]
pub struct MultiProtocolDkgRequest {
    threshold: u16,
    total_nodes: u16,
    network: bitcoin::Network,
}

#[derive(Serialize)]
pub struct MultiProtocolDkgResponse {
    success: bool,
    cggmp24_public_key: String,
    frost_public_key: String,
    segwit_address: String,
    taproot_address: String,
    supported_address_types: Vec<String>,
}
```

---

#### 🎨 User Experience: Transparent Protocol Switching

**CLI Commands:**

```bash
# Initialize wallet with both protocols
mpc-wallet-cli wallet init --threshold 4 --total 5 --dual-protocol

# Output:
# ✅ CGGMP24 DKG completed (ECDSA key)
# ✅ FROST DKG completed (Schnorr key)
#
# Your wallet addresses:
#   SegWit:  bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh
#   Taproot: bc1p5d7rjq7g6rdk2yhzks9smlaqtedr4dekq08ge8e5u2t3z4e

# Send to ANY address type (automatic protocol selection)
mpc-wallet-cli wallet send \
  --to bc1p5d7rjq7g6rdk2yhzks9smlaqtedr4dekq08 \
  --amount 0.001

# Output:
# 🔍 Detected: Taproot (P2TR) address
# ⚡ Selected: FROST protocol (Schnorr signature)
# ⏱️  Estimated signing time: 800ms
# ✅ Transaction signed and broadcast
# 📋 TXID: abc123def456...

# Send to SegWit (automatic switch to CGGMP24)
mpc-wallet-cli wallet send \
  --to bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh \
  --amount 0.001

# Output:
# 🔍 Detected: Native SegWit (P2WPKH) address
# ⚡ Selected: CGGMP24 protocol (ECDSA signature)
# ⏱️  Estimated signing time: 450ms (presignature pool)
# ✅ Transaction signed and broadcast
# 📋 TXID: def456abc789...
```

**API Endpoint:**

```bash
# POST /api/v1/wallet/send
curl -X POST http://localhost:8080/api/v1/wallet/send \
  -H "Content-Type: application/json" \
  -d '{
    "recipient": "bc1p5d7rjq7g6rdk2yhzks9smlaqtedr4dekq08",
    "amount_sats": 100000,
    "fee_rate": 10
  }'

# Response:
{
  "success": true,
  "txid": "abc123...",
  "protocol_used": "FROST",
  "address_type": "Taproot (P2TR)",
  "signature_type": "Schnorr",
  "signing_time_ms": 823
}
```

---

#### 📋 Database Schema Enhancements

```sql
-- Add protocol tracking to transactions table
ALTER TABLE transactions ADD COLUMN protocol_used TEXT;
ALTER TABLE transactions ADD COLUMN address_type TEXT;
ALTER TABLE transactions ADD COLUMN signature_type TEXT;

-- Add indexes for protocol statistics
CREATE INDEX idx_transactions_protocol ON transactions(protocol_used);
CREATE INDEX idx_transactions_address_type ON transactions(address_type);

-- Track DKG ceremonies for both protocols
ALTER TABLE dkg_ceremonies ADD COLUMN protocol TEXT NOT NULL; -- 'cggmp24' or 'frost'

-- Track presignature pool stats by protocol
CREATE TABLE presignature_pool_stats (
    id BIGSERIAL PRIMARY KEY,
    protocol TEXT NOT NULL,
    pool_size INTEGER NOT NULL,
    target_size INTEGER NOT NULL,
    utilization_percent INTEGER NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

---

#### 🧪 Testing Strategy

**Unit Tests:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_type_detection() {
        // SegWit
        let addr = "bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh";
        let class = BitcoinAddressClass::detect(addr, Network::Bitcoin).unwrap();
        assert_eq!(class, BitcoinAddressClass::NativeSegWit);
        assert_eq!(class.required_protocol(), ProtocolType::CGGMP24);

        // Taproot
        let addr = "bc1p5d7rjq7g6rdk2yhzks9smlaqtedr4dekq08ge8e5u2t3z4e";
        let class = BitcoinAddressClass::detect(addr, Network::Bitcoin).unwrap();
        assert_eq!(class, BitcoinAddressClass::Taproot);
        assert_eq!(class.required_protocol(), ProtocolType::FROST);

        // Legacy
        let addr = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa";
        let class = BitcoinAddressClass::detect(addr, Network::Bitcoin).unwrap();
        assert_eq!(class, BitcoinAddressClass::LegacyP2PKH);
        assert_eq!(class.required_protocol(), ProtocolType::CGGMP24);
    }

    #[test]
    fn test_invalid_address_rejection() {
        let addr = "invalid_address_123";
        let result = BitcoinAddressClass::detect(addr, Network::Bitcoin);
        assert!(result.is_err());
    }
}
```

**E2E Test:**
```rust
#[tokio::test]
async fn test_multi_protocol_transaction_flow() {
    let cluster = setup_5_node_cluster().await;

    // Initialize wallet with both protocols
    let dkg_response = cluster.nodes[0]
        .post("/api/v1/dkg/multi-protocol")
        .json(&json!({ "threshold": 4, "total_nodes": 5, "network": "testnet" }))
        .await?;

    assert!(dkg_response["success"].as_bool().unwrap());
    let segwit_addr = dkg_response["segwit_address"].as_str().unwrap();
    let taproot_addr = dkg_response["taproot_address"].as_str().unwrap();

    // Test 1: Send to Taproot (should use FROST)
    let tx1 = cluster.nodes[0]
        .post("/api/v1/wallet/send")
        .json(&json!({ "recipient": taproot_addr, "amount_sats": 10000 }))
        .await?;

    assert_eq!(tx1["protocol_used"], "FROST");
    assert_eq!(tx1["signature_type"], "Schnorr");

    // Test 2: Send to SegWit (should use CGGMP24)
    let tx2 = cluster.nodes[0]
        .post("/api/v1/wallet/send")
        .json(&json!({ "recipient": segwit_addr, "amount_sats": 10000 }))
        .await?;

    assert_eq!(tx2["protocol_used"], "CGGMP24");
    assert_eq!(tx2["signature_type"], "ECDSA");
}
```

---

#### 🎯 Performance Optimization: Dual Presignature Pools

**Future Enhancement (Post-MVP):**

Once FROST is fully integrated, implement presignature pool for FROST as well:

```rust
pub struct DualPresignatureManager {
    cggmp24_pool: PresignaturePool<Secp256k1, SecurityLevel128>,
    frost_pool: FrostPresignaturePool, // NEW

    // Pool management
    cggmp24_target: usize,  // 100
    frost_target: usize,    // 50 (less common, smaller pool)
}

impl DualPresignatureManager {
    pub async fn run_generation_loops(&self) {
        // Spawn two independent background tasks
        tokio::spawn(self.cggmp24_pool.generation_loop());
        tokio::spawn(self.frost_pool.generation_loop());
    }

    pub async fn get_presignature(&self, protocol: ProtocolType) -> Option<Presignature> {
        match protocol {
            ProtocolType::CGGMP24 => self.cggmp24_pool.take().await,
            ProtocolType::FROST => self.frost_pool.take().await,
        }
    }
}
```

**Performance Target with Dual Pools:**
- CGGMP24 signing: **<500ms** (with pool)
- FROST signing: **<600ms** (with pool, post-enhancement)

---

#### 📊 Monitoring & Observability

**Prometheus Metrics:**
```rust
// Protocol selection distribution
protocol_selection_total{protocol="cggmp24"} 152
protocol_selection_total{protocol="frost"} 48

// Address type distribution
address_type_total{type="native_segwit"} 120
address_type_total{type="taproot"} 48
address_type_total{type="legacy"} 32

// Signing performance by protocol
signing_duration_seconds{protocol="cggmp24",quantile="0.5"} 0.450
signing_duration_seconds{protocol="cggmp24",quantile="0.99"} 0.680
signing_duration_seconds{protocol="frost",quantile="0.5"} 0.800
signing_duration_seconds{protocol="frost",quantile="0.99"} 1.200

// Protocol availability
protocol_available{protocol="cggmp24"} 1
protocol_available{protocol="frost"} 1
```

---

#### 📁 Files to Create/Modify

**New Files:**
```
✨ production/crates/common/src/bitcoin_address.rs
✨ production/crates/orchestrator/src/protocol_router.rs
✨ production/crates/types/src/protocol.rs (ProtocolType enum)
✨ production/crates/api/src/handlers/multi_dkg.rs
```

**Modified Files:**
```
✏️ production/crates/orchestrator/src/service.rs (add protocol routing)
✏️ production/crates/orchestrator/src/lib.rs (export protocol_router)
✏️ production/crates/api/src/routes/dkg.rs (add multi-protocol endpoint)
✏️ production/crates/cli/src/commands/wallet.rs (add dual-protocol init)
✏️ production/docker/init-db/01_schema.sql (add columns)
```

---

#### ✅ Summary: Why This Is SOTA

**State-of-the-Art Features:**

1. ✅ **Zero Configuration**: User doesn't need to know about protocols
2. ✅ **Automatic Detection**: Parse recipient address → select protocol
3. ✅ **Multi-Protocol Support**: Both CGGMP24 and FROST in one wallet
4. ✅ **Performance Optimized**: Presignature pools for fast signing
5. ✅ **Type Safety**: Rust compiler ensures correct protocol selection
6. ✅ **Validation**: Verify signature type before broadcast
7. ✅ **Observability**: Track protocol usage with metrics
8. ✅ **Future-Proof**: Easy to add MuSig2, ROAST, etc.
9. ✅ **User-Friendly**: Transparent protocol switching
10. ✅ **Fail-Safe**: Explicit errors if missing key shares

**User Experience:**
```
User sends to: bc1p5d7r...  (Taproot address)
System automatically:
  1. Detects Taproot address type
  2. Selects FROST protocol
  3. Uses Schnorr signature
  4. Verifies signature matches address
  5. Broadcasts transaction

User doesn't need to know ANY of this! 🎉
```

---

### **PRIORITY 3: FROST Signing Integration** - HIGH

**Effort:** 3-4 days
**Complexity:** High
**Optional:** Can use CGGMP24 only for MVP

#### Why Important?
For **Taproot support** (modern Bitcoin). Not critical for MVP (can use SegWit only).

**Source Code:** `torcus-wallet/crates/protocols/src/frost/signing.rs`
**Target:** `production/crates/orchestrator/src/signing_coordinator.rs` (NEW)
**Library:** `givre` v0.2 (https://github.com/LFDT-Lockness/givre)

**Implementation:**
- Similar to CGGMP24 signing integration
- Use `givre::signing` module for FROST signing
- **Key Feature:** Uses same key shares from `givre::keygen` (from Priority 0)
- No presignature pool yet (FROST doesn't require presigs - simpler protocol)
- Produces BIP-340 compliant Schnorr signatures for Taproot
- **Faster than CGGMP24:** ~800ms signing time (no presig pool needed)

**Givre FROST Signing Flow:**
```rust
use givre::signing::{SigningParty, SigningRound};
use givre::ciphersuite::Secp256k1;

impl SigningCoordinator {
    pub async fn sign_with_frost(
        &self,
        message: &[u8],
        key_share: &KeyShare<Secp256k1>,
    ) -> Result<SchnorrSignature> {
        // 1. Initialize FROST signing round (2 rounds total)
        let mut party = SigningParty::new(
            key_share.clone(),
            self.threshold,
            self.party_index,
        )?;

        // 2. Round 1: Generate commitment (nonce commitment)
        let round1_msg = party.create_round1_message()?;
        self.broadcast_message(round1_msg).await?;

        // 3. Collect commitments from other parties
        let commitments = self.collect_round1_messages().await?;

        // 4. Round 2: Generate signature share
        let round2_msg = party.create_round2_message(message, &commitments)?;
        self.broadcast_message(round2_msg).await?;

        // 5. Collect signature shares and aggregate
        let sig_shares = self.collect_round2_messages().await?;
        let signature = party.aggregate_signatures(&sig_shares)?;

        // 6. Verify BIP-340 compliance
        assert!(signature.verify_bip340(message, &key_share.public_key()));

        Ok(signature)
    }
}
```

**Why Givre for FROST Signing?**
- **Integrated Solution:** Uses same key shares from givre DKG (Priority 0)
- **Fast:** Only 2 rounds (~800ms), no presignature pool needed
- **BIP-340 Native:** Built-in Taproot/Schnorr support
- **Simpler than CGGMP24:** No auxiliary info or presignatures required

**API Route Selection:**
```rust
impl TransactionOrchestrator {
    async fn select_signing_protocol(&self, tx: &Transaction) -> ProtocolType {
        // Check Bitcoin address type
        let address = bitcoin::Address::from_str(&tx.recipient)?;

        match address.address_type()? {
            AddressType::P2tr => ProtocolType::FROST,  // Taproot
            _ => ProtocolType::CGGMP24,                // SegWit
        }
    }
}
```

**Files to Modify:**
```
✏️ production/crates/orchestrator/src/signing_coordinator.rs (NEW)
✏️ production/crates/orchestrator/src/service.rs (add FROST route)
✏️ production/crates/types/src/protocol.rs (add ProtocolType enum)
```

---

### **PRIORITY 4: QUIC Vote Broadcasting** - MEDIUM

**Effort:** 2-3 days
**Complexity:** Medium
**Current Workaround:** Manual SQL insertion works

#### Why Needed?
Currently, **votes must be manually inserted into PostgreSQL**. Nodes should automatically cast votes via QUIC network.

**Current Status:** Voting threshold detection **WORKS** (tested), but votes are inserted manually.

**Implementation:**

1. **Vote Request Broadcast**
   ```rust
   // production/crates/orchestrator/src/service.rs
   async fn initiate_voting(&self, tx: &Transaction) -> Result<()> {
       // Create voting round
       let round_id = self.create_voting_round(tx).await?;

       // Broadcast vote request via QUIC
       let vote_request = VoteRequest {
           tx_id: tx.txid.clone(),
           round_id,
           unsigned_tx: tx.unsigned_tx.clone(),
           amount_sats: tx.amount_sats,
           fee_sats: tx.fee_sats,
           recipient: tx.recipient.clone(),
       };

       let msg = ProtocolMessage::VoteRequest(vote_request);
       self.quic_engine.broadcast_to_all(msg).await?;

       info!("Broadcast vote request for tx: {:?}", tx.txid);
   }
   ```

2. **Node-Side Vote Handler**
   ```rust
   // production/crates/protocols/src/p2p/message_handler.rs
   async fn handle_vote_request(&self, request: VoteRequest) -> Result<()> {
       // Validate transaction
       let is_valid = self.validate_transaction(&request).await?;

       // Create vote
       let vote = Vote {
           round_id: request.round_id,
           node_id: self.node_id,
           tx_id: request.tx_id.clone(),
           approve: is_valid,
           signature: self.sign_vote(&request)?,
       };

       // Send vote back to coordinator via QUIC
       let msg = ProtocolMessage::VoteResponse(vote);
       self.quic_engine.send_to_coordinator(msg).await?;

       info!("Cast vote for tx {:?}: {}", request.tx_id, if is_valid { "APPROVE" } else { "REJECT" });
   }
   ```

3. **Vote Collection**
   ```rust
   async fn collect_votes(&self, round_id: i64, timeout: Duration) -> Result<Vec<Vote>> {
       let mut votes = Vec::new();
       let start = Instant::now();

       while votes.len() < self.total_nodes && start.elapsed() < timeout {
           if let Some(msg) = self.quic_engine.recv_message().await? {
               if let ProtocolMessage::VoteResponse(vote) = msg {
                   if vote.round_id == round_id {
                       // Insert vote into PostgreSQL
                       self.postgres.insert_vote(&vote).await?;
                       votes.push(vote);
                   }
               }
           }
           tokio::time::sleep(Duration::from_millis(100)).await;
       }

       Ok(votes)
   }
   ```

**Files to Modify:**
```
✏️ production/crates/types/src/messages.rs (add VoteRequest, VoteResponse)
✏️ production/crates/orchestrator/src/service.rs (add initiate_voting, collect_votes)
✏️ production/crates/protocols/src/p2p/message_handler.rs (add handle_vote_request)
```

---

### **PRIORITY 5: Signature Verification** - MEDIUM

**Effort:** 1-2 days
**Complexity:** Low-Medium
**Current Status:** TODO in crypto crate

**File:** `production/crates/crypto/src/lib.rs:5`

**Implementation:**

```rust
// production/crates/crypto/src/lib.rs
use ed25519_dalek::{Verifier, Signature, VerifyingKey};
use k256::ecdsa::{signature::Verifier as EcdsaVerifier, Signature as EcdsaSignature, VerifyingKey as EcdsaVerifyingKey};

/// Verify Ed25519 signature (for vote signatures)
pub fn verify_ed25519_signature(
    public_key: &[u8; 32],
    message: &[u8],
    signature: &[u8; 64],
) -> Result<(), CryptoError> {
    let verifying_key = VerifyingKey::from_bytes(public_key)
        .map_err(|e| CryptoError::InvalidPublicKey(e.to_string()))?;

    let sig = Signature::from_bytes(signature);

    verifying_key.verify(message, &sig)
        .map_err(|e| CryptoError::InvalidSignature(e.to_string()))
}

/// Verify ECDSA signature (for Bitcoin transactions)
pub fn verify_ecdsa_signature(
    public_key: &[u8],
    message_hash: &[u8; 32],
    signature: &[u8],
) -> Result<(), CryptoError> {
    let verifying_key = EcdsaVerifyingKey::from_sec1_bytes(public_key)
        .map_err(|e| CryptoError::InvalidPublicKey(e.to_string()))?;

    let sig = EcdsaSignature::from_der(signature)
        .map_err(|e| CryptoError::InvalidSignature(e.to_string()))?;

    verifying_key.verify(message_hash, &sig)
        .map_err(|e| CryptoError::InvalidSignature(e.to_string()))
}

/// Verify Bitcoin transaction signature
pub fn verify_bitcoin_signature(
    signed_tx: &[u8],
    public_key: &[u8],
) -> Result<(), CryptoError> {
    // Parse transaction
    // Extract signature from witness/scriptSig
    // Compute sighash
    // Verify ECDSA signature
    todo!("Implement Bitcoin signature verification using bitcoin crate")
}
```

**Files to Modify:**
```
✏️ production/crates/crypto/src/lib.rs (implement all functions)
```

---

### **PRIORITY 6: mTLS Certificate Validation** - LOW

**Effort:** 1 day
**Complexity:** Low
**Current Status:** Multiple TODOs in network crate

**Files:** `production/crates/network/src/quic_listener.rs` (lines 157, 170, 200, 260)

**TODOs:**
1. Extract node ID from peer TLS certificate
2. Validate message sender matches authenticated node
3. Close connections without valid certificates

**Implementation:**

```rust
// production/crates/network/src/quic_listener.rs

/// Extract node ID from peer certificate
fn extract_node_id_from_cert(peer_cert: &Certificate) -> Result<NodeId> {
    // Parse X.509 certificate
    let cert = x509_parser::parse_x509_certificate(&peer_cert.0)?;

    // Extract Common Name (CN) from Subject
    let subject = cert.1.subject();
    let cn = subject.iter_common_name()
        .next()
        .ok_or(NetworkError::NoCNInCertificate)?;

    // CN format: "node-1" -> NodeId(0)
    let node_id_str = cn.as_str()?;
    parse_node_id(node_id_str)
}

/// Validate message sender
fn validate_message_sender(
    msg: &ProtocolMessage,
    authenticated_node_id: NodeId,
) -> Result<()> {
    if msg.sender_node_id != authenticated_node_id {
        return Err(NetworkError::SenderMismatch {
            claimed: msg.sender_node_id,
            authenticated: authenticated_node_id,
        });
    }
    Ok(())
}
```

**Files to Modify:**
```
✏️ production/crates/network/src/quic_listener.rs (implement all TODOs)
```

---

## 📦 Source Code Mapping

### From `torcus-wallet` → `production`

| Source File | Target Location | Status |
|------------|----------------|--------|
| `crates/protocols/src/frost/keygen.rs` | `production/crates/protocols/src/frost/keygen.rs` | ✅ Copied |
| `crates/protocols/src/frost/signing.rs` | `production/crates/protocols/src/frost/signing.rs` | ✅ Copied |
| `crates/protocols/src/frost/mod.rs` | `production/crates/protocols/src/frost/mod.rs` | ✅ Copied |

**Integration:** ❌ Not yet integrated with orchestrator

### From `threshold-signing (Copy)` → `production`

| Source File | Target Location | Status |
|------------|----------------|--------|
| `node/src/keygen.rs` | `production/crates/protocols/src/cggmp24/keygen.rs` | ✅ Copied |
| `node/src/presignature.rs` | `production/crates/protocols/src/cggmp24/presignature.rs` | ✅ Copied |
| `node/src/presignature_pool.rs` | `production/crates/protocols/src/cggmp24/presig_pool.rs` | ✅ Copied |
| `node/src/signing.rs` | `production/crates/protocols/src/cggmp24/signing.rs` | ✅ Copied |
| `node/src/signing_fast.rs` | `production/crates/protocols/src/cggmp24/signing_fast.rs` | ✅ Copied |

**Integration:** ❌ Not yet integrated with orchestrator

---

## 🗺️ Implementation Roadmap (Timeline)

### **Week 1: Core Cryptography (P0)**

**Days 1-4: DKG Implementation**
- [ ] Create DkgService orchestrator
- [ ] Implement CGGMP24 DKG integration
- [ ] Implement FROST DKG integration
- [ ] Add QUIC message routing for DKG
- [ ] Create API endpoints + CLI commands
- [ ] Add database schema
- [ ] Write E2E tests

**Days 5-7: Presignature Pool**
- [ ] Create PresignatureService
- [ ] Implement background generation loop
- [ ] Add presignature storage (PostgreSQL)
- [ ] Create API endpoints + CLI commands
- [ ] Test pool refill logic

### **Week 2: Signing Integration (P2-P3)**

**Days 8-12: CGGMP24 Fast Signing**
- [ ] Remove mock signatures from orchestrator
- [ ] Implement CGGMP signing coordination
- [ ] Add signature share collection
- [ ] Create node-side signing handler
- [ ] Implement signature combination
- [ ] Add Bitcoin transaction construction
- [ ] Test end-to-end signing

**Days 13-14: FROST Signing (Optional)**
- [ ] Implement FROST signing coordination
- [ ] Add protocol selection logic
- [ ] Test Taproot signatures

### **Week 3: Communication & Finalization (P4-P6)**

**Days 15-17: QUIC Vote Broadcasting**
- [ ] Implement vote request broadcast
- [ ] Add node-side vote handlers
- [ ] Create vote collection logic
- [ ] Test automatic voting

**Day 18: Signature Verification**
- [ ] Implement Ed25519 verification (votes)
- [ ] Implement ECDSA verification (Bitcoin)
- [ ] Add Bitcoin signature verification

**Day 19: mTLS Validation**
- [ ] Extract node ID from certificates
- [ ] Add sender validation
- [ ] Handle invalid certificates

**Days 20-21: Testing & Integration**
- [ ] Run full E2E test suite
- [ ] Performance tuning
- [ ] Fix any bugs
- [ ] Documentation updates

---

## 📊 Work Estimate Summary

| Priority | Component | Effort | Developer | Parallel? |
|----------|-----------|--------|-----------|-----------|
| P0 | DKG Implementation | 3-4 days | Dev 1 | ❌ Sequential |
| P0 | Presignature Pool | 3-4 days | Dev 1 | ❌ Sequential |
| P2 | CGGMP24 Signing | 4-5 days | Dev 2 | ✅ Can parallelize |
| P3 | FROST Signing | 3-4 days | Dev 2 | ✅ After CGGMP |
| P4 | QUIC Vote Broadcast | 2-3 days | Dev 3 | ✅ Parallel |
| P5 | Signature Verification | 1-2 days | Dev 3 | ✅ Parallel |
| P6 | mTLS Validation | 1 day | Dev 3 | ✅ Parallel |

**Total Sequential:** ~20-25 days (1 developer)
**Total Parallel (3 devs):** ~12-15 days
**Minimum Viable (P0 only):** ~7-8 days

---

## 🎯 Minimum Viable Product (MVP) Definition

To have a **working MPC wallet**, you MUST implement:

### ✅ MVP Requirements (P0)
1. ✅ DKG (Distributed Key Generation)
2. ✅ Presignature Pool Management
3. ✅ CGGMP24 Real Signing (remove mock)

### Optional for MVP (Can add later)
- ⏸️ FROST signing (use CGGMP24 only)
- ⏸️ QUIC vote broadcasting (manual SQL works)
- ⏸️ Full mTLS validation

**MVP Timeline:** ~10-12 days (single developer)

---

## 🚨 Critical Dependencies

```
DKG Implementation (P0)
    ↓
Presignature Pool (P0) ← requires key shares from DKG
    ↓
CGGMP Signing (P2) ← requires presignatures
    ↓
✅ System Functional

Parallel:
├─ FROST Signing (P3) ← requires DKG
├─ QUIC Vote Broadcast (P4) ← independent
└─ Signature Verification (P5) ← independent
```

**Critical Path:** DKG → Presig Pool → CGGMP Signing (10-13 days)

---

## 📝 Files Summary

### New Files to Create
```
✨ production/crates/orchestrator/src/dkg_service.rs
✨ production/crates/orchestrator/src/presig_service.rs
✨ production/crates/orchestrator/src/signing_coordinator.rs
✨ production/crates/api/src/handlers/dkg.rs
✨ production/crates/api/src/handlers/presig.rs
✨ production/crates/api/src/routes/dkg.rs
✨ production/crates/api/src/routes/presig.rs
```

### Files to Modify
```
✏️ production/crates/orchestrator/src/service.rs (replace mock signing)
✏️ production/crates/api/src/bin/server.rs (add services)
✏️ production/crates/types/src/messages.rs (add protocol messages)
✏️ production/crates/protocols/src/p2p/message_handler.rs (add handlers)
✏️ production/crates/cli/src/commands/dkg.rs (implement real API calls)
✏️ production/crates/cli/src/commands/presig.rs (implement real API calls)
✏️ production/crates/crypto/src/lib.rs (implement verification)
✏️ production/crates/network/src/quic_listener.rs (implement TODOs)
✏️ production/docker/init-db/01_schema.sql (add tables)
```

---

## ✅ What's Already Done (Don't Redo)

The following components are **COMPLETE** and working:

1. ✅ **Infrastructure:**
   - Docker deployment (5 nodes + etcd + PostgreSQL)
   - QUIC+mTLS transport
   - PostgreSQL schema (9 tables)
   - etcd coordination

2. ✅ **Orchestration:**
   - Transaction lifecycle state machine
   - Automatic state transitions (pending → voting → approved → signing → signed)
   - Byzantine vote detection (database constraints)
   - Timeout monitoring
   - Health checks

3. ✅ **API:**
   - REST API (Axum)
   - All transaction endpoints
   - Health endpoints
   - CORS configured

4. ✅ **CLI:**
   - Transaction commands
   - Wallet commands
   - Cluster status

5. ✅ **Protocol Code:**
   - CGGMP24 implementation copied from threshold-signing
   - FROST implementation copied from torcus-wallet
   - Presignature pool structure exists

**What's Missing:** Integration of protocols with orchestrator + DKG + Presig generation

---

## 🎓 Learning Resources

### CGGMP24 Protocol
- Paper: "UC Non-Interactive, Proactive, Threshold ECDSA" (2024)
- Library: https://github.com/ZenGo-X/cggmp24
- Tutorial: Check threshold-signing (Copy) README

### FROST Protocol
- Paper: "FROST: Flexible Round-Optimized Schnorr Threshold Signatures" (2020)
- **Givre Library (USED IN THIS PROJECT):** https://github.com/LFDT-Lockness/givre
  - **Maintainer:** LFDT-Lockness (Lattice-Based Fully Distributed Threshold)
  - **Version:** 0.2
  - **Features:** CGGMP21 DKG + FROST signing + Bitcoin secp256k1 support
  - **Docs:** https://docs.rs/givre/latest/givre/
- Alternative Library: https://github.com/ZcashFoundation/frost (Zcash implementation, not used here)

### Bitcoin Cryptography
- BIP-340 (Schnorr): https://github.com/bitcoin/bips/blob/master/bip-0340.mediawiki
- BIP-341 (Taproot): https://github.com/bitcoin/bips/blob/master/bip-0341.mediawiki
- Rust Library: https://docs.rs/bitcoin/latest/bitcoin/

---

## 🔍 Testing Strategy

### Unit Tests
- Test presignature pool operations
- Test signature combination logic
- Test vote validation

### Integration Tests
- Test DKG across 5 nodes
- Test presignature generation batch
- Test CGGMP signing end-to-end
- Test FROST signing end-to-end

### E2E Tests
- Test full transaction lifecycle with real signatures
- Test Byzantine node behavior during signing
- Test presignature pool refill under load
- Test failover scenarios

---

## 📞 Support & Questions

If you have questions during implementation:

1. **DKG Issues:** Check `threshold-signing (Copy)/node/src/keygen.rs` for reference
2. **Presignature Issues:** Check `threshold-signing (Copy)/node/src/presignature_pool.rs`
3. **FROST Issues:** Check `torcus-wallet/crates/protocols/src/frost/`
4. **QUIC Issues:** Check existing `production/crates/network/src/quic_engine.rs`

---

---

## 🔧 Cargo Dependencies Reference

### Required Dependencies (Already in Cargo.toml)

```toml
[workspace.dependencies]
# Threshold cryptography - CGGMP24 for ECDSA (SegWit)
# Source: threshold-signing (Copy)/node/src/
cggmp24 = { version = "0.7.0-alpha.3" }

# Threshold cryptography - FROST for Schnorr (Taproot)
# Source: torcus-wallet/crates/protocols/src/frost/
# ONLY for FROST - DO NOT use for CGGMP24!
givre = {
    version = "0.2",
    features = [
        "full-signing",        # Complete FROST DKG + signing
        "serde",               # Serialization for network
        "ciphersuite-bitcoin"  # Bitcoin secp256k1 curve
    ]
}

# Bitcoin primitives
bitcoin = { version = "0.31", features = ["serde", "rand"] }

# Networking
quinn = "0.10"                 # QUIC implementation
rustls = { version = "0.21", features = ["dangerous_configuration"] }
rcgen = "0.11"                 # Certificate generation

# Storage
tokio-postgres = "0.7"
etcd-client = "0.12"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
bincode = "1.3"

# Async runtime
tokio = { version = "1", features = ["full"] }

# Error handling
anyhow = "1.0"
thiserror = "1.0"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

---

## 🐛 Troubleshooting Guide

### Common Issues During Implementation

#### 1. DKG Fails to Complete

**Symptom:** DKG ceremony hangs or times out

**Possible Causes:**
- Network connectivity issues between nodes
- Insufficient threshold (need at least `threshold` nodes online)
- etcd lock contention

**Solution:**
```bash
# Check all nodes are reachable
mpc-wallet-cli cluster status

# Verify etcd connectivity
etcdctl get /cluster/nodes --prefix

# Check DKG logs
docker logs node-1 | grep -i "dkg"

# Release stuck locks
etcdctl del /locks/dkg
```

#### 2. Presignature Pool Empty

**Symptom:** Signing fails with "PresignaturePoolEmpty" error

**Possible Causes:**
- Background generation service not started
- Generation rate slower than consumption
- DKG not completed (no key shares)

**Solution:**
```bash
# Check pool status
mpc-wallet-cli presig status

# Manually generate presignatures
mpc-wallet-cli presig generate --count 50

# Verify key shares exist
psql -c "SELECT * FROM key_shares LIMIT 5;"

# Check presig generation logs
docker logs node-1 | grep -i "presignature"
```

#### 3. Signature Verification Fails

**Symptom:** Bitcoin node rejects transaction with "bad-txns-in-belowout" or "mandatory-script-verify-flag-failed"

**Possible Causes:**
- Using wrong protocol for address type (ECDSA for Taproot or vice versa)
- Signature shares combined incorrectly
- Transaction malformed

**Solution:**
```bash
# Verify protocol selection
curl http://localhost:3001/api/v1/transactions/{txid} | jq '.protocol'

# Test signature locally before broadcast
bitcoin-cli testmempoolaccept '["<signed_tx_hex>"]'

# Check if address type matches protocol
# Taproot (bc1p...) = FROST/Schnorr
# SegWit (bc1q...) = CGGMP24/ECDSA
```

#### 4. QUIC Connection Failures

**Symptom:** Nodes cannot communicate, "connection refused" errors

**Possible Causes:**
- mTLS certificate mismatch
- Port not exposed in Docker
- Firewall blocking UDP traffic (QUIC uses UDP)

**Solution:**
```bash
# Verify QUIC ports are open
docker ps | grep "50051-50055"

# Check certificate validity
openssl x509 -in certs/node-1.crt -text -noout

# Test QUIC connectivity
nc -u -v node-2 50052

# Check QUIC logs
docker logs node-1 | grep -i "quic\|connection"
```

#### 5. Givre Library Errors

**Symptom:** "unsupported curve" or "invalid ciphersuite" errors

**Possible Causes:**
- Missing `ciphersuite-bitcoin` feature flag
- Using wrong curve (not secp256k1)
- Incompatible givre version

**Solution:**
```toml
# Verify Cargo.toml has correct features
givre = {
    version = "0.2",
    features = ["cggmp21-keygen", "full-signing", "serde", "ciphersuite-bitcoin"]
}
```

```rust
// Ensure using Bitcoin ciphersuite
use givre::ciphersuite::Secp256k1;

// NOT this:
// use givre::ciphersuite::Secp256r1; // Wrong curve!
```

#### 6. Database Schema Errors

**Symptom:** "relation does not exist" or "column not found"

**Possible Causes:**
- Database migrations not applied
- Schema out of sync with code

**Solution:**
```bash
# Re-initialize database
docker-compose down -v
docker-compose up -d postgres

# Verify tables exist
psql -c "\dt"

# Check specific table
psql -c "\d dkg_ceremonies"
```

---

## 📚 API Examples

### Complete DKG Flow (CGGMP24)

```rust
use cggmp24::{
    key_share::{DirtyKeyShare, PartyAux},
    supported_curves::Secp256k1,
    security_level::SecurityLevel128,
};

// Round 1: Key generation setup
let eid = cggmp24::ExecutionId::new(b"dkg-session-001");
let i = PartyIndex::from_usize(0); // Party 0
let n = NumberOfParties::from_usize(5);
let t = Threshold::from_usize(4);

let mut party = cggmp24::keygen::KeygenParty::<Secp256k1, SecurityLevel128>::new(
    i, n, t, eid
)?;

// Round 1: Broadcast commitment
let round1_msg = party.round1()?;
broadcast_to_all(round1_msg).await?;

// Round 2: Receive commitments, send shares
let commitments = collect_messages().await?;
let round2_msg = party.round2(&commitments)?;
broadcast_to_all(round2_msg).await?;

// Rounds 3-5: Continue DKG protocol...
// (Full implementation in production/crates/protocols/src/cggmp24/keygen.rs)

// Final: Extract key share
let key_share: DirtyKeyShare<Secp256k1, SecurityLevel128> = party.finalize()?;
let public_key = key_share.shared_public_key();
```

### Complete FROST DKG Flow (Givre)

**IMPORTANT:** This is **ONLY for FROST** (Taproot/Schnorr). For CGGMP24 (SegWit/ECDSA), use the `cggmp24` library instead!

```rust
use givre::{
    ciphersuite::Secp256k1,
    key_share::KeyShare,
};

// Initialize FROST-native DKG
// Source: torcus-wallet/crates/protocols/src/frost/keygen.rs
let party_index = 1u16;
let threshold = 4u16;
let total_parties = 5u16;

// Round 1: Generate and broadcast commitments
let (round1_secret, round1_msg) = givre::keygen::round1::<Secp256k1>(
    party_index,
    threshold,
    total_parties,
)?;

broadcast_message(round1_msg).await?;

// Round 2: Collect commitments and generate shares
let round1_messages = collect_messages().await?;
let (key_share, round2_msg) = givre::keygen::round2::<Secp256k1>(
    round1_secret,
    round1_messages,
)?;

broadcast_message(round2_msg).await?;

// Round 3: Finalize key share
let round2_messages = collect_messages().await?;
let final_key_share: KeyShare<Secp256k1> = givre::keygen::finalize(
    key_share,
    round2_messages,
)?;

// Extract BIP-340 x-only public key for Taproot
let x_only_pubkey = final_key_share.public_key().x_only();
let taproot_address = create_taproot_address(x_only_pubkey)?;
```

**Key Points:**
- This is **FROST-native DKG**, not CGGMP21-based
- Used **ONLY** for Taproot (bc1p...) addresses
- For SegWit (bc1q...), use `cggmp24` library DKG instead

### FROST Signing with Givre

```rust
use givre::signing::{SigningParty, SigningRound};

// Prepare message (Bitcoin transaction hash)
let message: [u8; 32] = tx_hash.to_byte_array();

// Round 1: Generate nonce commitments
let mut party = SigningParty::<Secp256k1>::new(
    key_share.clone(),
    party_index,
    threshold,
)?;

let round1_msg = party.round1(&message)?;
broadcast_message(round1_msg).await?;

// Round 2: Generate signature share
let round1_messages = collect_messages().await?;
let round2_msg = party.round2(round1_messages)?;
broadcast_message(round2_msg).await?;

// Aggregate: Combine signature shares
let round2_messages = collect_messages().await?;
let signature = party.aggregate(round2_messages)?;

// Verify BIP-340 Schnorr signature
assert!(signature.verify_bip340(&message, &key_share.public_key()));
```

---

## 🎯 Quick Start Checklist

### ⚠️ CRITICAL: Library Usage Rules

**DO NOT MIX LIBRARIES!**
- ✅ **CGGMP24 (SegWit)**: Use `cggmp24` library → Source: `threshold-signing (Copy)/`
- ✅ **FROST (Taproot)**: Use `givre` library → Source: `torcus-wallet/`
- ❌ **DO NOT** use Givre for CGGMP24
- ❌ **DO NOT** use cggmp24 for FROST

### Pre-Implementation Checklist

Before starting implementation, ensure:

- [ ] **Library Understanding:**
  - [ ] Understand `cggmp24` is for SegWit (ECDSA)
  - [ ] Understand `givre` is for Taproot (Schnorr)
  - [ ] Never mix the two libraries
- [ ] All dependencies installed in `Cargo.toml`
- [ ] Docker environment running (5 nodes + etcd + PostgreSQL)
- [ ] Database schema initialized (`01_schema.sql` applied)
- [ ] QUIC+mTLS certificates generated
- [ ] Network connectivity between nodes verified
- [ ] Source code copied from `torcus-wallet` and `threshold-signing (Copy)`
- [ ] Read Protocol documentation:
  - [ ] CGGMP24 paper
  - [ ] FROST paper
  - [ ] Givre library docs: https://github.com/LFDT-Lockness/givre
  - [ ] BIP-340 (Schnorr)
  - [ ] BIP-341 (Taproot)

---

---

## 🚀 Next Steps: Where to Start

### Day 1 Morning: Setup & Planning (2 hours)

1. **Read Documentation** (1 hour)
   - [ ] Read CGGMP24 paper introduction
   - [ ] Read Givre library README: https://github.com/LFDT-Lockness/givre
   - [ ] Skim BIP-340 (Schnorr): https://github.com/bitcoin/bips/blob/master/bip-0340.mediawiki

2. **Verify Environment** (30 minutes)
   ```bash
   cd production
   docker-compose up -d
   docker ps  # Should show 5 nodes + etcd + postgres
   cargo build  # Verify all dependencies compile
   ```

3. **Review Existing Code** (30 minutes)
   - [ ] Read: [production/crates/protocols/src/cggmp24/keygen.rs](production/crates/protocols/src/cggmp24/keygen.rs)
   - [ ] Read: [production/crates/protocols/src/frost/keygen.rs](production/crates/protocols/src/frost/keygen.rs)
   - [ ] Read: [production/crates/orchestrator/src/service.rs](production/crates/orchestrator/src/service.rs) (lines 388-533, mock signing)

### Day 1 Afternoon: Start DKG Implementation (4-6 hours)

**FIRST FILE TO CREATE:** `production/crates/orchestrator/src/dkg_service.rs`

```bash
# Create the file
touch production/crates/orchestrator/src/dkg_service.rs
```

**Start with this template:**

```rust
// production/crates/orchestrator/src/dkg_service.rs

use std::sync::Arc;
use threshold_storage::{EtcdStorage, PostgresStorage};
use threshold_network::QuicEngine;
use threshold_types::{NodeId, ProtocolType, Result};

/// Service for coordinating Distributed Key Generation across MPC nodes
pub struct DkgService {
    postgres: Arc<PostgresStorage>,
    etcd: Arc<EtcdStorage>,
    quic: Arc<QuicEngine>,
    node_id: NodeId,
}

impl DkgService {
    pub fn new(
        postgres: Arc<PostgresStorage>,
        etcd: Arc<EtcdStorage>,
        quic: Arc<QuicEngine>,
        node_id: NodeId,
    ) -> Self {
        Self {
            postgres,
            etcd,
            quic,
            node_id,
        }
    }

    /// Run CGGMP24 DKG for ECDSA key generation (SegWit addresses)
    /// Uses cggmp24 library from threshold-signing (Copy) folder
    pub async fn run_cggmp24_dkg(
        &self,
        threshold: u16,
        total_parties: u16,
    ) -> Result<DkgResult> {
        // TODO: Implement CGGMP24 DKG
        // Source: threshold-signing (Copy)/node/src/keygen.rs
        // Already copied to: production/crates/protocols/src/cggmp24/keygen.rs
        // Library: cggmp24 v0.7.0-alpha.3
        todo!("Day 1-2: Implement CGGMP24 DKG coordination")
    }

    /// Run FROST DKG using Givre for Schnorr key generation (Taproot addresses)
    /// Uses givre library from torcus-wallet folder
    /// IMPORTANT: This is FROST-native DKG, NOT CGGMP21!
    pub async fn run_frost_dkg(
        &self,
        threshold: u16,
        total_parties: u16,
    ) -> Result<DkgResult> {
        // TODO: Implement FROST DKG using Givre
        // Source: torcus-wallet/crates/protocols/src/frost/keygen.rs
        // Already copied to: production/crates/protocols/src/frost/keygen.rs
        // Library: givre v0.2
        // Reference: https://github.com/LFDT-Lockness/givre
        todo!("Day 3: Implement FROST DKG using Givre")
    }
}

pub struct DkgResult {
    pub public_key: Vec<u8>,
    pub protocol: ProtocolType,
}
```

**Then modify:** `production/crates/orchestrator/src/lib.rs`

```rust
// Add this line
pub mod dkg_service;

// Re-export
pub use dkg_service::{DkgService, DkgResult};
```

### Day 1-2: Complete CGGMP24 DKG (Detailed Steps)

Follow this exact sequence:

1. **Create QUIC message types** (2 hours)
   - File: [production/crates/types/src/messages.rs](production/crates/types/src/messages.rs)
   - Add: `DkgMessage` enum with all DKG rounds

2. **Implement DKG coordination** (4 hours)
   - File: [production/crates/orchestrator/src/dkg_service.rs](production/crates/orchestrator/src/dkg_service.rs)
   - Copy logic from: `threshold-signing (Copy)/node/src/keygen.rs`
   - Adapt to work with QUIC messaging

3. **Create API endpoints** (2 hours)
   - File: [production/crates/api/src/handlers/dkg.rs](production/crates/api/src/handlers/dkg.rs) (NEW)
   - File: [production/crates/api/src/routes/dkg.rs](production/crates/api/src/routes/dkg.rs) (NEW)

4. **Add database schema** (1 hour)
   - File: [production/docker/init-db/01_schema.sql](production/docker/init-db/01_schema.sql)
   - Add: `dkg_ceremonies`, `key_shares` tables

5. **Test DKG** (2 hours)
   - Create E2E test in `production/e2e/dkg_test.rs`
   - Run: `cargo test --test dkg_test`

### Reference Files (Keep Open While Coding)

Always have these files open in separate tabs:

1. **Source Code Reference:**
   - [threshold-signing (Copy)/node/src/keygen.rs](threshold-signing (Copy)/node/src/keygen.rs) - CGGMP24 DKG reference
   - [torcus-wallet/crates/protocols/src/frost/keygen.rs](torcus-wallet/crates/protocols/src/frost/keygen.rs) - FROST DKG reference

2. **Existing Infrastructure:**
   - [production/crates/orchestrator/src/service.rs](production/crates/orchestrator/src/service.rs) - Orchestration patterns
   - [production/crates/network/src/quic_engine.rs](production/crates/network/src/quic_engine.rs) - QUIC messaging

3. **Type Definitions:**
   - [production/crates/types/src/lib.rs](production/crates/types/src/lib.rs) - Shared types
   - [production/crates/types/src/messages.rs](production/crates/types/src/messages.rs) - Protocol messages

### Daily Progress Tracking

**Day 1:**
- [ ] Environment verified
- [ ] DkgService file created
- [ ] CGGMP24 DKG message types added
- [ ] DKG coordination logic started

**Day 2:**
- [ ] CGGMP24 DKG coordination completed
- [ ] API endpoints created
- [ ] Database schema updated
- [ ] Basic DKG test passing

**Day 3:**
- [ ] FROST DKG (Givre) integration
- [ ] Both protocols tested end-to-end
- [ ] Documentation updated

**Day 4:**
- [ ] DKG refinement and error handling
- [ ] Start Presignature Pool (Priority 1)

---

**Document Version:** 2.0
**Last Updated:** 2026-01-22
**Status:** Complete - Ready for Implementation
**Estimated Total Effort:** 10-25 days (depending on parallelization)

**🎯 IMMEDIATE ACTION:** Create [production/crates/orchestrator/src/dkg_service.rs](production/crates/orchestrator/src/dkg_service.rs) and start with CGGMP24 DKG implementation
