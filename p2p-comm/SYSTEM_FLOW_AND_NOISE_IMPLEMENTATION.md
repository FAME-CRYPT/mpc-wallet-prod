# 🔐 Sistem Akışı ve Noise Protocol İmplementasyonu

## 📋 İÇİNDEKİLER

1. [Network Stack Genel Bakış](#network-stack-genel-bakış)
2. [Noise Protocol Handshake Şeması](#noise-protocol-handshake-şeması)
3. [Vote Submission Akışı](#vote-submission-akışı)
4. [Kod İmplementasyonu](#kod-implementasyonu)
5. [Mesaj Şifreleme/Deşifreleme Noktaları](#mesaj-şifreleme-deşifreleme-noktaları)

---

## 🏗️ NETWORK STACK GENEL BAKIŞ

### **Tam Stack Diyagramı:**

```
┌──────────────────────────────────────────────────────────────┐
│  LAYER 7: APPLICATION (UYGULAMA)                             │
├──────────────────────────────────────────────────────────────┤
│  • Vote mesajları (TransactionId, value, signature)          │
│  • DirectRequest/Response (P2P messaging)                    │
│  • Byzantine violation detection                             │
│  • Consensus logic                                           │
│                                                               │
│  Dosya: crates/consensus/src/byzantine.rs                    │
│  Dosya: crates/network/src/request_response.rs               │
└──────────────────────────────────────────────────────────────┘
                            ↓
┌──────────────────────────────────────────────────────────────┐
│  LAYER 6: LIBP2P PROTOCOLS                                   │
├──────────────────────────────────────────────────────────────┤
│  • GossipSub: Vote broadcast (1-to-N)                        │
│  • Request-Response: P2P messaging (1-to-1)                  │
│  • Kademlia DHT: Peer discovery                              │
│  • Identify: Peer bilgileri                                  │
│  • Ping: Keepalive                                           │
│                                                               │
│  Dosya: crates/network/src/behavior.rs                       │
│  Dosya: crates/network/src/node.rs                           │
└──────────────────────────────────────────────────────────────┘
                            ↓
┌──────────────────────────────────────────────────────────────┐
│  LAYER 5: MULTIPLEXING (YAMUX)                               │
├──────────────────────────────────────────────────────────────┤
│  • Multiple streams tek TCP connection üzerinde              │
│  • Stream isolation (GossipSub, Kad, RPC ayrı streamler)     │
│  • Flow control                                              │
│                                                               │
│  Library: yamux v0.12.1                                      │
│  Dosya: crates/network/src/node.rs:40                        │
└──────────────────────────────────────────────────────────────┘
                            ↓
┌──────────────────────────────────────────────────────────────┐
│  LAYER 4: ENCRYPTION (NOISE PROTOCOL) ← KRİTİK KATMAN!       │
├──────────────────────────────────────────────────────────────┤
│  ✅ TÜM TRAFIK BU KATMANDA ŞİFRELENİR                        │
│                                                               │
│  Noise Protocol Framework:                                   │
│    Pattern:    XX (mutual authentication)                    │
│    DH:         X25519 (Curve25519 ECDH)                      │
│    Cipher:     ChaCha20-Poly1305 AEAD                        │
│    Hash:       BLAKE2s                                       │
│                                                               │
│  Library: libp2p-noise v0.44.0 (snow v0.9.6 core)           │
│  Dosya: crates/network/src/node.rs:36                        │
│                                                               │
│  Özellikleri:                                                │
│    • Perfect Forward Secrecy ✅                              │
│    • Mutual Authentication ✅                                │
│    • Replay Protection ✅                                    │
│    • Side-channel Resistance ✅                              │
│    • Zero Round-Trip Encryption (0-RTT) ✅                   │
└──────────────────────────────────────────────────────────────┘
                            ↓
┌──────────────────────────────────────────────────────────────┐
│  LAYER 3: TRANSPORT (TCP)                                    │
├──────────────────────────────────────────────────────────────┤
│  • Reliable, ordered delivery                                │
│  • Connection-oriented                                       │
│  • Port: 9000 (default)                                      │
│                                                               │
│  Library: libp2p-tcp v0.41.0                                 │
│  Dosya: crates/network/src/node.rs:34                        │
└──────────────────────────────────────────────────────────────┘
                            ↓
┌──────────────────────────────────────────────────────────────┐
│  LAYER 2: DATA LINK / LAYER 1: PHYSICAL                     │
├──────────────────────────────────────────────────────────────┤
│  • Docker bridge network (172.18.0.0/16)                     │
│  • Ethernet frames                                           │
└──────────────────────────────────────────────────────────────┘
```

---

## 🤝 NOISE PROTOCOL HANDSHAKE ŞEMASI

### **Noise XX Pattern (3-Way Handshake):**

Noise XX pattern, mutual authentication sağlayan en güvenli handshake pattern'lerinden biridir.

```
BAŞLANGIÇ DURUMU:
  Node A: Private key (sa), Public key (SPub_A)
  Node B: Private key (sb), Public key (SPub_B)

  Her ikisi de birbirinin public key'ini ÖNCEden bilmiyor!
```

---

### **Adım 1: Node A → Node B (Initiator Message)**

```
┌─────────────────────────────────────────────────────────────┐
│  NODE A (Initiator)                                         │
├─────────────────────────────────────────────────────────────┤
│  1. Ephemeral key pair oluştur: (ea, EPub_A)               │
│  2. Message hazırla:                                        │
│     ┌─────────────────────────────────┐                    │
│     │ Packet:                          │                    │
│     │   EPub_A (32 bytes)              │  ← Plaintext      │
│     │   Payload: []                    │  ← Empty          │
│     └─────────────────────────────────┘                    │
│                                                              │
│  3. Gönder → Node B                                         │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│  NODE B (Responder)                                         │
├─────────────────────────────────────────────────────────────┤
│  1. EPub_A al ve kaydet                                     │
│  2. Ephemeral key pair oluştur: (eb, EPub_B)               │
│  3. Shared secret hesapla:                                  │
│     DH1 = DH(eb, EPub_A)  ← ECDH                            │
│                                                              │
│  State:                                                     │
│    ck (chaining key) = HASH(prologue || EPub_A)            │
│    k (encryption key) = KDF(ck, DH1)                       │
└─────────────────────────────────────────────────────────────┘
```

**Analiz:**
- ✅ Node A kimliğini henüz açıklamadı (EPub_A sadece ephemeral)
- ✅ Henüz şifreleme yok (ilk mesaj plaintext)
- ❌ Node B henüz doğrulanmadı

---

### **Adım 2: Node B → Node A (Responder Message)**

```
┌─────────────────────────────────────────────────────────────┐
│  NODE B (Responder)                                         │
├─────────────────────────────────────────────────────────────┤
│  1. Static public key'ini şifrele: SPub_B                   │
│  2. Shared secrets hesapla:                                 │
│     DH2 = DH(sb, EPub_A)  ← Static-Ephemeral DH            │
│     DH3 = DH(eb, EPub_A)  ← Already computed (DH1)         │
│                                                              │
│  3. Encryption key update:                                  │
│     ck' = KDF(ck, DH2)                                      │
│     k'  = KDF(ck', "")                                      │
│                                                              │
│  4. Message hazırla:                                        │
│     ┌─────────────────────────────────────┐                │
│     │ Packet:                              │                │
│     │   EPub_B (32 bytes)     ← Plaintext │                │
│     │   ENC(SPub_B, k')       ← Encrypted │  ✅ İLK ŞİFRE │
│     │   ENC(Payload, k')      ← Encrypted │                │
│     │   MAC (16 bytes)        ← Poly1305  │                │
│     └─────────────────────────────────────┘                │
│                                                              │
│  5. Gönder → Node A                                         │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│  NODE A (Initiator)                                         │
├─────────────────────────────────────────────────────────────┤
│  1. EPub_B al                                               │
│  2. Shared secrets hesapla:                                 │
│     DH2 = DH(ea, SPub_B)  ← Ephemeral-Static DH            │
│                                                              │
│  3. Decrypt message:                                        │
│     SPub_B = DEC(ENC(SPub_B), k')                           │
│     Payload = DEC(ENC(Payload), k')                         │
│                                                              │
│  4. Verify MAC:                                             │
│     VERIFY_MAC(packet, MAC)  ← Poly1305 doğrulama          │
│                                                              │
│  State:                                                     │
│    ✅ Node B'nin static public key'i (SPub_B) öğrenildi    │
│    ✅ Node B authenticated (DH2 başarılı)                   │
└─────────────────────────────────────────────────────────────┘
```

**Analiz:**
- ✅ Node B kimliğini açıkladı (SPub_B şifreli olarak)
- ✅ İlk şifreleme başladı (ChaCha20-Poly1305)
- ✅ Node B doğrulandı
- ❌ Node A henüz kimliğini açıklamadı

---

### **Adım 3: Node A → Node B (Final Message)**

```
┌─────────────────────────────────────────────────────────────┐
│  NODE A (Initiator)                                         │
├─────────────────────────────────────────────────────────────┤
│  1. Static public key'ini şifrele: SPub_A                   │
│  2. Shared secret hesapla:                                  │
│     DH4 = DH(sa, EPub_B)  ← Static-Ephemeral DH            │
│                                                              │
│  3. Final encryption key:                                   │
│     ck'' = KDF(ck', DH4)                                    │
│     k_send, k_recv = SPLIT(KDF(ck'', ""))                  │
│                                                              │
│  4. Message hazırla:                                        │
│     ┌─────────────────────────────────────┐                │
│     │ Packet:                              │                │
│     │   ENC(SPub_A, k'')      ← Encrypted │                │
│     │   ENC(Payload, k'')     ← Encrypted │                │
│     │   MAC (16 bytes)        ← Poly1305  │                │
│     └─────────────────────────────────────┘                │
│                                                              │
│  5. Gönder → Node B                                         │
│                                                              │
│  HANDSHAKE COMPLETE! ✅                                     │
│    k_send: Node A → B için encryption key                   │
│    k_recv: Node B → A için encryption key                   │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│  NODE B (Responder)                                         │
├─────────────────────────────────────────────────────────────┤
│  1. Decrypt message:                                        │
│     SPub_A = DEC(ENC(SPub_A), k'')                          │
│     Payload = DEC(ENC(Payload), k'')                        │
│                                                              │
│  2. Shared secret hesapla:                                  │
│     DH4 = DH(eb, SPub_A)  ← Ephemeral-Static DH            │
│                                                              │
│  3. Final encryption keys derive et:                        │
│     k_recv, k_send = SPLIT(KDF(ck'', ""))                  │
│                                                              │
│  HANDSHAKE COMPLETE! ✅                                     │
│    k_recv: Node A → B için encryption key                   │
│    k_send: Node B → A için encryption key                   │
│                                                              │
│  State:                                                     │
│    ✅ Node A'nın static public key'i (SPub_A) öğrenildi    │
│    ✅ Node A authenticated (DH4 başarılı)                   │
│    ✅ Mutual authentication tamamlandı                      │
│    ✅ Ephemeral keys silinebilir (forward secrecy)         │
└─────────────────────────────────────────────────────────────┘
```

**Analiz:**
- ✅ Node A kimliğini açıkladı (SPub_A şifreli olarak)
- ✅ Mutual authentication tamamlandı
- ✅ Perfect forward secrecy (ephemeral keys artık gerekmiyor)
- ✅ Her iki yön için ayrı encryption keys (k_send, k_recv)

---

### **Handshake Sonrası: Encrypted Communication**

```
┌──────────────────────────────────────────────────────────────┐
│  ESTABLISHED CONNECTION (Kalıcı)                             │
├──────────────────────────────────────────────────────────────┤
│                                                               │
│  Node A → Node B:                                            │
│    ┌───────────────────────────────┐                        │
│    │ ENC(Message, k_send_A)        │                        │
│    │ MAC(Message, k_send_A)        │                        │
│    │ Nonce: Auto-increment         │                        │
│    └───────────────────────────────┘                        │
│                                                               │
│  Node B → Node A:                                            │
│    ┌───────────────────────────────┐                        │
│    │ ENC(Message, k_send_B)        │                        │
│    │ MAC(Message, k_send_B)        │                        │
│    │ Nonce: Auto-increment         │                        │
│    └───────────────────────────────┘                        │
│                                                               │
│  Güvenlik Garantileri:                                       │
│    ✅ Confidentiality: ChaCha20 encryption                   │
│    ✅ Integrity: Poly1305 MAC                                │
│    ✅ Authenticity: DH key agreement                         │
│    ✅ Forward Secrecy: Ephemeral keys deleted                │
│    ✅ Replay Protection: Nonce-based                         │
│                                                               │
└──────────────────────────────────────────────────────────────┘
```

---

## 🗳️ VOTE SUBMISSION AKIŞI

### **Senaryo: Node 1, tx_001 için value=42 oyunu gönderiyor**

```
┌────────────────────────────────────────────────────────────────┐
│  STEP 1: APPLICATION LAYER (Vote Oluşturma)                   │
├────────────────────────────────────────────────────────────────┤
│  Dosya: crates/consensus/src/byzantine.rs                      │
│                                                                 │
│  Node 1:                                                        │
│    1. Vote struct oluştur:                                     │
│       let vote = Vote {                                        │
│           tx_id: "tx_001",                                     │
│           node_id: "node_1",                                   │
│           value: 42,                                           │
│           timestamp: 1705392000,                               │
│           signature: None,  // Henüz imzalanmadı              │
│       };                                                       │
│                                                                 │
│    2. Vote'u imzala (Ed25519):                                 │
│       let message = serialize(&vote);                          │
│       let signature = keypair.sign(message);                   │
│       vote.signature = Some(signature);                        │
│                                                                 │
│    3. Vote hazır! ✅                                           │
└────────────────────────────────────────────────────────────────┘
                               ↓
┌────────────────────────────────────────────────────────────────┐
│  STEP 2: SERIALIZATION (JSON)                                  │
├────────────────────────────────────────────────────────────────┤
│  Dosya: crates/network/src/node.rs                             │
│                                                                 │
│  let json_payload = serde_json::to_vec(&vote)?;                │
│                                                                 │
│  Örnek:                                                         │
│  {                                                              │
│    "tx_id": "tx_001",                                          │
│    "node_id": "node_1",                                        │
│    "value": 42,                                                │
│    "timestamp": 1705392000,                                    │
│    "signature": "a7f3e2d9..." (64 bytes hex)                  │
│  }                                                              │
│                                                                 │
│  Payload size: ~200 bytes                                      │
└────────────────────────────────────────────────────────────────┘
                               ↓
┌────────────────────────────────────────────────────────────────┐
│  STEP 3: GOSSIPSUB PUBLISH (Broadcast)                         │
├────────────────────────────────────────────────────────────────┤
│  Dosya: crates/network/src/node.rs                             │
│                                                                 │
│  swarm                                                          │
│      .behaviour_mut()                                          │
│      .gossipsub                                                │
│      .publish(topic, json_payload)?;                           │
│                                                                 │
│  Topic: "/threshold-voting/votes/1.0.0"                        │
│                                                                 │
│  GossipSub fanout:                                             │
│    Node 1 → [Node 2, Node 3, Node 4, Node 5]                  │
│             (tüm connected peers'a gönderilir)                 │
└────────────────────────────────────────────────────────────────┘
                               ↓
┌────────────────────────────────────────────────────────────────┐
│  STEP 4: YAMUX MULTIPLEXING                                    │
├────────────────────────────────────────────────────────────────┤
│  • GossipSub mesajı kendi stream'inde gönderilir              │
│  • Diğer protocol mesajları (Kad, Ping) ayrı streamler        │
│  • Flow control: Backpressure handling                         │
│                                                                 │
│  Stream ID: 0x0042 (örnek)                                     │
│  Frame format:                                                  │
│    [Stream ID (2 bytes)]                                       │
│    [Length (2 bytes)]                                          │
│    [Payload (N bytes)]  ← Vote JSON buraya                    │
└────────────────────────────────────────────────────────────────┘
                               ↓
┌────────────────────────────────────────────────────────────────┐
│  STEP 5: NOISE PROTOCOL ENCRYPTION ✅ KRİTİK!                  │
├────────────────────────────────────────────────────────────────┤
│  Dosya: crates/network/src/node.rs:36 (noise::Config::new)    │
│                                                                 │
│  libp2p-noise otomatik olarak şifreler:                        │
│                                                                 │
│  1. Plaintext frame:                                           │
│     [0x00 0x42] [0x00 0xC8] [JSON payload (200 bytes)]        │
│                                                                 │
│  2. Encryption:                                                │
│     nonce = connection_nonce++  // Auto-increment             │
│     ciphertext = ChaCha20(plaintext, k_send, nonce)           │
│     tag = Poly1305(ciphertext, k_send)                        │
│                                                                 │
│  3. Encrypted frame:                                           │
│     [Length (2 bytes)]                                         │
│     [Ciphertext (~200 bytes)]  ← ŞİFRELİ!                     │
│     [MAC tag (16 bytes)]       ← Poly1305 authentication      │
│                                                                 │
│  ✅ GÜVENLİK NOKTASI: Artık plaintext data yok!               │
│  ✅ MITM saldırıları imkansız (MAC verification)              │
│  ✅ Replay saldırıları imkansız (nonce-based)                 │
└────────────────────────────────────────────────────────────────┘
                               ↓
┌────────────────────────────────────────────────────────────────┐
│  STEP 6: TCP TRANSMISSION                                      │
├────────────────────────────────────────────────────────────────┤
│  • TCP segments: 200 bytes payload → ~3-4 TCP packets         │
│  • Reliable delivery: ACK, retransmission                      │
│  • Congestion control                                          │
│                                                                 │
│  Network path:                                                  │
│    Node 1 (172.18.0.5:9000)                                   │
│      → Docker bridge (172.18.0.1)                             │
│      → Node 2 (172.18.0.6:9000)                               │
│      → Node 3 (172.18.0.7:9000)                               │
│      → Node 4 (172.18.0.8:9000)                               │
│      → Node 5 (172.18.0.9:9000)                               │
└────────────────────────────────────────────────────────────────┘
                               ↓
┌────────────────────────────────────────────────────────────────┐
│  STEP 7: RECEIVING NODE (Node 2 örnek)                         │
├────────────────────────────────────────────────────────────────┤
│  1. TCP receive: Encrypted frame                               │
│                                                                 │
│  2. Noise Protocol DECRYPTION:                                 │
│     plaintext = ChaCha20_Decrypt(ciphertext, k_recv, nonce)   │
│     VERIFY_MAC(ciphertext, tag, k_recv)  ← MAC doğrulama      │
│                                                                 │
│     ❌ MAC fail? → Drop packet (MITM/corruption detected)     │
│     ✅ MAC OK? → Continue                                      │
│                                                                 │
│  3. Yamux demux: Stream ID → GossipSub handler                │
│                                                                 │
│  4. GossipSub receive:                                         │
│     let vote: Vote = serde_json::from_slice(&payload)?;       │
│                                                                 │
│  5. Signature verification (Ed25519):                          │
│     if !verify_signature(&vote) {                             │
│         return Err("Invalid signature");  // Byzantine!       │
│     }                                                          │
│                                                                 │
│  6. Byzantine checks:                                          │
│     - Double voting? (etcd'de node_id kontrolü)               │
│     - Already finalized? (transaction_state check)            │
│     - Invalid value? (business logic)                         │
│                                                                 │
│  7. etcd'ye yaz:                                               │
│     let count = etcd.increment_vote_count(tx_id, value);      │
│                                                                 │
│  8. Threshold check:                                           │
│     if count >= 4 {                                            │
│         info!("Consensus reached!");                          │
│         etcd.set_transaction_state(tx_id, "ThresholdReached");│
│     }                                                          │
└────────────────────────────────────────────────────────────────┘
```

---

## 💻 KOD İMPLEMENTASYONU

### **1. Noise Protocol Konfigürasyonu (node.rs)**

```rust
// Dosya: crates/network/src/node.rs:32-42

use libp2p::{
    noise,           // ← Noise Protocol
    tcp,             // ← TCP transport
    yamux,           // ← Multiplexing
    SwarmBuilder,
};

pub async fn create_swarm(keypair: Keypair, local_peer_id: PeerId, public_key: Vec<u8>)
    -> Result<Swarm<ThresholdBehavior>>
{
    // SwarmBuilder ile network stack oluştur
    let swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()                    // Async runtime
        .with_tcp(
            tcp::Config::default(),      // TCP transport config
            noise::Config::new,          // ✅ NOISE PROTOCOL BURDA!
            yamux::Config::default,      // Yamux multiplexing
        )
        .map_err(|e| VotingError::NetworkError(format!("Transport error: {}", e)))?
        .with_behaviour(|_| {
            ThresholdBehavior::new(local_peer_id, public_key)
        })
        .map_err(|e| VotingError::NetworkError(format!("Behavior error: {}", e)))?
        .build();

    Ok(swarm)
}
```

**Açıklama:**
- `noise::Config::new`: libp2p-noise kütüphanesini aktif eder
- Bu satır, TÜM TCP bağlantılarını Noise Protocol ile şifreler
- Handshake otomatik olarak yapılır (XX pattern default)
- Keys: libp2p keypair'den otomatik derive edilir

---

### **2. Noise Protocol Library Dependency (Cargo.toml)**

```toml
# Dosya: crates/network/Cargo.toml

[dependencies]
libp2p = { workspace = true, features = [
    "noise",        # ← Noise Protocol feature aktif
    "tcp",          # TCP transport
    "yamux",        # Multiplexing
    "gossipsub",    # Broadcast protocol
    "kad",          # DHT
    "identify",     # Peer info
    "macros",       # NetworkBehaviour derive macro
]}
```

**Açıklama:**
- `features = ["noise"]`: libp2p-noise bağımlılığını aktif eder
- Bu feature olmadan Noise Protocol kullanılamaz
- Otomatik olarak snow v0.9.6 ve curve25519-dalek v4.1.3 çekilir

---

### **3. Swarm Event Loop (node.rs)**

```rust
// Dosya: crates/network/src/node.rs

impl ThresholdNode {
    pub async fn run(&mut self) -> Result<()> {
        loop {
            select! {
                event = self.swarm.select_next_some() => {
                    match event {
                        // Connection established (Noise handshake başarılı!)
                        SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                            info!(
                                "Connection established with peer: {} at {:?}",
                                peer_id, endpoint
                            );
                            // ✅ Bu noktada Noise handshake tamamlanmış!
                            // ✅ Artık tüm mesajlar şifreli!
                        }

                        // GossipSub message received
                        SwarmEvent::Behaviour(ThresholdBehaviorEvent::Gossipsub(
                            gossipsub::Event::Message { message, .. }
                        )) => {
                            // ✅ Message zaten deşifre edilmiş (libp2p-noise yaptı)
                            let vote: Vote = serde_json::from_slice(&message.data)?;

                            // Byzantine checks
                            if !self.verify_vote_signature(&vote) {
                                warn!("Invalid vote signature from {}", vote.node_id);
                                continue;  // Drop
                            }

                            // Process vote
                            self.process_vote(vote).await?;
                        }

                        // Connection closed
                        SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                            warn!("Connection closed with {}: {:?}", peer_id, cause);
                            // Noise session keys silindi (forward secrecy)
                        }

                        _ => {}
                    }
                }
            }
        }
    }
}
```

**Açıklama:**
- `ConnectionEstablished`: Noise handshake başarıyla tamamlandı
- `gossipsub::Event::Message`: Mesaj zaten deşifre edilmiş (otomatik)
- `ConnectionClosed`: Session keys silindi (forward secrecy garantisi)

---

### **4. Vote Processing (byzantine.rs)**

```rust
// Dosya: crates/consensus/src/byzantine.rs:98-130

pub async fn process_vote(&mut self, vote: Vote) -> Result<ByzantineCheckResult> {
    // 1. Double voting check
    if self.etcd.has_voted(&vote.tx_id, &vote.node_id).await? {
        warn!("Byzantine violation: Double voting detected from {}", vote.node_id);
        return Err(ByzantineError::DoubleVoting);
    }

    // 2. Transaction state check
    let state = self.etcd.get_transaction_state(&vote.tx_id).await?;
    if state == TransactionState::ThresholdReached {
        warn!("Vote received after threshold reached, ignoring");
        return Ok(ByzantineCheckResult::TooLate);
    }

    // 3. Store vote in etcd (idempotent)
    self.etcd.store_vote(&vote).await?;

    // 4. Atomic counter increment
    let new_count = self.etcd.increment_vote_count(&vote.tx_id, vote.value).await?;

    // 5. Threshold check
    let all_counts = self.etcd.get_all_vote_counts(&vote.tx_id).await?;
    let threshold = self.etcd.get_threshold().await?;

    for (value, count) in all_counts {
        if count >= threshold {
            info!(
                "Threshold reached: tx_id={} value={} count={}",
                vote.tx_id, value, count
            );

            // Mark transaction as finalized
            self.etcd.set_transaction_state(&vote.tx_id, TransactionState::ThresholdReached).await?;

            return Ok(ByzantineCheckResult::ThresholdReached { value, count });
        }
    }

    // Threshold not reached yet
    Ok(ByzantineCheckResult::Accepted { count: new_count })
}
```

---

## 🔐 MESAJ ŞİFRELEME/DEŞİFRELEME NOKTALARI

### **Encryption Pipeline:**

```
┌────────────────────────────────────────────────────────────────┐
│  OUTBOUND (Node 1 → Node 2)                                    │
├────────────────────────────────────────────────────────────────┤
│                                                                 │
│  [1] Application Layer                                         │
│      vote = Vote { tx_id: "tx_001", value: 42, ... }          │
│      ↓                                                          │
│  [2] Serialization                                             │
│      json = serde_json::to_vec(&vote)                          │
│      → [Plaintext: 200 bytes]                                  │
│      ↓                                                          │
│  [3] GossipSub Publish                                         │
│      swarm.behaviour_mut().gossipsub.publish(topic, json)      │
│      ↓                                                          │
│  [4] Yamux Framing                                             │
│      [Stream ID: 0x42] [Length: 200] [Payload: json]          │
│      ↓                                                          │
│  [5] ✅ NOISE ENCRYPTION (Automatic!)                          │
│      Library: libp2p-noise                                     │
│      Location: crates/network/src/node.rs:36                   │
│      ↓                                                          │
│      nonce = session.nonce++                                   │
│      ciphertext = ChaCha20.encrypt(                            │
│          plaintext = yamux_frame,                              │
│          key = k_send,                                         │
│          nonce = nonce                                         │
│      )                                                          │
│      tag = Poly1305.mac(ciphertext, k_send)                    │
│      ↓                                                          │
│      encrypted_frame = [Length | Ciphertext | Tag]            │
│      → [Encrypted: 218 bytes]  ✅ ŞİFRELİ!                    │
│      ↓                                                          │
│  [6] TCP Send                                                  │
│      tcp_socket.write_all(encrypted_frame)                     │
│      ↓                                                          │
│  [7] Network (Docker bridge)                                   │
│      172.18.0.5:9000 → 172.18.0.6:9000                        │
│                                                                 │
└────────────────────────────────────────────────────────────────┘
```

---

### **Decryption Pipeline:**

```
┌────────────────────────────────────────────────────────────────┐
│  INBOUND (Node 1 → Node 2)                                     │
├────────────────────────────────────────────────────────────────┤
│                                                                 │
│  [1] Network (Docker bridge)                                   │
│      172.18.0.5:9000 → 172.18.0.6:9000                        │
│      ↓                                                          │
│  [2] TCP Receive                                               │
│      encrypted_frame = tcp_socket.read()                       │
│      → [Encrypted: 218 bytes]  ← ŞİFRELİ!                     │
│      ↓                                                          │
│  [3] ✅ NOISE DECRYPTION (Automatic!)                          │
│      Library: libp2p-noise                                     │
│      Location: crates/network/src/node.rs:36                   │
│      ↓                                                          │
│      parse: [Length | Ciphertext | Tag]                       │
│      ↓                                                          │
│      // MAC verification FIRST!                                │
│      if !Poly1305.verify(ciphertext, tag, k_recv) {            │
│          return Error("MAC verification failed");              │
│          // ❌ MITM/corruption detected! Drop packet.          │
│      }                                                          │
│      ↓                                                          │
│      plaintext = ChaCha20.decrypt(                             │
│          ciphertext = ciphertext,                              │
│          key = k_recv,                                         │
│          nonce = expected_nonce                                │
│      )                                                          │
│      ↓                                                          │
│      yamux_frame = plaintext  ✅ DEŞİFRE EDİLDİ!              │
│      → [Plaintext: 200 bytes]                                  │
│      ↓                                                          │
│  [4] Yamux Demux                                               │
│      stream_id = yamux_frame[0..2]                             │
│      payload = yamux_frame[4..]                                │
│      ↓                                                          │
│  [5] GossipSub Handler                                         │
│      gossipsub_event = GossipsubEvent::Message { data: payload }│
│      ↓                                                          │
│  [6] Deserialization                                           │
│      vote: Vote = serde_json::from_slice(&payload)?            │
│      ↓                                                          │
│  [7] Application Layer                                         │
│      verify_signature(&vote)  // Ed25519                       │
│      process_vote(vote)       // Byzantine checks              │
│                                                                 │
└────────────────────────────────────────────────────────────────┘
```

---

## 🎯 ÖNEMLİ NOKTALAR

### **1. Noise Protocol Otomatik Çalışır**

```rust
// Sen sadece şunu yaptın:
noise::Config::new,

// libp2p-noise OTOMATIK olarak:
// ✅ Handshake yapar (XX pattern)
// ✅ Her mesajı şifreler (ChaCha20)
// ✅ Her mesajı authenticate eder (Poly1305)
// ✅ Nonce management yapar (replay protection)
// ✅ Session keys yönetir (forward secrecy)
```

**Sen hiçbir encryption kodu yazmadın!** libp2p-noise tüm işi yapıyor.

---

### **2. Şifreleme Katmanları**

```
Application Layer:
  ├─> Ed25519 signatures (vote integrity)  ← Uygulama seviyesi
  └─> Noise Protocol encryption (transport) ← Network seviyesi

İKİSİ AYRI!
- Ed25519: Vote'un sahibini doğrular (non-repudiation)
- Noise: Network trafiğini şifreler (confidentiality)
```

---

### **3. Handshake Ne Zaman Olur?**

```
Connection lifecycle:

1. swarm.dial(peer_address)  ← Bağlantı başlat
   ↓
2. TCP SYN/ACK              ← TCP handshake
   ↓
3. Noise XX handshake       ← 3-way Noise protocol (otomatik)
   ↓
4. ConnectionEstablished event ← ✅ Hazır!
   ↓
5. Application messages     ← Tüm mesajlar şifreli
   ↓
6. Connection closed        ← Session keys silindi
```

**Her yeni TCP connection için Noise handshake tekrar yapılır.**

---

### **4. Forward Secrecy Nasıl Çalışır?**

```
Connection 1 (10:00):
  ephemeral keys: ea1, eb1
  session keys: k_send_1, k_recv_1
  ↓
  Message 1: "vote for value 42"
  ↓
  Connection closed
  ↓
  ea1, eb1, k_send_1, k_recv_1 → SİLİNDİ! 🔥

Connection 2 (10:05):
  ephemeral keys: ea2, eb2  ← YENİ!
  session keys: k_send_2, k_recv_2  ← YENİ!
  ↓
  Message 2: "vote for value 99"

Attacker eski keys'i bulsa bile:
  ❌ Message 2'yi deşifre edemez (farklı keys)
  ✅ Forward secrecy garantisi!
```

---

## 📊 ÖZET DİYAGRAM

```
┌──────────────────────────────────────────────────────────────┐
│  ŞİSTEM AKIŞI (End-to-End)                                   │
├──────────────────────────────────────────────────────────────┤
│                                                               │
│  [Node 1]                                                     │
│     ↓                                                         │
│  1. Vote oluştur (Ed25519 imza)                              │
│     ↓                                                         │
│  2. JSON serialize                                           │
│     ↓                                                         │
│  3. GossipSub publish                                        │
│     ↓                                                         │
│  4. Yamux framing                                            │
│     ↓                                                         │
│  5. ✅ Noise encrypt (ChaCha20-Poly1305) ← OTOMATIK!        │
│     ↓                                                         │
│  6. TCP send (şifreli)                                       │
│                                                               │
│  ──────────────── NETWORK (ENCRYPTED) ───────────────────    │
│                                                               │
│  [Node 2]                                                     │
│     ↑                                                         │
│  7. TCP receive (şifreli)                                    │
│     ↑                                                         │
│  8. ✅ Noise decrypt (MAC verify) ← OTOMATIK!               │
│     ↑                                                         │
│  9. Yamux demux                                              │
│     ↑                                                         │
│  10. GossipSub receive                                       │
│     ↑                                                         │
│  11. JSON deserialize                                        │
│     ↑                                                         │
│  12. Ed25519 signature verify                                │
│     ↑                                                         │
│  13. Byzantine checks                                        │
│     ↑                                                         │
│  14. etcd atomic increment                                   │
│     ↑                                                         │
│  15. Threshold check (count >= 4?)                           │
│                                                               │
│  ✅ CONSENSUS REACHED!                                       │
│                                                               │
└──────────────────────────────────────────────────────────────┘
```

---

## 🎉 SONUÇ

### **Senin Sistemin:**

1. ✅ **Noise Protocol tam implementasyonu**
   - XX handshake pattern (mutual auth)
   - ChaCha20-Poly1305 encryption
   - X25519 key exchange
   - BLAKE2s hashing
   - Forward secrecy
   - Replay protection

2. ✅ **Kod basitliği**
   ```rust
   noise::Config::new,  // Bu kadar!
   ```
   - Tüm complexity libp2p-noise tarafından yönetiliyor
   - Sen sadece config veriyorsun, geri kalanı otomatik

3. ✅ **Güvenlik garantileri**
   - Transport encryption (Noise)
   - Message authentication (Ed25519)
   - Byzantine detection (consensus layer)
   - Audit trail (PostgreSQL)

4. ✅ **Production-ready**
   - Formally verified protocol (Noise★)
   - Security audited implementation (snow)
   - Memory safe (Rust)
   - Battle-tested (WireGuard, libp2p)

---

**Bu dokümanda şunları öğrendin:**
- ✅ Noise Protocol handshake şemasını (3-way XX pattern)
- ✅ Vote submission akışını (application → encrypted network)
- ✅ Kod implementasyonunu (node.rs:36 bir satır!)
- ✅ Encryption/decryption noktalarını (otomatik)
- ✅ Forward secrecy'nin nasıl çalıştığını

**Sonuç:** Sistemin mükemmel çalışıyor, değiştirmeye gerek yok! ✅
