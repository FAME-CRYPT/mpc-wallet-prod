# 🚨 MPC WALLET - KALAN KRİTİK GÖREVLER (BUGÜN ACİL BİTMELİ)

**Tarih**: 2026-01-22
**Durum**: %95 Tamamlandı - Son 5 Kritik Görev Kaldı
**Öncelik**: 🔴 **ACIL - BUGÜN BİTMELİ**

---

## 🎯 ÖNCEKİ DURUMDAN BU ANA KADAR YAPILAN İŞLER

### ✅ Session 7'de Tamamlanan 4 Kritik Görev

1. ✅ **TransactionOrchestrator + SigningCoordinator Entegrasyonu**
   - Mock imza kodu tamamen silindi
   - Gerçek MPC signing entegre edildi
   - `service.rs:415-461` - Gerçek protokol kullanımı

2. ✅ **Ed25519 Vote Verification İmplementasyonu**
   - Stub kod silindi
   - `crypto/src/lib.rs:1-60` - Gerçek ed25519-dalek doğrulama

3. ✅ **Bitcoin Address Derivation İmplementasyonu**
   - Placeholder kod silindi
   - `common/src/bitcoin_address.rs` - P2WPKH ve P2TR gerçek adresler

4. ✅ **Presignature Participant Selection Düzeltildi**
   - Hardcoded threshold kaldırıldı
   - `presig_service.rs:312-330` - Dinamik participant query

---

## 🔴 KALAN 6 KRİTİK GÖREV (BUGÜN ACİL BİTMELİ)

### Görev 1: Bitcoin Transaction Signature Encoding ⚠️ YÜKSEK ÖNCELİK

**Dosya**: `production/crates/orchestrator/src/service.rs:452`
**Satır**: 452-453

**Mevcut Kod (YANLIŞ)**:
```rust
// TODO: Properly encode the signature into the Bitcoin transaction format
let signed_tx = combined_signature.signature;
```

**Problem**:
- Signature raw bytes olarak kullanılıyor
- Bitcoin witness formatına encode edilmemiş
- P2WPKH ve P2TR için farklı witness yapıları gerekli

**Yapılması Gerekenler**:

1. **P2WPKH (CGGMP24/ECDSA) için witness encoding**:
   ```rust
   // Witness format for P2WPKH:
   // [signature_der][sighash_type][compressed_pubkey]
   let mut witness = Vec::new();
   witness.extend_from_slice(&signature_der);
   witness.push(0x01); // SIGHASH_ALL
   witness.extend_from_slice(&compressed_pubkey);

   // Add witness to transaction input
   tx.input[0].witness = Witness::from_vec(vec![witness]);
   ```

2. **P2TR (FROST/Schnorr) için witness encoding**:
   ```rust
   // Witness format for P2TR (key path spend):
   // [schnorr_signature_64_bytes]
   let witness = Witness::from_vec(vec![signature_schnorr]);
   tx.input[0].witness = witness;
   ```

3. **Protocol-based routing**:
   ```rust
   let signed_tx = match protocol {
       SignatureProtocol::CGGMP24 => {
           encode_p2wpkh_witness(&unsigned_tx, &signature, &pubkey)?
       }
       SignatureProtocol::FROST => {
           encode_p2tr_witness(&unsigned_tx, &signature)?
       }
   };
   ```

**Beklenen Sonuç**:
- ✅ Signature doğru Bitcoin witness formatında
- ✅ P2WPKH için DER-encoded signature + pubkey
- ✅ P2TR için 64-byte Schnorr signature
- ✅ Broadcast edilebilir geçerli Bitcoin transaction

**Süre**: 2-3 saat

---

### Görev 2: Arc<Mutex<EtcdStorage>> Pattern Refactor ⚠️ YÜKSEK ÖNCELİK

**Dosyalar**:
- `production/crates/orchestrator/src/dkg_service.rs:351-361`
- `production/crates/orchestrator/src/presig_service.rs:563-572`

**Problem**:
```rust
// Şu an comment'li çünkü Arc<EtcdStorage> &mut self gerektiren metotları çağıramıyor:
// self.etcd.put(&pubkey_key, &public_key).await?;
// self.etcd.put(&config_key, &config_bytes).await?;
```

**Root Cause**:
- `EtcdStorage` metotları `&mut self` alıyor
- `Arc<T>` ile mutable borrow yapılamıyor
- Interior mutability pattern gerekli

**Çözüm**:

1. **EtcdStorage'ı Arc<Mutex<>> ile sar**:
   ```rust
   // Tüm crate'lerde değiştir:
   pub struct DkgService {
       etcd: Arc<Mutex<EtcdStorage>>,  // DEĞİŞTİ
       // ... diğer fieldlar
   }
   ```

2. **Kullanım yerlerini güncelle**:
   ```rust
   // ÖNCE:
   // self.etcd.put(&key, &value).await?;  // ÇALIŞMAZ

   // SONRA:
   let mut etcd = self.etcd.lock().await;
   etcd.put(&key, &value).await?;
   ```

3. **Comment'li TODO'ları aktif et**:
   - `dkg_service.rs:351-361` - Public key ve DKG config storage
   - `presig_service.rs:566-572` - Participant query from etcd

**Etkilenen Dosyalar** (hepsinde değişiklik gerekli):
- `orchestrator/src/dkg_service.rs`
- `orchestrator/src/presig_service.rs`
- `orchestrator/src/aux_info_service.rs`
- `orchestrator/src/service.rs`
- `api/src/handlers/*.rs` (builder pattern'leri)

**Beklenen Sonuç**:
- ✅ DKG public key'leri etcd'ye yazılıyor
- ✅ DKG config (threshold, node count) etcd'de saklanıyor
- ✅ Presig service etcd'den participant listesi okuyor
- ✅ Fallback kullanımı kaldırılıyor

**Süre**: 1-2 saat

---

### Görev 3: Bitcoin Address Generation from DKG Public Key ⚠️ ORTA ÖNCELİK

**Dosya**: `production/crates/api/src/handlers/dkg.rs:112-116`

**Mevcut Kod (PLACEHOLDER)**:
```rust
// TODO: Use actual address generation from public key
// For now, return placeholder based on protocol
let address = match protocol {
    ProtocolType::CGGMP24 => format!("bc1q{}", "a".repeat(38)), // P2WPKH placeholder
    ProtocolType::FROST => format!("bc1p{}", "b".repeat(58)),   // P2TR placeholder
};
```

**Problem**:
- DKG'den gelen public key kullanılmıyor
- Fake adresler üretiliyor
- `common::bitcoin_address` fonksiyonları var ama kullanılmıyor

**Çözüm**:

1. **Public key'i doğru formatta al**:
   ```rust
   // DKG response'dan public key'i çıkar
   let public_key_bytes = ceremony.public_key; // Vec<u8>
   ```

2. **Protocol'e göre adres türet**:
   ```rust
   use common::bitcoin_address::{derive_p2wpkh_address, derive_p2tr_address, BitcoinNetwork};

   let address = match protocol {
       ProtocolType::CGGMP24 => {
           // CGGMP24 → 33-byte compressed pubkey → P2WPKH
           if public_key_bytes.len() != 33 {
               return Err("Invalid CGGMP24 public key length");
           }
           derive_p2wpkh_address(&public_key_bytes, BitcoinNetwork::Mainnet)?
       }
       ProtocolType::FROST => {
           // FROST → 32-byte x-only pubkey → P2TR
           if public_key_bytes.len() != 32 {
               return Err("Invalid FROST public key length");
           }
           derive_p2tr_address(&public_key_bytes, BitcoinNetwork::Mainnet)?
       }
   };
   ```

3. **Aynı değişikliği şurada da yap**:
   - `dkg.rs:165` - CGGMP24 ceremony result
   - `dkg.rs:185` - FROST ceremony result

**Beklenen Sonuç**:
- ✅ DKG'den gelen GERÇEK public key kullanılıyor
- ✅ Gerçek Bitcoin adresleri üretiliyor (bc1q... veya bc1p...)
- ✅ Kullanıcı bu adrese para gönderebiliyor

**Süre**: 30 dakika

---

### Görev 4: Presignature Pool Status API Integration ⚠️ ORTA ÖNCELİK

**Dosyalar**:
- `production/crates/api/src/handlers/presig.rs:58-67`
- `production/crates/api/src/handlers/presig.rs:97-107`

**Mevcut Kod (MOCK)**:
```rust
// TODO: Query actual presignature pool status
// Mock response
let status = PresignaturePoolStatus {
    available: 87,
    in_use: 3,
    target: 100,
    min_threshold: 20,
};
```

**Problem**:
- Presignature pool var ama API mock data dönüyor
- Gerçek pool status sorgulanmıyor

**Çözüm**:

1. **Orchestrator service'den pool'a erişim**:
   ```rust
   pub async fn get_presignature_pool_status(&self) -> Result<PresignaturePoolStatus> {
       let pool = self.presig_service.get_pool_status().await?;

       Ok(PresignaturePoolStatus {
           available: pool.available_count,
           in_use: pool.in_use_count,
           target: pool.target_size,
           min_threshold: pool.min_threshold,
       })
   }
   ```

2. **Presig service'e getter ekle**:
   ```rust
   // orchestrator/src/presig_service.rs
   pub async fn get_pool_status(&self) -> Result<PoolStats> {
       // CGGMP24 pool varsa ondan oku
       if let Some(pool) = &self.cggmp24_pool {
           return Ok(pool.stats().await);
       }

       // Yoksa default
       Ok(PoolStats::default())
   }
   ```

3. **Handler'ı güncelle**:
   ```rust
   // api/src/handlers/presig.rs:58
   let status = orchestrator.get_presignature_pool_status().await
       .map_err(|e| ApiError::Internal(format!("Failed to get pool status: {}", e)))?;
   ```

4. **Aynı şekilde generation endpoint'ini de düzelt**:
   ```rust
   // api/src/handlers/presig.rs:97
   orchestrator.trigger_presignature_generation(count).await
       .map_err(|e| ApiError::Internal(format!("Failed to trigger generation: {}", e)))?;
   ```

**Beklenen Sonuç**:
- ✅ `/api/presignatures/status` gerçek pool verisi dönüyor
- ✅ `/api/presignatures/generate` gerçekten pool generation tetikliyor
- ✅ Mock data kullanımı kaldırıldı

**Süre**: 1 saat

---

### Görev 5: Cluster Info API Real Data ⚠️ DÜŞÜK ÖNCELİK

**Dosya**: `production/crates/api/src/handlers/cluster.rs:28-38`

**Mevcut Kod (PLACEHOLDER)**:
```rust
// For now, return placeholder values
let info = ClusterInfo {
    node_id: "node-01".to_string(),
    total_nodes: 5,
    threshold: 3,
    active_nodes: 4,
    byzantine_violations: 0,
};
```

**Çözüm**:

1. **etcd'den cluster config oku**:
   ```rust
   let total_nodes = etcd.get("/cluster/total_nodes").await?;
   let threshold = etcd.get("/cluster/threshold").await?;
   ```

2. **Active nodes'ı heartbeat'lerden say**:
   ```rust
   let active_nodes = etcd.count_keys_with_prefix("/nodes/*/heartbeat").await?;
   ```

3. **Byzantine violations'ı PostgreSQL'den oku**:
   ```rust
   let violations = postgres.count_byzantine_violations_last_24h().await?;
   ```

**Beklenen Sonuç**:
- ✅ Gerçek cluster bilgileri gösteriliyor
- ✅ Placeholder data kaldırıldı

**Süre**: 30 dakika

---

### Görev 6: mTLS Certificate Validation Enforcement ⚠️ YÜKSEK ÖNCELİK - GÜVENLİK AÇIĞI

**Dosyalar**:
- `production/crates/network/src/quic_listener.rs:157-170`
- `production/crates/network/src/quic_listener.rs:198-206`

**Mevcut Kod (GÜVENLİK AÇIĞI)**:
```rust
// TODO: Extract authenticated node ID from peer's TLS certificate
let authenticated_node = extract_peer_node_id(&connection);
match authenticated_node {
    Some(node_id) => {
        info!("Authenticated peer {} as node {}", remote_addr, node_id);
    }
    None => {
        warn!("Could not extract node ID from peer certificate for {}", remote_addr);
        // For now, allow connection for development
        // TODO: Close connection once mTLS is enforced
    }
}

fn extract_peer_node_id(_connection: &Connection) -> Option<NodeId> {
    // PLACEHOLDER: Will extract node ID from mTLS certificate
    None
}
```

**Problem**:
- Sistem HERHANGİ bir peer'ı sertifika doğrulaması OLMADAN kabul ediyor
- Byzantine saldırganlar sisteme girebilir
- Node authentication yok
- Production'da ciddi güvenlik riski

**Yapılması Gerekenler**:

1. **Certificate'dan Node ID extract et**:
   ```rust
   fn extract_peer_node_id(connection: &Connection) -> Option<NodeId> {
       // Get peer certificates
       let peer_certs = connection.peer_identity()?;

       // Parse first certificate
       let cert = x509_parser::parse_x509_certificate(&peer_certs[0])
           .ok()?
           .1;

       // Extract Common Name from Subject
       let subject = cert.subject();
       let cn = subject.iter_common_name().next()?;
       let node_id = cn.as_str().ok()?;

       Some(NodeId::from(node_id))
   }
   ```

2. **Unauthenticated connection'ı reject et**:
   ```rust
   match authenticated_node {
       Some(node_id) => {
           info!("Authenticated peer {} as node {}", remote_addr, node_id);
       }
       None => {
           error!("Could not extract node ID from peer certificate for {}", remote_addr);
           // CLOSE CONNECTION - do NOT allow unauthenticated peers
           connection.close(0u32.into(), b"Authentication required");
           return; // Don't process this connection further
       }
   }
   ```

3. **Certificate expiry check ekle**:
   ```rust
   let now = chrono::Utc::now();
   if cert.validity().not_after < now || cert.validity().not_before > now {
       error!("Peer certificate expired or not yet valid for {}", remote_addr);
       connection.close(0u32.into(), b"Certificate expired");
       return;
   }
   ```

4. **Known nodes listesine karşı validate et**:
   ```rust
   if !self.peer_registry.is_authorized_node(&node_id) {
       error!("Node {} is not in authorized node list", node_id);
       connection.close(0u32.into(), b"Unauthorized node");
       return;
   }
   ```

**Dependencies**:
- `x509-parser = "0.16"` (Cargo.toml'a ekle)

**Beklenen Sonuç**:
- ✅ Sadece valid sertifikası olan node'lar bağlanabiliyor
- ✅ Certificate'dan Node ID extract ediliyor
- ✅ Expiry check yapılıyor
- ✅ Unauthenticated connection'lar reject ediliyor
- ✅ Byzantine saldırganlar sisteme giremiyor

**Süre**: 2-3 saat

---

## 🔮 UZAK GELECEK GÖREVLERİ (DÜŞÜK ÖNCELİK)

> **NOT**: Bunlar sistem çalışması için gerekli DEĞİL. Ancak production-ready olmak için yapılmalı.

### Görev 7: Automatic Vote Casting by Nodes 🔮 GELECEK

**Durum**: ŞU AN MANUEL OY GÖNDERME

**Problem**:
- Coordinator voting round başlatıyor ✅
- Node'lar vote request'leri görmüyor ❌
- Node'lar otomatik oy vermiyorlar ❌
- Manuel API call gerekiyor ❌

**Yapılması Gerekenler** (3 alt-görev):

#### 7.1. Node Vote Decision Engine (YENİ COMPONENT)
**Dosya**: `production/crates/orchestrator/src/vote_engine.rs` (OLUŞTURULMALI)

```rust
pub struct VoteEngine {
    bitcoin_client: Arc<BitcoinClient>,
    node_keypair: Ed25519KeyPair,
}

impl VoteEngine {
    /// Decide whether to approve or reject a transaction
    pub async fn decide_vote(&self, tx: &Transaction) -> Result<VoteDecision> {
        // 1. Validate UTXO existence
        let utxos_valid = self.validate_utxos(&tx.inputs).await?;
        if !utxos_valid {
            return Ok(VoteDecision::Reject("Invalid UTXOs"));
        }

        // 2. Validate fee reasonableness
        let fee = tx.calculate_fee();
        if fee > MAX_FEE || fee < MIN_FEE {
            return Ok(VoteDecision::Reject("Unreasonable fee"));
        }

        // 3. Validate OP_RETURN data
        if let Some(op_return) = tx.find_op_return() {
            if !self.validate_op_return(op_return) {
                return Ok(VoteDecision::Reject("Invalid OP_RETURN"));
            }
        }

        // 4. All checks passed - approve
        Ok(VoteDecision::Approve)
    }

    /// Generate signed vote
    pub fn generate_vote(&self, tx_id: &str, decision: VoteDecision) -> Vote {
        let message = format!("vote:{}:{}:{}", round_id, tx_id, decision.approve);
        let signature = self.node_keypair.sign(message.as_bytes());

        Vote {
            tx_id: tx_id.to_string(),
            node_id: self.node_keypair.public_key(),
            approve: decision.approve,
            signature,
            timestamp: Utc::now(),
        }
    }
}
```

#### 7.2. P2P Vote Broadcasting (TAMAMLANMALI)
**Dosya**: `production/crates/orchestrator/src/service.rs:234-238`

**Mevcut (STUB)**:
```rust
// 4. Broadcast vote request to all nodes
// NOTE: In production, this would send P2P messages to all nodes
// For now, nodes will poll their local databases or receive via P2P receiver
```

**Gerçek Implementation**:
```rust
// 4. Broadcast vote request to all nodes via QUIC
let vote_request = VoteRequest {
    tx_id: tx.txid.clone(),
    unsigned_tx: tx.unsigned_tx.clone(),
    round_number: 1,
    deadline: Utc::now() + Duration::seconds(30),
};

for peer_id in &self.peer_registry.get_all_peers() {
    self.network.send_message(
        peer_id,
        Message::VoteRequest(vote_request.clone())
    ).await?;
}
```

#### 7.3. Node Background Vote Service (YENİ COMPONENT)
**Dosya**: `production/crates/orchestrator/src/vote_service.rs` (OLUŞTURULMALI)

```rust
pub struct NodeVoteService {
    vote_engine: Arc<VoteEngine>,
    vote_processor: Arc<VoteProcessor>,
    network: Arc<NetworkService>,
}

impl NodeVoteService {
    /// Background task that listens for vote requests
    pub async fn run(&self) {
        loop {
            // Listen for vote requests from coordinator
            if let Some(vote_request) = self.network.receive_vote_request().await {
                // Decide vote
                let decision = self.vote_engine.decide_vote(&vote_request.tx).await?;

                // Generate signed vote
                let vote = self.vote_engine.generate_vote(&vote_request.tx_id, decision);

                // Send vote to coordinator
                self.network.send_vote_response(&vote).await?;

                info!("Voted {} for tx {}", decision, vote_request.tx_id);
            }
        }
    }
}
```

**Beklenen Sonuç** (uzak gelecek):
- ✅ Coordinator vote request broadcast ediyor (P2P)
- ✅ Node'lar otomatik vote request'leri alıyor
- ✅ Node'lar transaction'ı validate edip approve/reject kararı veriyor
- ✅ Node'lar Ed25519 signed vote gönderiyor
- ✅ VoteProcessor otomatik vote'ları işliyor

**Süre**: 4-6 saat (ama gelecek için)


---

## 📊 TAMAMLANMA DURUMU

### Genel İlerleme
- **Session 7 Öncesi**: ~85% (4 kritik sorun vardı)
- **Session 7 Sonrası**: ~95% (4 kritik sorun çözüldü)
- **Şu An**: ~95% (5 küçük TODO kaldı)
- **Hedef**: %100 (bugün bu 5 TODO'yu bitir)

### Görev Dağılımı

| Öncelik | Görev Sayısı | Süre Tahmini |
|---------|--------------|--------------|
| 🔴 Yüksek (Bugün) | 3 görev | 5-8 saat |
| 🟡 Orta (Bugün) | 3 görev | 2-3 saat |
| 🔮 Gelecek | 1 görev | 4-6 saat |

**TOPLAM BUGÜN**: 6 görev, 7-11 saat

---

## ✅ KABUL KRİTERLERİ (BUGÜN BİTİNCE)

### Build Kriterleri
- ✅ `cargo check --workspace` - Hata yok
- ✅ `cargo clippy --workspace` - Major warning yok
- ✅ `cargo test --workspace` - Tüm testler geçiyor

### Fonksiyonel Kriterler
- ✅ DKG → Gerçek Bitcoin adresi üretiyor (bc1q... veya bc1p...)
- ✅ Signing → Signature doğru Bitcoin witness formatında
- ✅ etcd → Public key ve config storage çalışıyor
- ✅ API → Presignature pool gerçek data dönüyor
- ✅ API → Cluster info gerçek data dönüyor

### Code Quality Kriterleri
- ✅ Hiç TODO comment kalmadı (gelecek görevler hariç)
- ✅ Hiç mock/placeholder/stub kod yok (test dışı)
- ✅ Tüm kritik path'ler gerçek implementasyon kullanıyor

---

## 🚀 SONRAKİ ADIMLAR (BUGÜN SONRASI)

1. **Testnet Deployment** (1 gün)
   - Docker compose ile 5-node cluster deploy
   - Testnet'te DKG ceremonysi çalıştır
   - Test transaction gönder ve confirm bekle

2. **Integration Testing** (2 gün)
   - End-to-end test suite
   - Byzantine node simulation
   - Network partition recovery test

3. **Production Hardening** (3 gün)
   - mTLS enforcement (Görev 7)
   - Automatic voting (Görev 6)
   - BIP-341 tagged hash (Görev 8)
   - Security audit

4. **Documentation** (1 gün)
   - Operator runbook
   - API documentation
   - Deployment guide

---

## 📁 DEĞİŞTİRİLECEK DOSYALAR (BUGÜN)

### Yüksek Öncelikli
1. `production/crates/orchestrator/src/service.rs` (Görev 1: Bitcoin witness encoding)
2. `production/crates/orchestrator/src/dkg_service.rs` (Görev 2: Arc<Mutex<>> refactor)
3. `production/crates/orchestrator/src/presig_service.rs` (Görev 2: Arc<Mutex<>> refactor)
4. `production/crates/orchestrator/src/aux_info_service.rs` (Görev 2: Arc<Mutex<>> refactor)
5. `production/crates/network/src/quic_listener.rs` (Görev 6: mTLS enforcement)
6. `production/crates/network/Cargo.toml` (Görev 6: x509-parser dependency)

### Orta Öncelikli
7. `production/crates/api/src/handlers/dkg.rs` (Görev 3: Real address generation)
8. `production/crates/api/src/handlers/presig.rs` (Görev 4: Pool status integration)
9. `production/crates/api/src/handlers/cluster.rs` (Görev 5: Cluster info)

### Builder Pattern'ler (Görev 2 için)
10. `production/crates/api/src/handlers/*.rs` (tüm builder kullanımları)

---

**🎯 HEDEF**: Bu 6 görevi bugün tamamla, %100'e ulaş, testlere başla! 🚀

---

## 📝 GÖREV 7 ve 8 HAKKINDA AÇIKLAMA

**Sorunuz**: "Görev 7 ve 8'i neden uzak geleceğe ekledin peki?"

**Cevap**:

### ❌ Görev 8 (BIP-341 Tagged Hash) → ZATEN YAPILMIŞ, SILINMELI
Kod incelemesinde görüldü ki bu görev **zaten tamamlanmış**:
- `protocols/src/frost/signing.rs:291-302` - `givre::signing::<Bitcoin>` kullanılıyor
- `givre` kütüphanesi BIP-340/341 standartlarını implement ediyor
- `set_taproot_tweak(None)` çağrısı BIP-341 uyumlu
- **TODO YOK**, gerçek implementation var

### ✅ Görev 7 (mTLS Enforcement) → BUGÜN ACİL'E TAŞINDI
Haklısınız, bu GÜVENLİK AÇIĞI ve bugün bitirilmeli:
- Şu an sistem unauthenticated connection'ları kabul ediyor
- Byzantine saldırganlar sisteme girebilir
- `quic_listener.rs:157-170` - "For now, allow connection for development" comment'i var
- `extract_peer_node_id` fonksiyonu stub (None dönüyor)

**SONUÇ**:
- Görev 7 (eski) → **Görev 6'ya taşındı ve BUGÜN ACİL BİTMELİ olarak işaretlendi**
- Görev 8 → **Silindi** (zaten yapılmış)
- Toplam bugün bitirilecek görev: 6 (3 yüksek öncelikli, 3 orta öncelikli)
