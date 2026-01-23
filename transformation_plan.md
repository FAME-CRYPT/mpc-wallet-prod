# TORCUS MPC WALLET - DÖNÜŞÜM PLANI

## Genel Bakış

Bu doküman, mevcut Bitcoin odaklı MPC wallet implementasyonunun, `production/project_proposal.md` dokümanında tanımlanan Ethereum/EVM multi-chain MPC custody sistemine dönüştürülmesi için gereken eksiksiz adım planını içerir.

### Kritik Fark Analizi

**Mevcut Durum:** Bitcoin-only MPC wallet (CGGMP24/FROST protocols)
**Hedef Durum:** Ethereum/EVM multi-chain MPC custody sistemi (PolicyEngine, ChainMonitor, TxObserver, BackupNet)

---

## 1. MİMARİ DÖNÜŞÜM

### 1.1. Mevcut Bileşenler (Korunacak/Uyarlanacak)

#### ✅ KORUNACAK BILEŞENLER
- **network crate**: QUIC/mTLS altyapısı ✓
- **security crate**: TLS/cert yönetimi ✓
- **consensus crate**: Byzantine detection (uyarlanacak) ✓
- **storage crate**: PostgreSQL/etcd (genişletilecek) ✓

#### 🔄 UYARLANACAK BILEŞENLER
- **protocols crate**:
  - Mevcut: Bitcoin için CGGMP24/FROST
  - Yeni: Ethereum için ECDSA (secp256k1) TSS protocol
  - Aksiyon: Ethereum signing logic ekle, BIP32/44 yerine EIP-2334 kullan

- **orchestrator crate**:
  - Mevcut: Basit coordination
  - Yeni: PolicyEngine'in çekirdeği olacak
  - Aksiyon: RAFT consensus entegrasyonu, policy enforcement logic

#### ❌ KALDIRILACAK/DEĞİŞTİRİLECEK BILEŞENLER
- **bitcoin crate**: Ethereum/EVM equivalenti ile değiştirilecek
- **cli crate**: Platform API'sine göre yeniden tasarlanacak

### 1.2. Yeni Eklenecek Bileşenler

#### 📦 YENİ CRATES

**1. `policy_engine` crate**
```
Konum: production/crates/policy_engine/
Sorumluluk:
- TEE içinde çalışan RAFT cluster
- Kural seti yönetimi (daily limits, whitelist, KYT)
- İşlem approval/reject kararları
- Admin threshold signature doğrulama
- EventLog üretimi
```

**2. `chain_monitor` crate**
```
Konum: production/crates/chain_monitor/
Sorumluluk:
- Multi-chain RPC integration (Ethereum, BSC, Avalanche, etc.)
- Deposit address tracking (L^watch listesi)
- Balance monitoring
- Auto-sweep triggering
- Event emission to PolicyEngine
```

**3. `tx_observer` crate**
```
Konum: production/crates/tx_observer/
Sorumluluk:
- Transaction lifecycle management
- Nonce tracking per address
- RBF (Replace-By-Fee) logic
- Mempool monitoring
- Receipt confirmation
- Redis/In-memory cache (hot storage)
```

**4. `backup_net` crate**
```
Konum: production/crates/backup_net/
Sorumluluk:
- RAFT consensus implementasyonu
- L1 key backup storage (şifreli)
- Audit log storage (plaintext)
- SMT (Sparse Merkle Tree) data structure
- Recovery request handling
```

**5. `evm_chain` crate**
```
Konum: production/crates/evm_chain/
Sorumluluk:
- EVM transaction builder (EIP-1559, Legacy)
- Address derivation (m/44'/60'/0'/0/index)
- RLP encoding/decoding
- Gas estimation
- Multi-chain RPC client (ethers-rs based)
```

**6. `api_gateway` crate**
```
Konum: production/crates/api_gateway/
Sorumluluk:
- Load balancer entry point
- Rate limiting
- Initial authentication
- Request signing (sk^API_j)
- Forwarding to PolicyEngine
```

**7. `gas_station` crate**
```
Konum: production/crates/gas_station/
Sorumluluk:
- GAS_TANK wallet management
- ETH/native coin injection for token transfers
- Balance monitoring
- Gas price estimation
- Daily limit enforcement (GAS_TANK_LIMIT)
```

---

## 2. PROTOKOL DÖNÜŞÜMÜ

### 2.1. Kriptografik Protokol Değişiklikleri

#### Mevcut Durum (Bitcoin)
```rust
// CGGMP24 for ECDSA (Bitcoin)
// FROST for Schnorr (Taproot)
protocols/src/cggmp24/
protocols/src/frost/
```

#### Hedef Durum (Ethereum)
```rust
// ECDSA TSS for Ethereum (secp256k1)
// Aynı CGGMP24 kullanılabilir (curve değişikliği ile)
protocols/src/ethereum_tss/
  - keygen.rs (TSS-DKG for secp256k1)
  - signing.rs (TSS-Sign for Ethereum TX)
  - derivation.rs (Hierarchical: L1 -> L2)
```

**Aksiyon:**
1. CGGMP24'ü Ethereum için adapt et (secp256k1 curve)
2. BIP32/44 yerine EIP-2334 derivation path kullan
3. Ethereum transaction signing format (RLP encoded)
4. Recovery ID (v value) hesaplama

### 2.2. Anahtar Yönetimi Değişiklikleri

#### Hiyerarşik Yapı (L1 -> L2)

**L1 (Root Level):**
```
ChainID başına bir root key
- sk_j^{ChainID,root} (her düğümde pay)
- pk^{ChainID,root} (ortak public key)
- HSM ile şifrelenir ve BackupNet'e yedeklenir
```

**L2 (User/Deposit Level):**
```
Deterministik türetme:
- sk_j^{ChainID,user,ctr} = Derive(sk_j^{ChainID,root}, user_id, ctr)
- pk^{ChainID,user,ctr} = Derive(pk^{ChainID,root}, user_id, ctr)
- Volatile memory'de tutulur (disk'e yazılmaz)
- BackupNet'te sadece WalletMetadata saklanır
```

**WalletMetadata:**
```rust
struct WalletMetadata {
    pk_root: PublicKey,
    cred_user: UserCredential,
    chain_id: ChainId,
    counter: u32,
    wallet_type: WalletType, // PERSONAL | DEPOSIT
    end_user_id: Option<String>, // Platform customers için
}
```

### 2.3. İmza Akışı Değişiklikleri

#### Proposal'a Göre İmza Akışı

```mermaid
User/ChainMonitor -> APIGateway -> PolicyEngine -> MPCnet -> TxObserver -> Blockchain
                                        |                                    |
                                        v                                    v
                                   BackupNet                            Confirmation
```

**Adımlar:**
1. User/ChainMonitor creates request
2. APIGateway validates & rate limits
3. PolicyEngine checks rules:
   - Credential validation
   - Daily limit check
   - Whitelist check
   - Manual approval if needed
4. MPCnet performs TSS-Sign
5. TxObserver broadcasts & tracks
6. RBF if stuck
7. Confirmation

---

## 3. VERİ YAPILARI VE STORAGE

### 3.1. PostgreSQL Schema Değişiklikleri

#### Yeni Tablolar

**1. `policy_rules` tablosu**
```sql
CREATE TABLE policy_rules (
    user_id VARCHAR(255) PRIMARY KEY,
    daily_limit_fiat DECIMAL(20,2),
    tx_limit_fiat DECIMAL(20,2),
    asset_whitelist TEXT[], -- JSON array
    req_approvals INTEGER, -- k-of-n
    admin_keys TEXT[], -- JSON array of pubkeys
    webhook_url VARCHAR(512),
    kyt_level VARCHAR(50),
    system_fee DECIMAL(10,6),
    sweep_threshold DECIMAL(20,8),
    master_vault_addr VARCHAR(42),
    gas_tank_limit DECIMAL(20,8),
    withdrawal_addr TEXT[], -- JSON array
    enforce_whitelist BOOLEAN,
    created_at TIMESTAMP,
    updated_at TIMESTAMP
);
```

**2. `wallet_metadata` tablosu**
```sql
CREATE TABLE wallet_metadata (
    wallet_id SERIAL PRIMARY KEY,
    pk_root VARCHAR(132), -- hex encoded
    cred_user VARCHAR(255),
    chain_id VARCHAR(50),
    counter INTEGER,
    wallet_type VARCHAR(20), -- PERSONAL | DEPOSIT
    end_user_id VARCHAR(255) NULL,
    address VARCHAR(42),
    created_at TIMESTAMP,
    INDEX idx_user_chain (cred_user, chain_id),
    INDEX idx_address (address),
    INDEX idx_end_user (end_user_id)
);
```

**3. `deposit_addresses` tablosu (ChainMonitor için)**
```sql
CREATE TABLE deposit_addresses (
    address VARCHAR(42) PRIMARY KEY,
    chain_id VARCHAR(50),
    end_user_id VARCHAR(255),
    master_vault_addr VARCHAR(42),
    last_balance DECIMAL(30,18),
    last_check TIMESTAMP,
    status VARCHAR(20), -- ACTIVE | SWEPT | PENDING
    created_at TIMESTAMP
);
```

**4. `transaction_tracking` tablosu (TxObserver için)**
```sql
CREATE TABLE transaction_tracking (
    tracking_id UUID PRIMARY KEY,
    user_id VARCHAR(255),
    chain_id VARCHAR(50),
    nonce INTEGER,
    current_status VARCHAR(20), -- PENDING | CONFIRMED | DROPPED | REPLACED
    created_at TIMESTAMP,
    attempts JSONB, -- [{tx_hash, gas_price, status}]
    latest_tx_hash VARCHAR(66)
);
```

**5. `event_logs` tablosu (BackupNet için)**
```sql
CREATE TABLE event_logs (
    log_index BIGSERIAL PRIMARY KEY,
    timestamp TIMESTAMP,
    source_component VARCHAR(50), -- PolicyEngine | MPCnet | ChainMonitor
    pk_source VARCHAR(132),
    event_type VARCHAR(50),
    event_data JSONB,
    signature TEXT,
    raft_term INTEGER,
    raft_log_index BIGINT
);
```

**6. `key_backups` tablosu (BackupNet için)**
```sql
CREATE TABLE key_backups (
    node_id VARCHAR(50) PRIMARY KEY,
    chain_id VARCHAR(50),
    pk_mpc VARCHAR(132),
    pk_root VARCHAR(132),
    encrypted_key BYTEA,
    signature TEXT,
    backup_timestamp TIMESTAMP,
    raft_log_index BIGINT
);
```

### 3.2. Redis Schema (Hot Storage - TxObserver)

```rust
// Redis key pattern
"tx:tracking:{tracking_id}" -> JSON {
    tracking_id,
    user_id,
    chain_id,
    nonce,
    current_status,
    created_at,
    attempts: [
        {
            attempt_id,
            tx_hash,
            gas_price,
            broadcast_time,
            status,
            chain_status_code
        }
    ],
    latest_tx_hash
}

// TTL: Move to PostgreSQL after CONFIRMED
```

### 3.3. etcd Key Structure

```
# Cluster coordination
/cluster/nodes/{node_id} -> NodeInfo
/cluster/leader -> LeaderID

# RAFT state (PolicyEngine)
/policy/raft/term -> Current term
/policy/raft/voted_for -> Voted node
/policy/raft/log/{index} -> Log entry

# Watch list (ChainMonitor)
/monitor/watch/{chain_id}/{address} -> EndUserID

# Active sessions (MPCnet)
/mpc/sessions/{session_id} -> SessionState
```

---

## 4. NETWORK VE İLETİŞİM

### 4.1. Bileşenler Arası İletişim

#### mTLS Kanal Matrisi

```
Component         | Talks To              | Protocol
------------------|-----------------------|----------
APIGateway        | PolicyEngine          | mTLS/gRPC
ChainMonitor      | PolicyEngine          | mTLS/gRPC
PolicyEngine      | MPCnet                | mTLS/gRPC
PolicyEngine      | BackupNet             | mTLS/gRPC
PolicyEngine      | TxObserver            | Internal (same process)
TxObserver        | Blockchain RPC        | HTTPS/WSS
MPCnet            | BackupNet             | mTLS/gRPC
MPCnet            | HSM                   | PCIe/Local
```

#### gRPC Service Definitions

**PolicyEngineService:**
```protobuf
service PolicyEngine {
  rpc CheckCreateWallet(CreateWalletRequest) returns (PolicyDecision);
  rpc CheckTransaction(TransactionRequest) returns (PolicyDecision);
  rpc UpdateRules(RuleUpdateRequest) returns (UpdateResponse);
  rpc GetEventLogs(LogQueryRequest) returns (LogQueryResponse);
}
```

**MPCnetService:**
```protobuf
service MPCnet {
  rpc StartKeygen(KeygenRequest) returns (KeygenResponse);
  rpc StartSign(SignRequest) returns (SignResponse);
  rpc BackupKey(KeyBackupRequest) returns (BackupResponse);
  rpc RecoverKey(RecoveryRequest) returns (RecoveryResponse);
}
```

**ChainMonitorService:**
```protobuf
service ChainMonitor {
  rpc RegisterWatch(WatchRequest) returns (WatchResponse);
  rpc GetDeposits(DepositQuery) returns (DepositList);
  rpc TriggerSweep(SweepRequest) returns (SweepResponse);
}
```

**BackupNetService:**
```protobuf
service BackupNet {
  rpc BackupKey(SignedKeyBackup) returns (BackupConfirmation);
  rpc LogEvent(EventLog) returns (LogConfirmation);
  rpc RecoverKey(SignedRecoveryRequest) returns (EncryptedKey);
  rpc QueryLogs(SignedLogQuery) returns (LogResults);
}
```

### 4.2. Multi-Chain RPC Integration

#### Chain Configuration

```rust
struct ChainConfig {
    chain_id: ChainId,
    network_name: String,
    rpc_endpoints: Vec<String>, // Multiple for failover
    wss_endpoints: Vec<String>, // For event listening
    chain_type: ChainType, // EVM | Bitcoin | Solana
    native_currency: Currency,
    block_time: Duration,
    confirmation_blocks: u32,
    supports_eip1559: bool,
}

// Supported chains (Phase 1)
const CHAINS: &[ChainConfig] = &[
    ChainConfig::ethereum_mainnet(),
    ChainConfig::bsc_mainnet(),
    ChainConfig::avalanche_c_chain(),
    ChainConfig::arbitrum_one(),
    ChainConfig::polygon_pos(),
];
```

---

## 5. OPERASYONEL PROTOKOLLER

### 5.1. Cüzdan Oluşturma Akışı

#### Proposal'a Göre Adımlar

**Senaryo A: Bireysel Müşteri (PERSONAL)**
```
1. User -> APIGateway: CreateWalletRequest {
     credential,
     chain_id,
     wallet_type: PERSONAL,
     signature(sk_client)
   }

2. APIGateway validates:
   - Rate limit check
   - Signature verification
   -> Forward to PolicyEngine (signed with sk^API_j)

3. PolicyEngine checks:
   - Credential in L^c?
   - Signature valid?
   - CheckCreateWallet() -> 0/1
   -> If approved, send to MPCnet

4. MPCnet derives L2 key:
   - counter = get_next_counter(user, chain_id)
   - sk_j^L2 = Derive(sk_j^L1, user, counter)
   - pk^L2 = Derive(pk^L1, user, counter)
   - Address = to_address(pk^L2)

5. PolicyEngine stores WalletMetadata to BackupNet

6. Response to User: {
     address,
     wallet_type: PERSONAL,
     chain_id
   }
```

**Senaryo B: Platform Müşterisi (DEPOSIT)**
```
Same flow BUT:
- wallet_type: DEPOSIT
- end_user_id: "platform_user_123"
- PolicyEngine -> ChainMonitor: RegisterWatch {
    chain_id,
    address,
    end_user_id,
    master_vault_addr
  }
```

### 5.2. İşlem İmzalama Akışı

#### User-Initiated Transaction

```
1. User -> APIGateway: SignTxRequest {
     credential,
     chain_id,
     wallet_type,
     pk_wallet,
     tx_body: {
       to,
       value,
       data,
       gas_limit,
       max_fee_per_gas, // EIP-1559
       max_priority_fee_per_gas
     },
     signature(sk_client)
   }

2. APIGateway:
   - Rate limit check
   - Signature verify
   -> Forward to PolicyEngine

3. PolicyEngine checks:
   - Credential valid?
   - Client signature valid?
   - Timestamp check (< Δ_max)
   - CheckTransaction():
     * Daily limit OK?
     * To address in whitelist?
     * KYT screening passed?

   If ENFORCE_WHITELIST=true && to NOT in whitelist:
     -> REJECT

   If ENFORCE_WHITELIST=false && to NOT in whitelist:
     -> PENDING_APPROVAL (wait for k-of-n admin signatures)

   If all OK:
     -> Generate sid
     -> Send SignedCommand to MPCnet

4. MPCnet:
   - Verify SignedCommand signature
   - Lookup sk_j^L2 from local cache
   - TSS-Sign ceremony
   - Return signed_tx (r, s, v)

5. PolicyEngine -> TxObserver: {
     signed_tx,
     nonce,
     retry_policy
   }

6. TxObserver:
   - Queue to Redis (hot storage)
   - Broadcast to chain RPC
   - Return tracking_id to User

7. TxObserver monitors:
   - Poll for receipt
   - If stuck -> RBF:
     * Increase gas price
     * Request new signature from MPCnet
     * Broadcast replacement tx
   - If confirmed -> Move to PostgreSQL
```

### 5.3. Auto-Sweep Akışı (Platform Müşterileri İçin)

```
1. ChainMonitor detects deposit:
   For each address in L^watch:
     balance = rpc.get_balance(address)

     If balance >= SWEEP_THRESHOLD:
       Emit SweepReq to PolicyEngine

2. PolicyEngine receives SweepReq:
   - Verify source is ChainMonitor (internal)
   - Check target == MASTER_VAULT_ADDR

   If asset is TOKEN (ERC20):
     - Check native balance for gas
     - If insufficient:
       * Create GAS injection TX
       * MPCnet signs injection TX
       * TxObserver broadcasts
       * Wait for confirmation

   - Create sweep TX (all balance)
   - Send to MPCnet for signing

3. MPCnet signs sweep TX

4. TxObserver broadcasts & tracks

5. On confirmation:
   - Update deposit_addresses.status = SWEPT
   - Log event to BackupNet
```

### 5.4. RBF (Replace-By-Fee) Protokolü

```rust
// TxObserver RBF logic
impl TxObserver {
    async fn monitor_pending_tx(&self, tracking_id: Uuid) {
        let mut check_count = 0;
        let max_checks = 60; // 5 blocks * 12 checks/block

        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;

            let receipt = self.rpc.get_transaction_receipt(tx_hash).await;

            match receipt {
                Some(r) if r.status == 1 => {
                    // CONFIRMED
                    self.update_status(tracking_id, TxStatus::Confirmed).await;
                    self.archive_to_postgres(tracking_id).await;
                    break;
                }
                Some(r) if r.status == 0 => {
                    // FAILED
                    self.update_status(tracking_id, TxStatus::Failed).await;
                    break;
                }
                None => {
                    check_count += 1;

                    if check_count >= max_checks {
                        // STUCK - Trigger RBF
                        self.trigger_rbf(tracking_id).await;
                        check_count = 0; // Reset for new TX
                    }
                }
            }
        }
    }

    async fn trigger_rbf(&self, tracking_id: Uuid) {
        let tx_data = self.get_tracking_data(tracking_id).await;

        // Increase gas price by 10%
        let new_gas_price = tx_data.latest_gas_price * 110 / 100;

        // Request new signature from MPCnet (SAME nonce)
        let new_signed_tx = self.policy_engine
            .request_rbf_signature(tx_data.nonce, new_gas_price)
            .await;

        // Broadcast replacement
        let new_tx_hash = self.rpc.send_raw_transaction(new_signed_tx).await;

        // Update tracking
        self.add_attempt(tracking_id, new_tx_hash, new_gas_price).await;
    }
}
```

---

## 6. GÜVENLİK VE YEDEKLEME

### 6.1. BackupNet Implementasyonu

#### RAFT Consensus

```rust
// BackupNet node
struct BackupNode {
    node_id: String,
    raft: RaftNode,
    storage: Arc<BackupStorage>,
    authorized_mpcs: HashSet<PublicKey>, // L^backup_pk
    authorized_admins: HashSet<PublicKey>, // L^admin_pk
}

impl BackupNode {
    async fn handle_key_backup(&self, req: SignedKeyBackup) -> Result<()> {
        // 1. Verify sender is authorized MPC node
        if !self.authorized_mpcs.contains(&req.pk_mpc) {
            return Err(Error::Unauthorized);
        }

        // 2. Verify signature
        if !verify_signature(req.pk_mpc, &req.key_backup, &req.signature) {
            return Err(Error::InvalidSignature);
        }

        // 3. Check idempotency
        if self.storage.exists(&req.key_backup).await {
            return Ok(()); // Already backed up
        }

        // 4. If not leader, forward to leader
        if !self.raft.is_leader() {
            return self.forward_to_leader(req).await;
        }

        // 5. Append to RAFT log
        let entry = RaftLogEntry {
            term: self.raft.current_term(),
            index: self.raft.next_index(),
            data: req.key_backup,
        };

        self.raft.append_entry(entry).await?;

        // 6. Wait for quorum
        self.raft.wait_for_commit(entry.index).await?;

        // 7. Persist to storage
        self.storage.store_key_backup(req.key_backup).await?;

        Ok(())
    }

    async fn handle_event_log(&self, log: EventLog) -> Result<()> {
        // Similar RAFT consensus flow
        // Store in L^audit list
        // No encryption (plaintext)
    }
}
```

#### Sparse Merkle Tree (SMT) for Backup Verification

```rust
struct BackupSMT {
    root: Hash,
    tree: SparseMerkleTree,
}

impl BackupSMT {
    fn insert_backup(&mut self, node_id: &str, backup: &KeyBackup) {
        let key = hash(node_id);
        let value = hash(backup);
        self.tree.insert(key, value);
        self.root = self.tree.root();
    }

    fn generate_proof(&self, node_id: &str) -> MerkleProof {
        let key = hash(node_id);
        self.tree.generate_proof(key)
    }

    fn verify_proof(&self, node_id: &str, backup: &KeyBackup, proof: &MerkleProof) -> bool {
        let key = hash(node_id);
        let value = hash(backup);
        proof.verify(self.root, key, value)
    }
}
```

### 6.2. HSM Entegrasyonu

#### Key Wrapping Protocol

```rust
// TEE side
struct MPCNode {
    hsm_client: HsmClient,
    node_keys: NodeKeys,
}

impl MPCNode {
    async fn backup_root_key(&self, sk_root: SecretKey, chain_id: ChainId) -> Result<()> {
        // 1. Serialize key
        let key_bytes = sk_root.to_bytes();

        // 2. Send to HSM for encryption
        let encrypted = self.hsm_client.wrap_key(HsmWrapRequest {
            key_data: key_bytes,
            metadata: KeyMetadata {
                chain_id: chain_id.clone(),
                key_type: KeyType::RootL1,
            },
        }).await?;

        // 3. Build backup message
        let backup = KeyBackup {
            node_id: self.node_keys.node_id.clone(),
            pk_root: sk_root.public_key(),
            encrypted_key: encrypted.ciphertext,
        };

        // 4. Sign with node's key
        let signature = sign(self.node_keys.sk_mpc, &backup);

        // 5. Send to BackupNet
        let signed_backup = SignedKeyBackup {
            pk_mpc: self.node_keys.pk_mpc,
            key_backup: backup,
            signature,
        };

        self.backup_net_client.backup_key(signed_backup).await?;

        Ok(())
    }

    async fn recover_root_key(&self, encrypted: Vec<u8>) -> Result<SecretKey> {
        // 1. Send to HSM for decryption
        let decrypted = self.hsm_client.unwrap_key(HsmUnwrapRequest {
            ciphertext: encrypted,
        }).await?;

        // 2. Load into TEE RAM (never touch disk)
        let sk_root = SecretKey::from_bytes(&decrypted.key_data)?;

        Ok(sk_root)
    }
}
```

### 6.3. Disaster Recovery Protokolü

#### Node Failure & Recovery

```
SCENARIO: MPC Node j crashes

1. PolicyEngine detects timeout:
   - Mark node_j as UNAVAILABLE
   - Remove from active sign set
   - Alert KVHS admins

2. Physical intervention:
   - Extract HSM_j from failed server
   - Insert into new clean server (Node'_j)
   - Admin login to HSM_j with k-of-n credentials

3. Recovery request:
   - Admins create RecoveryRequest {
       node_id: j,
       hardware_id: new_server_id
     }
   - k admins sign with TSS-Sign (sk^admin shares)
   - Send SignedRecoveryRequest to BackupNet

4. BackupNet validates:
   - Verify admin threshold signature
   - Lookup encrypted key from L^backup_bck
   - Send EncryptedKey to Node'_j

5. Node'_j decrypts:
   - HSM_j unwraps with KEK
   - Load sk_j^root to TEE RAM
   - Fetch WalletMetadata from BackupNet
   - Re-derive all L2 keys locally

6. Node'_j rejoins:
   - mTLS handshake with PolicyEngine
   - Status -> HEALTHY
   - Include in next sign round
```

---

## 7. ADMIN VE POLICY MANAGEMENT

### 7.1. Admin Threshold Signature Setup

#### TSS-DKG for Admin Keys

```rust
// During system bootstrapping
async fn initialize_admin_keys(kvhs_admins: Vec<AdminInfo>) -> Result<AdminKeySet> {
    let n = kvhs_admins.len();
    let k = (n * 2) / 3 + 1; // 2/3 threshold

    // Run TSS-DKG ceremony
    let (pk_admin, sk_shares) = tss_dkg(k, n).await?;

    // Distribute shares to admins' secure devices (HSM/Smart Card)
    for (admin, sk_share) in kvhs_admins.iter().zip(sk_shares) {
        admin.secure_device.store_key(sk_share).await?;
    }

    // Store public key in PolicyEngine
    policy_engine.set_admin_pubkey(pk_admin).await?;

    Ok(AdminKeySet { pk_admin, k, n })
}
```

### 7.2. Policy Update Protocol

```
1. User requests rule change:
   User -> KVHS Dashboard: {
     rule_id: "WITHDRAWAL_ADDR",
     new_value: ["0xABC...", "0xDEF..."],
     signature(sk_client)
   }

2. KVHS admins review:
   - Verify user identity
   - Check compliance
   - Verify signature
   - Approve request

3. Admins create PolicyUpdateBody:
   body = {
     "UpdatePolicy",
     cred_user,
     rule_id,
     new_value
   }

4. TSS-Sign ceremony (k-of-n admins):
   signature_admin = TSS-Sign(k, {sk_i^admin}, body)

5. Send to PolicyEngine:
   PolicyEngine.update_rules(body, signature_admin)

6. PolicyEngine validates:
   - Verify TSS signature (pk^admin)
   - Check access control (Mutable-User vs Mutable-KVHS)
   - Apply update to database

7. Log to BackupNet:
   EventLog {
     pk_cluster,
     "PolicyUpdate",
     body,
     signature_cluster
   }
```

### 7.3. Audit Log Query Protocol

```
1. KVHS admin creates query:
   query = {
     filter_type: BY_TIME,
     start: "2024-01-01T00:00:00Z",
     end: "2024-01-31T23:59:59Z",
     end_user_id: "platform_user_123"
   }

2. k-of-n admins sign:
   signature_admin = TSS-Sign(k, {sk_i^admin}, query)

3. Send to BackupNet:
   BackupNet.query_logs(query, signature_admin)

4. BackupNet validates:
   - pk^admin in L^admin_pk?
   - TSS-Verify(pk^admin, query, signature) == 1?

5. If valid:
   - RAFT consensus to log the query itself
   - Fetch logs from L^audit
   - Filter by criteria
   - Return plaintext logs

6. Logs include:
   - Original event signature (proves integrity)
   - Timestamp
   - Source component
   - Event data
```

---

## 8. IMPLEMENTATION ROADMAP

### Phase 1: Foundation (Weeks 1-4)

**Week 1-2: Architecture & Protobuf**
- [ ] Define all gRPC service interfaces
- [ ] Create protobuf definitions
- [ ] Setup multi-crate workspace structure
- [ ] Add new crates to Cargo.toml

**Week 3-4: EVM Chain Support**
- [ ] Implement `evm_chain` crate
- [ ] ECDSA TSS adaptation for Ethereum
- [ ] Hierarchical key derivation (L1 -> L2)
- [ ] Transaction builder (EIP-1559)
- [ ] Multi-chain RPC client

### Phase 2: Core Components (Weeks 5-10)

**Week 5-6: PolicyEngine**
- [ ] RAFT consensus integration
- [ ] Rule set database schema
- [ ] Policy checking logic
- [ ] Admin signature verification
- [ ] Integration with etcd

**Week 7-8: BackupNet**
- [ ] RAFT cluster implementation
- [ ] Key backup storage
- [ ] Event log storage
- [ ] SMT data structure
- [ ] Recovery protocols

**Week 9-10: TxObserver**
- [ ] Transaction lifecycle manager
- [ ] Redis hot storage
- [ ] RBF logic
- [ ] Nonce management
- [ ] Confirmation monitoring

### Phase 3: Operational Services (Weeks 11-14)

**Week 11-12: ChainMonitor**
- [ ] Multi-chain block listener
- [ ] Deposit detection
- [ ] Balance tracking
- [ ] Sweep triggering
- [ ] Watch list management

**Week 13-14: Gas Station**
- [ ] GAS_TANK wallet management
- [ ] ETH injection logic
- [ ] Gas estimation
- [ ] Daily limit tracking

### Phase 4: APIGateway & Integration (Weeks 15-18)

**Week 15-16: APIGateway**
- [ ] Load balancer setup
- [ ] Rate limiting (Redis-based)
- [ ] Authentication
- [ ] Request signing
- [ ] REST/gRPC endpoints

**Week 17-18: End-to-End Integration**
- [ ] Wire all components together
- [ ] Integration tests
- [ ] Error handling & recovery
- [ ] Logging & monitoring

### Phase 5: Security & Testing (Weeks 19-22)

**Week 19-20: Security Hardening**
- [ ] HSM integration testing
- [ ] TEE attestation
- [ ] mTLS certificate management
- [ ] Key rotation protocols

**Week 21-22: Testing**
- [ ] Unit tests for all components
- [ ] Integration test suite
- [ ] Chaos engineering tests
- [ ] Load testing
- [ ] Security audit preparation

### Phase 6: Documentation & Deployment (Weeks 23-24)

**Week 23: Documentation**
- [ ] API documentation
- [ ] Deployment guides
- [ ] Runbooks for operations
- [ ] Disaster recovery procedures

**Week 24: Production Deployment**
- [ ] Docker image builds
- [ ] Kubernetes manifests
- [ ] Production infrastructure setup
- [ ] Monitoring & alerting
- [ ] Go-live checklist

---

## 9. IMPLEMENTATION DETAILS

### 9.1. Crate Structure

```
production/
├── Cargo.toml (workspace)
├── crates/
│   ├── api_gateway/
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── auth.rs
│   │   │   ├── rate_limit.rs
│   │   │   ├── routes.rs
│   │   │   └── client.rs (PolicyEngine client)
│   │   └── Cargo.toml
│   │
│   ├── policy_engine/
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── raft.rs
│   │   │   ├── rules.rs
│   │   │   ├── checker.rs
│   │   │   ├── orchestrator.rs
│   │   │   └── admin.rs
│   │   └── Cargo.toml
│   │
│   ├── chain_monitor/
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── listener.rs
│   │   │   ├── detector.rs
│   │   │   ├── sweep.rs
│   │   │   └── rpc.rs
│   │   └── Cargo.toml
│   │
│   ├── tx_observer/
│   │   ├── src/
│   │   │   ├── lib.rs (embedded in PolicyEngine)
│   │   │   ├── tracker.rs
│   │   │   ├── rbf.rs
│   │   │   ├── nonce.rs
│   │   │   └── lifecycle.rs
│   │   └── Cargo.toml
│   │
│   ├── backup_net/
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── raft.rs
│   │   │   ├── storage.rs
│   │   │   ├── smt.rs
│   │   │   └── recovery.rs
│   │   └── Cargo.toml
│   │
│   ├── mpc_net/ (renamed from orchestrator)
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── tss.rs
│   │   │   ├── keygen.rs
│   │   │   ├── signing.rs
│   │   │   ├── derivation.rs
│   │   │   └── hsm.rs
│   │   └── Cargo.toml
│   │
│   ├── evm_chain/
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── tx_builder.rs
│   │   │   ├── address.rs
│   │   │   ├── rpc.rs
│   │   │   └── types.rs
│   │   └── Cargo.toml
│   │
│   ├── gas_station/
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── injector.rs
│   │   │   ├── estimator.rs
│   │   │   └── tracker.rs
│   │   └── Cargo.toml
│   │
│   ├── protocols/ (adapted)
│   │   ├── src/
│   │   │   ├── ethereum_tss/
│   │   │   │   ├── keygen.rs
│   │   │   │   ├── signing.rs
│   │   │   │   └── derivation.rs
│   │   │   ├── cggmp24/ (kept for Bitcoin if needed)
│   │   │   └── frost/ (kept for Bitcoin if needed)
│   │   └── Cargo.toml
│   │
│   ├── common/
│   ├── storage/
│   ├── network/
│   ├── security/
│   ├── consensus/
│   └── types/
│
├── docker/
│   ├── Dockerfile.api_gateway
│   ├── Dockerfile.policy_engine
│   ├── Dockerfile.chain_monitor
│   ├── Dockerfile.backup_net
│   ├── Dockerfile.mpc_node
│   ├── docker-compose.yml
│   └── k8s/ (Kubernetes manifests)
│
└── docs/
    ├── architecture.md
    ├── api.md
    ├── deployment.md
    └── disaster_recovery.md
```

### 9.2. Key Dependencies

```toml
[workspace.dependencies]
# Async runtime
tokio = { version = "1.35", features = ["full"] }
tokio-util = "0.7"

# gRPC & Protobuf
tonic = "0.10"
prost = "0.12"
tonic-build = "0.10"

# Web framework (for REST API)
axum = "0.7"
tower = "0.4"
tower-http = "0.5"

# Ethereum
ethers = { version = "2.0", features = ["ethers-solc", "abigen"] }
alloy-primitives = "0.5"
alloy-rlp = "0.3"

# Cryptography
k256 = "0.13" # secp256k1
sha2 = "0.10"
sha3 = "0.10"
hmac = "0.12"
rand = "0.8"
zeroize = "1.7"

# TSS (adapt existing)
curv = "0.10"
multi-party-ecdsa = "0.9"

# Storage
sqlx = { version = "0.7", features = ["postgres", "runtime-tokio-rustls", "json", "uuid", "time"] }
etcd-client = "0.12"
redis = { version = "0.24", features = ["tokio-comp", "connection-manager"] }

# RAFT
raft = "0.7"
protobuf = "3.3"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
bincode = "1.3"

# Networking
quinn = "0.10" # QUIC
rustls = "0.22"
tokio-rustls = "0.25"

# Observability
tracing = "0.1"
tracing-subscriber = "0.3"
prometheus = "0.13"

# Error handling
anyhow = "1.0"
thiserror = "1.0"

# Utils
uuid = { version = "1.6", features = ["v4", "serde"] }
chrono = "0.4"
hex = "0.4"
base64 = "0.21"
```

### 9.3. Configuration Management

#### config.toml Structure

```toml
[system]
environment = "production" # production | staging | development

[api_gateway]
listen_addr = "0.0.0.0:8080"
tls_cert = "/etc/torcus/certs/gateway.crt"
tls_key = "/etc/torcus/certs/gateway.key"
rate_limit_per_second = 100
policy_engine_endpoint = "https://policy-engine:9090"

[policy_engine]
listen_addr = "0.0.0.0:9090"
raft_listen_addr = "0.0.0.0:7070"
raft_peers = [
    "policy-engine-1:7070",
    "policy-engine-2:7070",
    "policy-engine-3:7070"
]
postgres_url = "postgresql://user:pass@postgres:5432/torcus"
etcd_endpoints = ["etcd-1:2379", "etcd-2:2379", "etcd-3:2379"]
mpc_net_endpoints = [
    "https://mpc-node-1:9091",
    "https://mpc-node-2:9091",
    "https://mpc-node-3:9091",
    "https://mpc-node-4:9091",
    "https://mpc-node-5:9091"
]
backup_net_endpoints = [
    "https://backup-node-1:9092",
    "https://backup-node-2:9092",
    "https://backup-node-3:9092"
]
admin_pubkey = "0x..." # Admin threshold signature public key

[mpc_net]
listen_addr = "0.0.0.0:9091"
node_id = "mpc-node-1"
tss_threshold = 3 # t
tss_total = 5     # n
hsm_endpoint = "/dev/hsm0" # PCIe HSM
tee_attestation_required = true
backup_net_endpoints = [
    "https://backup-node-1:9092",
    "https://backup-node-2:9092",
    "https://backup-node-3:9092"
]

[chain_monitor]
listen_addr = "0.0.0.0:9093"
policy_engine_endpoint = "https://policy-engine:9090"

[[chain_monitor.chains]]
chain_id = "eth-mainnet"
rpc_endpoints = [
    "https://eth-mainnet-1.infura.io/v3/PROJECT_ID",
    "https://eth-mainnet-2.alchemy.com/v2/API_KEY"
]
wss_endpoints = [
    "wss://eth-mainnet-1.infura.io/ws/v3/PROJECT_ID"
]
block_time = 12
confirmation_blocks = 12
sweep_check_interval = 30 # seconds

[[chain_monitor.chains]]
chain_id = "bsc-mainnet"
rpc_endpoints = ["https://bsc-dataseed1.binance.org"]
wss_endpoints = ["wss://bsc-ws-node.nariox.org:443"]
block_time = 3
confirmation_blocks = 15
sweep_check_interval = 15

# ... more chains

[tx_observer]
redis_url = "redis://redis:6379"
postgres_url = "postgresql://user:pass@postgres:5432/torcus"
polling_interval = 5 # seconds
stuck_threshold = 60 # seconds (5 blocks for Ethereum)
rbf_gas_increase_percent = 10
max_rbf_attempts = 5

[backup_net]
listen_addr = "0.0.0.0:9092"
node_id = "backup-node-1"
raft_listen_addr = "0.0.0.0:7071"
raft_peers = [
    "backup-node-1:7071",
    "backup-node-2:7071",
    "backup-node-3:7071"
]
postgres_url = "postgresql://user:pass@postgres:5432/torcus_backup"
authorized_mpc_pubkeys = [
    "0x...", # mpc-node-1
    "0x...", # mpc-node-2
    # ...
]
authorized_admin_pubkeys = [
    "0x..." # Admin threshold pubkey
]
smt_tree_depth = 256

[gas_station]
chains = [
    { chain_id = "eth-mainnet", tank_address = "0x...", daily_limit = "10.0" },
    { chain_id = "bsc-mainnet", tank_address = "0x...", daily_limit = "100.0" },
]
```

---

## 10. TESTING STRATEGY

### 10.1. Unit Tests

**Coverage Target: 80%+**

```rust
// Example: PolicyEngine rule checker
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_daily_limit_enforcement() {
        let rules = PolicyRules {
            daily_limit_fiat: Decimal::from(1000),
            ..Default::default()
        };

        let tx = TransactionRequest {
            value: Decimal::from(500),
            ..Default::default()
        };

        let result = rules.check_transaction(&tx).await;
        assert_eq!(result, PolicyDecision::Approve);

        let tx2 = TransactionRequest {
            value: Decimal::from(600),
            ..Default::default()
        };

        let result2 = rules.check_transaction(&tx2).await;
        assert_eq!(result2, PolicyDecision::Reject);
    }

    #[tokio::test]
    async fn test_whitelist_enforcement() {
        let rules = PolicyRules {
            withdrawal_addr: vec![
                Address::from_str("0xABC...").unwrap()
            ],
            enforce_whitelist: true,
            ..Default::default()
        };

        let tx = TransactionRequest {
            to: Address::from_str("0xDEF...").unwrap(),
            ..Default::default()
        };

        let result = rules.check_transaction(&tx).await;
        assert_eq!(result, PolicyDecision::Reject);
    }
}
```

### 10.2. Integration Tests

**Test Scenarios:**

1. **End-to-End Wallet Creation**
   - User request -> APIGateway -> PolicyEngine -> MPCnet -> BackupNet
   - Verify WalletMetadata stored
   - Verify address generated correctly

2. **Transaction Signing Flow**
   - Submit TX -> Policy check -> TSS-Sign -> Broadcast -> Confirm
   - Verify signature validity
   - Verify tracking in TxObserver

3. **Auto-Sweep Flow**
   - Deposit detected -> ChainMonitor -> PolicyEngine -> Gas injection (if needed) -> Sweep TX -> Confirm
   - Verify balance moved to master vault

4. **RBF Flow**
   - Submit TX with low gas -> Monitor -> Stuck -> RBF triggered -> Replacement TX -> Confirm
   - Verify correct nonce reuse

5. **Node Recovery**
   - Simulate node crash -> Admin recovery request -> BackupNet -> Key restoration -> Rejoin cluster
   - Verify L2 keys re-derived correctly

6. **Policy Update**
   - Admin creates update -> TSS-Sign (k-of-n) -> PolicyEngine validates -> Apply -> Log to BackupNet
   - Verify rule updated in database

### 10.3. Load Testing

**Tools: k6, Grafana**

**Scenarios:**
1. 1000 concurrent wallet creation requests
2. 5000 concurrent transaction signing requests
3. 100 chains monitored with 10k deposit addresses
4. 1000 pending transactions tracked simultaneously

**Metrics:**
- Response time (p50, p95, p99)
- Error rate
- Transaction throughput (TPS)
- Database query performance
- Network latency between components

### 10.4. Chaos Engineering

**Failure Scenarios:**
1. Random MPC node crash (expect: system continues with t nodes)
2. PostgreSQL failover (expect: queries redirect to replica)
3. Network partition (expect: RAFT handles split-brain)
4. RPC endpoint failure (expect: failover to backup RPC)
5. Redis crash (expect: TxObserver recovers from PostgreSQL)

---

## 11. MONITORING & OBSERVABILITY

### 11.1. Metrics (Prometheus)

**Key Metrics:**

```rust
// PolicyEngine
policy_requests_total{status="approved|rejected|pending"}
policy_check_duration_seconds
active_sessions_count

// MPCnet
tss_keygen_duration_seconds
tss_sign_duration_seconds{success="true|false"}
active_signing_sessions
node_availability{node_id}

// TxObserver
pending_transactions_count{chain_id}
confirmed_transactions_total{chain_id}
rbf_triggered_total{chain_id}
average_confirmation_time_seconds{chain_id}

// ChainMonitor
deposits_detected_total{chain_id}
sweeps_completed_total{chain_id}
sweep_failures_total{chain_id}
monitored_addresses_count{chain_id}

// BackupNet
key_backups_stored_total
event_logs_stored_total
raft_leader_elections_total
raft_log_entries_count
```

### 11.2. Logging (Structured with tracing)

**Log Levels:**
- ERROR: System failures, critical issues
- WARN: Policy rejections, RBF triggers, node unavailability
- INFO: Transaction submissions, confirmations, policy updates
- DEBUG: Detailed flow tracing
- TRACE: Network messages, cryptographic operations

**Example:**
```rust
use tracing::{info, warn, error, instrument};

#[instrument(skip(self), fields(user_id, chain_id))]
async fn create_wallet(&self, req: CreateWalletRequest) -> Result<Wallet> {
    info!("Wallet creation requested");

    let policy_decision = self.policy_engine.check_create_wallet(&req).await?;

    match policy_decision {
        PolicyDecision::Approve => {
            info!("Policy approved wallet creation");
            // ... proceed
        }
        PolicyDecision::Reject => {
            warn!("Policy rejected wallet creation");
            return Err(Error::PolicyReject);
        }
        _ => {}
    }
}
```

### 11.3. Alerting (Prometheus Alertmanager)

**Critical Alerts:**
```yaml
groups:
- name: mpc_critical
  rules:
  - alert: MPCNodeDown
    expr: node_availability == 0
    for: 1m
    labels:
      severity: critical
    annotations:
      summary: "MPC node {{ $labels.node_id }} is down"

  - alert: PolicyEngineUnreachable
    expr: up{job="policy_engine"} == 0
    for: 30s
    labels:
      severity: critical
    annotations:
      summary: "PolicyEngine cluster unreachable"

  - alert: BackupNetRaftLeaderMissing
    expr: raft_leader{cluster="backup_net"} == 0
    for: 1m
    labels:
      severity: critical
    annotations:
      summary: "BackupNet RAFT has no leader"

  - alert: TxStuckTooLong
    expr: tx_pending_duration_seconds > 600
    for: 1m
    labels:
      severity: warning
    annotations:
      summary: "Transaction {{ $labels.tracking_id }} stuck for 10+ minutes"

  - alert: SweepFailed
    expr: rate(sweep_failures_total[5m]) > 0.1
    labels:
      severity: warning
    annotations:
      summary: "High sweep failure rate on {{ $labels.chain_id }}"
```

---

## 12. DEPLOYMENT

### 12.1. Docker Images

**Build Scripts:**

```dockerfile
# Dockerfile.mpc_node
FROM rust:1.75 as builder

WORKDIR /build
COPY . .

RUN cargo build --release --bin mpc_node

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/mpc_node /usr/local/bin/

EXPOSE 9091

CMD ["mpc_node"]
```

### 12.2. Kubernetes Deployment

**Stateful Components:**
- PolicyEngine (StatefulSet with persistent etcd)
- MPCnet nodes (StatefulSet with HSM affinity)
- BackupNet (StatefulSet with RAFT volumes)
- PostgreSQL (StatefulSet with persistent storage)

**Stateless Components:**
- APIGateway (Deployment with HPA)
- ChainMonitor (Deployment)

**Example Manifest:**

```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: mpc-node
  namespace: torcus
spec:
  serviceName: mpc-node
  replicas: 5
  selector:
    matchLabels:
      app: mpc-node
  template:
    metadata:
      labels:
        app: mpc-node
    spec:
      affinity:
        podAntiAffinity:
          requiredDuringSchedulingIgnoredDuringExecution:
          - labelSelector:
              matchLabels:
                app: mpc-node
            topologyKey: kubernetes.io/hostname
      containers:
      - name: mpc-node
        image: torcus/mpc-node:latest
        ports:
        - containerPort: 9091
          name: grpc
        env:
        - name: NODE_ID
          valueFrom:
            fieldRef:
              fieldPath: metadata.name
        - name: CONFIG_PATH
          value: /etc/torcus/config.toml
        volumeMounts:
        - name: config
          mountPath: /etc/torcus
        - name: certs
          mountPath: /etc/torcus/certs
        - name: hsm
          mountPath: /dev/hsm0
        resources:
          requests:
            cpu: "2"
            memory: "4Gi"
          limits:
            cpu: "4"
            memory: "8Gi"
      volumes:
      - name: config
        configMap:
          name: mpc-node-config
      - name: certs
        secret:
          secretName: mpc-node-certs
      - name: hsm
        hostPath:
          path: /dev/hsm0
          type: CharDevice
---
apiVersion: v1
kind: Service
metadata:
  name: mpc-node
  namespace: torcus
spec:
  clusterIP: None
  selector:
    app: mpc-node
  ports:
  - port: 9091
    name: grpc
```

### 12.3. Production Checklist

**Before Go-Live:**

- [ ] All HSMs installed and initialized
- [ ] Admin threshold keys generated and distributed
- [ ] mTLS certificates issued and distributed
- [ ] PostgreSQL replicated with automatic failover
- [ ] etcd cluster healthy (3+ nodes)
- [ ] Redis cluster configured with persistence
- [ ] All RPC endpoints tested and rate limits configured
- [ ] RAFT clusters (PolicyEngine, BackupNet) healthy
- [ ] Monitoring dashboards created
- [ ] Alerting rules configured and tested
- [ ] Disaster recovery runbooks documented
- [ ] Security audit completed
- [ ] Penetration testing completed
- [ ] Load testing passed (target TPS achieved)
- [ ] Backup procedures tested
- [ ] Node recovery procedures tested
- [ ] Policy update procedures tested
- [ ] Admin key ceremony completed
- [ ] Physical security measures in place
- [ ] Compliance documentation ready

---

## 13. KRİTİK NOTLAR

### 13.1. Proposal'dan Ayrılmamak Gereken Noktalar

**ZORUNLU UYUM:**

1. **PolicyEngine MUTLAKA TEE içinde çalışmalı**
   - RAFT consensus ile
   - Tüm kararlar BackupNet'e loglanmalı
   - Admin threshold signature doğrulamalı

2. **ChainMonitor MUTLAKA ayrı servis olmalı**
   - APIGateway'den bağımsız
   - PolicyEngine'e direkt güvenli kanal

3. **TxObserver MUTLAKA RBF implementasyonu içermeli**
   - Nonce tracking
   - Gas price escalation
   - Redis hot storage -> PostgreSQL cold storage

4. **BackupNet MUTLAKA RAFT consensus kullanmalı**
   - L1 keys şifreli
   - Audit logs plaintext
   - SMT for verification

5. **Gas Station MUTLAKA implement edilmeli**
   - Token transfer için ETH injection
   - GAS_TANK_LIMIT enforcement

6. **WalletType ve EndUserID MUTLAKA desteklenmeli**
   - PERSONAL vs DEPOSIT
   - Platform müşterileri için sub-user tracking

7. **Hierarchical key management (L1->L2)**
   - L1 HSM-encrypted, BackupNet'te
   - L2 volatile memory only
   - WalletMetadata BackupNet'te

### 13.2. Esneklik Olan Noktalar

**IMPLEMENTATION DETAILS:**

1. Programlama dili (Rust önerilir ama zorunlu değil)
2. Exact TSS library (CGGMP24 vs GG20 vs custom)
3. Database specifics (PostgreSQL vs TimescaleDB)
4. Message queue (gRPC vs RabbitMQ vs Kafka)
5. Deployment platform (K8s vs Docker Swarm vs bare metal)

### 13.3. Bitcoin Desteği Nasıl Korunur?

**Eğer Bitcoin desteği de isteniyorsa:**

1. `protocols` crate'inde hem Bitcoin hem Ethereum TSS impl tutulur
2. ChainID'ye göre protocol seçimi yapılır
3. ChainMonitor multi-chain olduğu için Bitcoin RPC de eklenebilir
4. TxObserver Bitcoin için RBF yerine CPFP kullanabilir

**Tavsiye:** İlk fazda sadece Ethereum/EVM, sonra Bitcoin eklenir.

---

## 14. SORU-CEVAP

### S1: Mevcut CGGMP24 implementasyonu atılacak mı?

**C:** Hayır, uyarlanacak. Ethereum için aynı CGGMP24 kullanılabilir (secp256k1 curve), sadece transaction format ve derivation logic değişecek.

### S2: PostgreSQL ve etcd her ikisi de gerekli mi?

**C:** Evet. PostgreSQL persistent data için (wallets, rules, logs), etcd distributed coordination için (RAFT, leader election, ephemeral state).

### S3: Redis neden gerekli?

**C:** TxObserver için hot storage. Pending TX'ler sürekli update olur, PostgreSQL'e her seferinde yazmak yavaş olur. Redis'te tutar, confirm olduktan sonra PostgreSQL'e arşivler.

### S4: BackupNet neden ayrı bir sistem?

**C:** Separation of concerns. MPCnet compromise olsa bile yedekler güvende. Ayrıca audit logs immutable olmalı, operational system'den izole.

### S5: Admin threshold signature nasıl işleyecek?

**C:** KVHS admins TSS-DKG ile ortak bir admin key generate eder (k-of-n). Policy update vs. gibi kritik işlemler için k admin imza birleştirir, PolicyEngine doğrular.

### S6: HSM zorunlu mu?

**C:** Proposal'da zorunlu. Ancak dev/test ortamında mock HSM kullanılabilir. Production'da mutlaka gerçek HSM (PCIe veya network-attached).

### S7: Multi-chain support nasıl scale edecek?

**C:** ChainMonitor her chain için ayrı thread/task. RPC calls async/parallel. etcd'de chain config tutuluyor, dinamik chain ekleme/çıkarma yapılabilir.

### S8: Load testing için target TPS nedir?

**C:** Proposal'da belirtilmemiş ama önerilen:
- Wallet creation: 10 TPS
- Transaction signing: 100 TPS
- Deposit detection: 1000 addr/sec monitoring
- Sweep operations: 50 TPS

---

## 15. SONUÇ

Bu dönüşüm planı, mevcut Bitcoin odaklı MPC wallet implementasyonunuzun `production/project_proposal.md` dokümanında tanımlanan Ethereum/EVM multi-chain MPC custody sistemine eksiksiz dönüşümü için gerekli tüm adımları içermektedir.

### Özet İstatistikler

**Yeni Crates:** 7 (api_gateway, policy_engine, chain_monitor, tx_observer, backup_net, evm_chain, gas_station)

**Uyarlanan Crates:** 4 (protocols, orchestrator->mpc_net, storage, consensus)

**Yeni Tablolar:** 6 (policy_rules, wallet_metadata, deposit_addresses, transaction_tracking, event_logs, key_backups)

**Yeni gRPC Services:** 4 (PolicyEngine, ChainMonitor, BackupNet, MPCnet)

**Tahmini Süre:** 24 hafta (6 ay)

**Tahmini Kod Satırı:** ~50,000 LOC (yeni/değişen)

### Kritik Başarı Faktörleri

1. ✅ TEE ve HSM entegrasyonu doğru çalışmalı
2. ✅ RAFT consensus stable olmalı
3. ✅ Multi-chain RPC failover robust olmalı
4. ✅ RBF logic doğru implement edilmeli
5. ✅ Admin key management güvenli olmalı
6. ✅ Audit trail immutable olmalı
7. ✅ Disaster recovery test edilmiş olmalı

### Bir Sonraki Adımlar

1. Bu planı stakeholder'larla review et
2. Phase 1'i başlat (Architecture & Protobuf)
3. Haftalık sprint'ler ile ilerle
4. Her milestone'da integration test
5. Security audit için erken hazırlık

---

**Plan Versiyonu:** 1.0
**Oluşturulma Tarihi:** 2026-01-23
**Son Güncelleme:** 2026-01-23
**Hazırlayan:** Claude Sonnet 4.5
**Durum:** ✅ HAZIR - İMPLEMENTASYONA BAŞLANMALI
