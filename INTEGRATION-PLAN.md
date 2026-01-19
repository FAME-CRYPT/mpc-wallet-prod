# 🔗 MPC-WALLET Entegrasyon Ana Planı

## 📋 Yönetici Özeti

**Hedef**: Bitcoin saklama için üç repository'yi birleştirerek Byzantine-proof threshold imza sistemi oluşturmak:

| Repository | Rol | Alınacak Bileşenler |
|------------|------|----------------|
| **p2p-comm** | **TEMEL** | libp2p networking, Byzantine tespiti, consensus voting, etcd+PostgreSQL storage |
| **threshold-signing** | **CGGMP24 Kaynağı** | DKG, aux_info, presignature pool, HTTP→libp2p transport adapter |
| **torcus-wallet** | **FROST + Bitcoin** | FROST Schnorr, Bitcoin transaction builder, P2WPKH/P2TR adres türetimi |

**Final Sistem**: Production-grade MPC cüzdan:
- **Node'lar** threshold imzalama protokollerini (CGGMP24 + FROST) libp2p üzerinden çalıştırır
- **Byzantine consensus** kötü niyetli node tespitini transaction broadcast öncesi sağlar
- **Submitter node** final blockchain gönderimini (exactly-once garantisi) yönetir

---

## 🎯 Bu Entegrasyonun Mantıklı Olma Nedenleri

### Teknik Uyumluluk Analizi

| Katman | p2p-comm | threshold-signing | torcus-wallet | Entegrasyon Stratejisi |
|-------|----------------|-------------------|---------------|---------------------|
| **Communication** | libp2p (GossipSub + Noise XX) | HTTP polling (500ms) | HTTP relay coordinator | ✅ **p2p-comm kazanır** - HTTP'yi libp2p ile değiştir |
| **Consensus** | Byzantine voting (4 tip) | Yok | Yok | ✅ **Signing flow'a ekle** - Submission öncesi oylamaya ekle |
| **Crypto Protocol** | Özel voting (Ed25519) | CGGMP24 (secp256k1) | CGGMP24 + FROST | ✅ **İkisini de çıkar** - CGGMP24'ü threshold-signing'den, FROST'u torcus-wallet'tan |
| **Storage** | etcd + PostgreSQL | In-memory | SQLite | ✅ **Birleştir** - etcd (coordination), PostgreSQL (audit), SQLite (local key shares) |
| **Bitcoin** | Yok | Yok | Tam stack (esplora, RPC, tx builder) | ✅ **torcus-wallet** - Tam Bitcoin entegrasyonu |

### Versiyon Uyumluluğu

**Kritik Sorun**: CGGMP24 versiyon uyumsuzluğu
- **threshold-signing**: `cggmp24 = "0.7.0-alpha.2"`
- **torcus-wallet**: `cggmp24 = "0.7.0-alpha.3"`

**Çözüm**: Tümünü `0.7.0-alpha.3`'e yükselt
- Risk: **DÜŞÜK** - Aynı protokol, küçük API değişiklikleri
- Test: DKG + signing roundtrip'leri doğrula

---

## 🏗️ Final Sistem Mimarisi

```
┌──────────────────────────────────────────────────────────────────────┐
│                         Client Layer                                 │
│  Web UI / Mobile App / CLI                                           │
└────────────────────────────┬─────────────────────────────────────────┘
                             │
┌────────────────────────────▼─────────────────────────────────────────┐
│                    API Gateway (Yeni)                                │
│  - Kullanıcı kimlik doğrulama (JWT, API keys)                       │
│  - İstek yönlendirme                                                 │
│  - Rate limiting                                                     │
└────────────────────────────┬─────────────────────────────────────────┘
                             │
        ┌────────────────────┼────────────────────┐
        │                    │                    │
┌───────▼──────────┐  ┌──────▼────────┐  ┌──────▼──────────┐
│ Consensus Layer  │  │ Signing Layer │  │ Submitter Node  │
│ (Byzantine Vote) │  │ (MPC Protocols)│  │ (Blockchain TX) │
└───────┬──────────┘  └──────┬────────┘  └──────┬──────────┘
        │                    │                    │
        │  ┌─────────────────┴────────────┐       │
        │  │                              │       │
    ┌───▼──▼──┐  ┌────────┐  ┌────────┐  ┌──────▼───┐
    │ Node-1  │  │ Node-2 │  │ Node-3 │  │ Node-4   │
    │ (Rust)  │  │ (Rust) │  │ (Rust) │  │ (Rust)   │
    │         │  │        │  │        │  │          │
    │ libp2p  │◄─┤ libp2p │◄─┤ libp2p │◄─┤ libp2p   │
    │ CGGMP24 │  │ CGGMP24│  │ CGGMP24│  │ CGGMP24  │
    │ FROST   │  │ FROST  │  │ FROST  │  │ FROST    │
    │ SQLite  │  │ SQLite │  │ SQLite │  │ SQLite   │
    └───┬─────┘  └───┬────┘  └───┬────┘  └───┬──────┘
        │            │            │            │
        └────────────┴────────────┴────────────┘
                     │
        ┌────────────┴────────────┐
        │                         │
┌───────▼────────┐    ┌──────────▼──────────┐
│  etcd Cluster  │    │  PostgreSQL DB      │
│  (Raft)        │    │  (Audit Trail)      │
│  - Vote sayıları│   │  - Vote geçmişi     │
│  - TX durumu   │    │  - Byzantine log    │
│  - Kilitler    │    │  - TX gönderimleri  │
└────────────────┘    └─────────────────────┘
                             │
                    ┌────────▼────────┐
                    │  Bitcoin Network│
                    │  (Testnet/Main) │
                    └─────────────────┘
```

### Bileşen Sorumlulukları

**Node (1-4)**:
- Threshold imzalama protokollerini çalıştırma (CGGMP24 veya FROST)
- Byzantine consensus voting'e katılma
- Key share'leri lokal olarak saklama (SQLite)
- libp2p üzerinden iletişim kurma (şifreli P2P)

**Submitter Node (Özel Node)**:
- Tamamlanmış imzalar + vote onayı alma
- Bitcoin transaction oluşturma
- Blockchain'e broadcast etme
- Distributed lock edinme (exactly-once garantisi)
- PostgreSQL audit trail'e loglama

**etcd Cluster**:
- Atomik vote sayımı (CAS operasyonları)
- Transaction state machine
- Distributed lock'lar (TTL-tabanlı)
- Konfigürasyon depolama

**PostgreSQL**:
- Tam vote geçmişi
- Byzantine ihlal kayıtları
- Blockchain gönderim logu
- Node itibar takibi

---

## 📦 Faz 1: Repository Hazırlığı (Günler 1-2)

### 1.1 Birleşik Çalışma Alanı Oluşturma

```bash
cd /c/Users/user/Desktop/MPC-WALLET

# Yeni birleşik crate yapısı oluştur
mkdir -p unified-mpc-wallet/crates

# Dizin yapısı:
unified-mpc-wallet/
├── Cargo.toml                    # Workspace tanımı
├── crates/
│   ├── types/                    # Paylaşılan tipler (p2p-comm'den)
│   ├── crypto/                   # Ed25519 + secp256k1
│   ├── network/                  # libp2p stack (p2p-comm'den)
│   ├── storage/                  # etcd + PostgreSQL + SQLite
│   ├── consensus/                # Byzantine voting (p2p-comm'den)
│   ├── protocols/                # CGGMP24 + FROST
│   │   ├── src/
│   │   │   ├── cggmp24/         # threshold-signing'den
│   │   │   └── frost/           # torcus-wallet'tan
│   ├── chains/                   # Bitcoin entegrasyonu (torcus-wallet'tan)
│   ├── node/                     # MPC node binary
│   ├── submitter/                # YENİ: Blockchain submitter node
│   ├── coordinator/              # Protokol orkestrasyonu (torcus-wallet'tan)
│   └── cli/                      # Command-line interface
├── docker/
│   ├── Dockerfile.node
│   ├── Dockerfile.submitter
│   └── Dockerfile.coordinator
├── docker-compose.yml
└── scripts/
    ├── start-regtest.sh
    ├── start-testnet.sh
    └── verify.sh
```

### 1.2 Versiyon Hizalama

**Aksiyon Maddeleri**:

1. **CGGMP24 Yükseltme** (tüm repo'lar → `0.7.0-alpha.3`):
   ```toml
   # Tüm Cargo.toml dosyalarında
   [dependencies]
   cggmp24 = { version = "0.7.0-alpha.3", features = ["all-groups"] }
   ```

2. **API Değişikliklerini Doğrula**:
   ```bash
   # DKG roundtrip test et
   cargo test --package protocols test_cggmp24_dkg

   # Signing test et
   cargo test --package protocols test_cggmp24_signing
   ```

3. **Breaking Change'leri Belgele**:
   - `cggmp24` CHANGELOG'u kontrol et
   - Gerekirse function signature'ları güncelle
   - Odaklan:
     - `keygen()` API
     - `signing()` API
     - `aux_info_gen()` API

### 1.3 Dependency Denetimi

**Dependency'leri Çıkar**:

```bash
# threshold-signing'den
cd threshold-signing/node
cargo tree | grep -E "(cggmp24|round-based|sha2)" > deps-threshold.txt

# torcus-wallet'tan
cd torcus-wallet/crates/protocols
cargo tree | grep -E "(givre|frost|cggmp24)" > deps-torcus.txt

# p2p-comm'den
cd p2p-comm
cargo tree | grep -E "(libp2p|etcd|postgres)" > deps-mtls.txt
```

**Birleşik Cargo.toml**:

```toml
[workspace]
members = [
    "crates/types",
    "crates/crypto",
    "crates/network",
    "crates/storage",
    "crates/consensus",
    "crates/protocols",
    "crates/chains",
    "crates/node",
    "crates/submitter",
    "crates/coordinator",
    "crates/cli",
]
resolver = "2"

[workspace.dependencies]
# Async runtime
tokio = { version = "1.35", features = ["full"] }
tokio-stream = "0.1"

# Networking
libp2p = { version = "0.53", features = [
    "noise", "tcp", "gossipsub", "kad", "identify", "ping",
    "yamux", "dns", "tokio", "request-response"
]}

# Storage
etcd-client = "0.13"
tokio-postgres = { version = "0.7", features = ["with-serde_json-1", "with-chrono-0_4"] }
deadpool-postgres = "0.12"
rusqlite = { version = "0.30", features = ["bundled"] }

# Cryptography
cggmp24 = { version = "0.7.0-alpha.3", features = ["all-groups"] }
givre = "0.2"  # FROST
ed25519-dalek = { version = "2.1", features = ["rand_core"] }
k256 = { version = "0.13", features = ["ecdsa", "schnorr"] }
sha2 = "0.10"
rand = "0.8"

# Bitcoin
bitcoin = { version = "0.31", features = ["serde", "rand"] }
bip32 = "0.5"
esplora-client = "0.7"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
bincode = "1.3"

# Utilities
anyhow = "1.0"
thiserror = "1.0"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
clap = { version = "4.4", features = ["derive"] }
chrono = "0.4"
uuid = { version = "1.6", features = ["v4", "serde"] }
```

---

## 🚀 Faz 2: Transport Layer Entegrasyonu (Günler 3-6)

### 2.1 libp2p Transport Adapter Oluşturma

**Hedef**: HTTP polling'i libp2p Request-Response ile değiştirme

**Dosya**: `crates/protocols/src/transport/libp2p_transport.rs`

```rust
use cggmp24::round_based::{Delivery, IncomingMessages, Outgoing};
use futures::{Sink, Stream};
use libp2p::request_response::{self, ProtocolSupport};
use tokio::sync::mpsc;
use std::pin::Pin;
use std::task::{Context, Poll};

/// CGGMP24 protokol mesajları için libp2p transport adapter
pub struct LibP2PTransport {
    incoming: mpsc::Receiver<ProtocolMessage>,
    outgoing: mpsc::Sender<ProtocolMessage>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProtocolMessage {
    pub request_id: String,
    pub from_node: u16,
    pub to_node: Option<u16>,  // None = broadcast
    pub round: u16,
    pub payload: Vec<u8>,
}

impl LibP2PTransport {
    pub async fn new(
        node_id: u16,
        network: Arc<Mutex<NetworkBehavior>>,
        request_id: String,
    ) -> Result<(IncomingStream, OutgoingSink)> {
        let (tx_incoming, rx_incoming) = mpsc::channel(100);
        let (tx_outgoing, mut rx_outgoing) = mpsc::channel(100);

        // Alıcı task'i başlat
        let network_clone = network.clone();
        let request_id_clone = request_id.clone();
        tokio::spawn(async move {
            loop {
                // Gelen mesajlar için libp2p'yi poll et
                let mut net = network_clone.lock().await;
                if let Some(msg) = net.poll_protocol_message(&request_id_clone) {
                    if tx_incoming.send(msg).await.is_err() {
                        break;  // Kanal kapandı
                    }
                }
                drop(net);
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        });

        // Gönderici task'i başlat
        let network_clone = network.clone();
        tokio::spawn(async move {
            while let Some(msg) = rx_outgoing.recv().await {
                let mut net = network_clone.lock().await;
                if let Err(e) = net.send_protocol_message(msg).await {
                    error!("Mesaj gönderilemedi: {}", e);
                }
            }
        });

        Ok((
            IncomingStream { rx: rx_incoming },
            OutgoingSink { tx: tx_outgoing },
        ))
    }
}

/// Gelen mesaj stream'i (Stream trait implementasyonu)
pub struct IncomingStream {
    rx: mpsc::Receiver<ProtocolMessage>,
}

impl Stream for IncomingStream {
    type Item = Result<ProtocolMessage>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.rx.poll_recv(cx) {
            Poll::Ready(Some(msg)) => Poll::Ready(Some(Ok(msg))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Giden mesaj sink'i (Sink trait implementasyonu)
pub struct OutgoingSink {
    tx: mpsc::Sender<ProtocolMessage>,
}

impl Sink<ProtocolMessage> for OutgoingSink {
    type Error = anyhow::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        self.tx.poll_ready(cx).map_err(|e| anyhow!("Kanal hatası: {}", e))
    }

    fn start_send(mut self: Pin<&mut Self>, item: ProtocolMessage) -> Result<()> {
        self.tx.try_send(item).map_err(|e| anyhow!("Gönderim hatası: {}", e))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }
}
```

**Temel Tasarım Kararları**:
1. **Mevcut pattern'i takip eder** - `threshold-signing/node/src/http_transport.rs`
2. **Stream/Sink trait'leri** - `round_based` crate ile uyumlu
3. **Background task'ler** - non-blocking mesaj yönetimi
4. **Kanal-tabanlı** - libp2p'yi protokol mantığından ayırır

### 2.2 Network Behavior Genişletmesi

**Dosya**: `crates/network/src/behavior.rs` (p2p-comm'den genişlet)

```rust
use libp2p::request_response::{self, ProtocolSupport, RequestId};
use std::collections::HashMap;

#[derive(NetworkBehaviour)]
pub struct NetworkBehavior {
    gossipsub: gossipsub::Behaviour,
    kademlia: kad::Behaviour<MemoryStore>,
    request_response: request_response::Behaviour<ProtocolCodec>,
    identify: identify::Behaviour,
    ping: ping::Behaviour,

    // YENİ: Protokol mesaj yönlendirme
    protocol_messages: HashMap<String, VecDeque<ProtocolMessage>>,
}

impl NetworkBehavior {
    /// Protokol mesajı gönder (broadcast veya P2P)
    pub async fn send_protocol_message(&mut self, msg: ProtocolMessage) -> Result<()> {
        match msg.to_node {
            None => {
                // GossipSub ile broadcast
                let topic = IdentTopic::new(format!("protocol/{}", msg.request_id));
                let payload = bincode::serialize(&msg)?;
                self.gossipsub.publish(topic, payload)?;
            },
            Some(target_node) => {
                // Request-Response ile direkt P2P
                let peer_id = self.get_peer_id_for_node(target_node)?;
                let payload = bincode::serialize(&msg)?;
                self.request_response.send_request(&peer_id, payload);
            }
        }
        Ok(())
    }

    /// Gelen protokol mesajları için poll
    pub fn poll_protocol_message(&mut self, request_id: &str) -> Option<ProtocolMessage> {
        self.protocol_messages.get_mut(request_id)?.pop_front()
    }

    /// Gelen GossipSub mesajlarını işle
    fn handle_gossipsub_message(&mut self, msg: gossipsub::Message) {
        if let Ok(protocol_msg) = bincode::deserialize::<ProtocolMessage>(&msg.data) {
            self.protocol_messages
                .entry(protocol_msg.request_id.clone())
                .or_insert_with(VecDeque::new)
                .push_back(protocol_msg);
        }
    }

    /// Gelen Request-Response mesajlarını işle
    fn handle_request_response(&mut self, msg: request_response::Message) {
        if let request_response::Message::Request { request, .. } = msg {
            if let Ok(protocol_msg) = bincode::deserialize::<ProtocolMessage>(&request) {
                self.protocol_messages
                    .entry(protocol_msg.request_id.clone())
                    .or_insert_with(VecDeque::new)
                    .push_back(protocol_msg);
            }
        }
    }
}
```

### 2.3 Transport Layer Testleri

**Dosya**: `crates/protocols/tests/test_libp2p_transport.rs`

```rust
#[tokio::test]
async fn test_libp2p_transport_roundtrip() {
    // 3 node kur
    let (node1, net1) = setup_node(0).await;
    let (node2, net2) = setup_node(1).await;
    let (node3, net3) = setup_node(2).await;

    // Node'ları bağla
    net1.dial(net2.peer_id()).await.unwrap();
    net1.dial(net3.peer_id()).await.unwrap();

    // Transport'ları oluştur
    let request_id = "test_dkg_001";
    let (in1, out1) = LibP2PTransport::new(0, net1, request_id).await.unwrap();
    let (in2, out2) = LibP2PTransport::new(1, net2, request_id).await.unwrap();
    let (in3, out3) = LibP2PTransport::new(2, net3, request_id).await.unwrap();

    // Broadcast test et
    let msg = ProtocolMessage {
        request_id: request_id.to_string(),
        from_node: 0,
        to_node: None,  // Broadcast
        round: 1,
        payload: b"test message".to_vec(),
    };
    out1.send(msg.clone()).await.unwrap();

    // Tüm node'lar almalı
    let received2 = in2.next().await.unwrap().unwrap();
    let received3 = in3.next().await.unwrap().unwrap();

    assert_eq!(received2.payload, b"test message");
    assert_eq!(received3.payload, b"test message");
}

#[tokio::test]
async fn test_cggmp24_over_libp2p() {
    // libp2p üzerinden tam CGGMP24 DKG
    let eid = ExecutionId::new(b"test_dkg");

    // 3-of-4 threshold
    let n = 4;
    let t = 3;

    // libp2p ile 4 node kur
    let mut handles = vec![];
    for i in 0..n {
        let handle = tokio::spawn(async move {
            let (network, _) = setup_node(i).await;
            let (incoming, outgoing) = LibP2PTransport::new(i, network, "dkg_test").await.unwrap();

            // DKG çalıştır
            let key_share = cggmp24::keygen::<Secp256k1>(eid, i, n)
                .set_threshold(Some(t))
                .start(&mut OsRng, (incoming, outgoing))
                .await
                .unwrap();

            key_share
        });
        handles.push(handle);
    }

    // Tümünün tamamlanmasını bekle
    let key_shares: Vec<_> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();

    // Hepsinin aynı public key'e sahip olduğunu doğrula
    let pubkey = key_shares[0].shared_public_key();
    for share in &key_shares[1..] {
        assert_eq!(share.shared_public_key(), pubkey);
    }
}
```

---

## 🔐 Faz 3: Protokol Entegrasyonu (Günler 7-11)

### 3.1 CGGMP24 Bileşenlerini Çıkarma

**Kaynak**: `threshold-signing/node/src/`

**Hedef**: `crates/protocols/src/cggmp24/`

```bash
# Dosyaları kopyala
cp threshold-signing/node/src/keygen.rs crates/protocols/src/cggmp24/
cp threshold-signing/node/src/aux_info.rs crates/protocols/src/cggmp24/
cp threshold-signing/node/src/signing.rs crates/protocols/src/cggmp24/
cp threshold-signing/node/src/signing_fast.rs crates/protocols/src/cggmp24/
cp threshold-signing/node/src/presignature.rs crates/protocols/src/cggmp24/
cp threshold-signing/node/src/presignature_pool.rs crates/protocols/src/cggmp24/
```

**Import'ları Değiştir** (HTTP transport'u libp2p ile değiştir):

```rust
// ESKİ (threshold-signing)
use crate::http_transport::http_transport;

// YENİ (unified)
use crate::transport::libp2p_transport::LibP2PTransport;

pub async fn run_keygen(
    eid: ExecutionId<'_>,
    my_index: u16,
    n: u16,
    t: u16,
    network: Arc<Mutex<NetworkBehavior>>,  // MessageBoardClient'tan değiştirildi
) -> Result<IncompleteKeyShare<Secp256k1>> {
    info!("{} parti ile keygen başlatılıyor, threshold {}", n, t);

    // HTTP transport yerine libp2p transport kullan
    let (incoming, outgoing) = LibP2PTransport::new(my_index, network, eid.as_str()).await?;

    let key_share = cggmp24::keygen::<Secp256k1>(eid, my_index, n)
        .set_threshold(Some(t))
        .start(&mut OsRng, (incoming, outgoing))
        .await?;

    info!("Keygen tamamlandı. Public key: {:?}", key_share.shared_public_key);

    Ok(key_share)
}
```

**Dosya**: `crates/protocols/src/cggmp24/mod.rs`

```rust
mod keygen;
mod aux_info;
mod signing;
mod signing_fast;
mod presignature;
mod presignature_pool;

pub use keygen::run_keygen;
pub use aux_info::run_aux_info_gen;
pub use signing::sign_message;
pub use signing_fast::sign_message_fast;
pub use presignature::{generate_presignature, StoredPresignature};
pub use presignature_pool::PresignaturePool;
```

### 3.2 FROST Bileşenlerini Çıkarma

**Kaynak**: `torcus-wallet/crates/protocols/src/frost/`

**Hedef**: `crates/protocols/src/frost/`

```bash
# Tüm FROST implementasyonunu kopyala
cp -r torcus-wallet/crates/protocols/src/frost/ crates/protocols/src/
```

**Temel Dosyalar**:
- `frost/mod.rs` - Ana FROST API
- `frost/keygen.rs` - FROST DKG (anlık, aux_info gerekmez!)
- `frost/signing.rs` - Schnorr imzalama (~2ms)

**libp2p'ye Adapte Et**:

```rust
// crates/protocols/src/frost/keygen.rs

use givre::ciphersuite::Secp256k1;
use givre::KeygenParty;

pub async fn run_frost_keygen(
    my_index: u16,
    participants: Vec<u16>,
    threshold: u16,
    network: Arc<Mutex<NetworkBehavior>>,
) -> Result<KeyShare> {
    let eid = ExecutionId::new(b"frost_keygen");

    // libp2p transport
    let (incoming, outgoing) = LibP2PTransport::new(my_index, network, eid.as_str()).await?;

    // FROST DKG (anlık!)
    let key_share = KeygenParty::<Secp256k1>::new(
        my_index,
        participants,
        threshold,
    )?
    .run((incoming, outgoing))
    .await?;

    info!("FROST keygen ~4ms içinde tamamlandı");

    Ok(key_share)
}

pub async fn sign_frost(
    message: &[u8],
    my_index: u16,
    participants: Vec<u16>,
    key_share: &KeyShare,
    network: Arc<Mutex<NetworkBehavior>>,
) -> Result<Signature> {
    let eid = ExecutionId::new(message);
    let (incoming, outgoing) = LibP2PTransport::new(my_index, network, eid.as_str()).await?;

    // FROST imzalama (~2ms)
    let signature = givre::signing::sign(
        message,
        my_index,
        &participants,
        key_share,
        (incoming, outgoing),
    ).await?;

    info!("FROST imza ~2ms içinde oluşturuldu");

    Ok(signature)
}
```

### 3.3 Protokol Seçim Mantığı

**Dosya**: `crates/protocols/src/mod.rs`

```rust
pub mod cggmp24;
pub mod frost;
pub mod transport;

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum Protocol {
    CGGMP24,  // ECDSA, daha yavaş (~1-2s), Bitcoin P2WPKH (SegWit)
    FROST,    // Schnorr, daha hızlı (~2ms), Bitcoin P2TR (Taproot)
}

impl Protocol {
    pub fn for_address_type(addr_type: &AddressType) -> Self {
        match addr_type {
            AddressType::P2WPKH => Protocol::CGGMP24,  // SegWit için ECDSA gerekli
            AddressType::P2TR => Protocol::FROST,       // Taproot için Schnorr gerekli
        }
    }

    pub async fn sign(
        &self,
        message: &[u8],
        my_index: u16,
        participants: Vec<u16>,
        key_share: &KeyShareEnum,
        network: Arc<Mutex<NetworkBehavior>>,
    ) -> Result<SignatureEnum> {
        match self {
            Protocol::CGGMP24 => {
                let sig = cggmp24::sign_message_fast(
                    message,
                    my_index,
                    participants,
                    key_share.as_cggmp24()?,
                    network,
                ).await?;
                Ok(SignatureEnum::ECDSA(sig))
            },
            Protocol::FROST => {
                let sig = frost::sign_frost(
                    message,
                    my_index,
                    participants,
                    key_share.as_frost()?,
                    network,
                ).await?;
                Ok(SignatureEnum::Schnorr(sig))
            }
        }
    }
}
```

---

## 🔗 Faz 4: Node Implementasyonu (Günler 12-16)

### 4.1 Birleşik Node Yapısı

**Dosya**: `crates/node/src/main.rs`

```rust
use unified_mpc_wallet::{
    network::NetworkBehavior,
    protocols::{Protocol, cggmp24, frost},
    storage::{EtcdStorage, PostgresStorage, SqliteKeyStore},
    consensus::VoteProcessor,
};

#[derive(Debug, clap::Parser)]
struct Config {
    #[clap(long, env = "NODE_ID")]
    node_id: u16,

    #[clap(long, env = "LISTEN_ADDR", default_value = "/ip4/0.0.0.0/tcp/9000")]
    listen_addr: Multiaddr,

    #[clap(long, env = "BOOTSTRAP_PEERS")]
    bootstrap_peers: Vec<Multiaddr>,

    #[clap(long, env = "ETCD_ENDPOINTS")]
    etcd_endpoints: Vec<String>,

    #[clap(long, env = "POSTGRES_URL")]
    postgres_url: String,

    #[clap(long, env = "KEY_STORE_PATH", default_value = "/data/keystore.db")]
    key_store_path: String,

    #[clap(long, env = "TOTAL_NODES", default_value = "4")]
    total_nodes: u16,

    #[clap(long, env = "THRESHOLD", default_value = "3")]
    threshold: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let config = Config::parse();

    info!("MPC node {} başlatılıyor (t={}, n={})",
          config.node_id, config.threshold, config.total_nodes);

    // Storage katmanlarını başlat
    let etcd = EtcdStorage::connect(&config.etcd_endpoints).await?;
    let postgres = PostgresStorage::connect(&config.postgres_url).await?;
    let key_store = SqliteKeyStore::open(&config.key_store_path).await?;

    // libp2p network'ü başlat
    let (mut network, mut swarm) = NetworkBehavior::new(
        config.node_id,
        &config.listen_addr,
    ).await?;

    // Bootstrap peer'lara bağlan
    for peer in &config.bootstrap_peers {
        network.dial(peer.clone()).await?;
    }

    // Consensus voting'i başlat
    let vote_processor = VoteProcessor::new(etcd.clone(), postgres.clone());

    // Mevcut key share'leri kontrol et
    let (cggmp24_share, frost_share) = match key_store.load_key_shares().await? {
        Some(shares) => {
            info!("Mevcut key share'ler yüklendi");
            shares
        },
        None => {
            info!("Mevcut key yok, DKG çalıştırılıyor...");

            // CGGMP24 DKG çalıştır
            let cggmp24_share = cggmp24::run_keygen(
                ExecutionId::new(b"cggmp24_dkg_initial"),
                config.node_id,
                config.total_nodes,
                config.threshold,
                Arc::new(Mutex::new(network.clone())),
            ).await?;

            // FROST DKG çalıştır (hızlı!)
            let frost_share = frost::run_frost_keygen(
                config.node_id,
                (0..config.total_nodes).collect(),
                config.threshold,
                Arc::new(Mutex::new(network.clone())),
            ).await?;

            // SQLite'a kaydet
            key_store.save_key_shares(&cggmp24_share, &frost_share).await?;

            (cggmp24_share, frost_share)
        }
    };

    // Presignature pool'u başlat (sadece CGGMP24)
    let presig_pool = Arc::new(PresignaturePool::new(target: 20, max: 30));

    // Background task: Presignature pool'u sürdür
    if config.node_id == 0 {
        let pool = presig_pool.clone();
        let net = Arc::new(Mutex::new(network.clone()));
        tokio::spawn(async move {
            maintain_presignature_pool(pool, net, cggmp24_share.clone()).await;
        });
    }

    // Ana event loop
    info!("Node hazır, ana döngüye giriliyor");
    loop {
        tokio::select! {
            // Network event'lerini yönet
            event = swarm.select_next_some() => {
                handle_network_event(event, &mut network).await?;
            },

            // İmzalama isteklerini poll et
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                handle_signing_requests(
                    &etcd,
                    &vote_processor,
                    &network,
                    &cggmp24_share,
                    &frost_share,
                    &presig_pool,
                ).await?;
            }
        }
    }
}
```

### 4.2 İmzalama İstek Yöneticisi

**Dosya**: `crates/node/src/signing_handler.rs`

```rust
async fn handle_signing_requests(
    etcd: &EtcdStorage,
    vote_processor: &VoteProcessor,
    network: &NetworkBehavior,
    cggmp24_share: &KeyShare<Secp256k1>,
    frost_share: &KeyShare,
    presig_pool: &Arc<PresignaturePool>,
) -> Result<()> {
    // etcd'den bekleyen imzalama isteklerini al
    let requests = etcd.get_pending_signing_requests().await?;

    for req in requests {
        if !is_participant(&req.id, my_index, threshold) {
            continue;  // Bizim işimiz değil
        }

        info!("İmzalama isteği işleniyor: {}", req.id);

        // Protokolü belirle
        let protocol = Protocol::for_address_type(&req.address_type);

        // İmza oluştur
        let signature = match protocol {
            Protocol::CGGMP24 => {
                // Presignature ile hızlı yolu dene
                if let Some(presig) = presig_pool.take() {
                    cggmp24::sign_message_fast(
                        &req.message,
                        my_index,
                        &req.participants,
                        cggmp24_share,
                        presig,
                        network,
                    ).await?
                } else {
                    // Tam protokole geri dön
                    warn!("Presignature yok, tam protokol kullanılıyor");
                    cggmp24::sign_message(
                        &req.message,
                        my_index,
                        &req.participants,
                        cggmp24_share,
                        network,
                    ).await?
                }
            },
            Protocol::FROST => {
                // FROST her zaman hızlıdır (~2ms)
                frost::sign_frost(
                    &req.message,
                    my_index,
                    &req.participants,
                    frost_share,
                    network,
                ).await?
            }
        };

        info!("İstek için imza oluşturuldu: {}", req.id);

        // Kısmi imzayı etcd'ye gönder
        etcd.submit_partial_signature(&req.id, my_index, &signature).await?;

        // Tüm kısmi imzaları bekle
        let all_sigs = wait_for_all_partials(etcd, &req.id, &req.participants).await?;

        // İlk toplayan node birleştirir ve oylar
        if my_index == req.participants[0] {
            let combined_sig = combine_signatures(all_sigs, &protocol)?;

            // İmzayı doğrula
            let pubkey = match protocol {
                Protocol::CGGMP24 => cggmp24_share.shared_public_key(),
                Protocol::FROST => frost_share.group_public_key(),
            };
            verify_signature(&combined_sig, &req.message, &pubkey)?;

            info!("İmza başarıyla doğrulandı");

            // Consensus katmanına vote gönder
            vote_processor.cast_vote(
                &req.id,
                VoteValue::Approve {
                    signature: combined_sig,
                    message: req.message.clone(),
                },
                my_keypair,
            ).await?;
        }
    }

    Ok(())
}
```

---

## 📤 Faz 5: Submitter Node (Günler 17-19)

### 5.1 Submitter Node Binary

**Dosya**: `crates/submitter/src/main.rs`

```rust
use unified_mpc_wallet::{
    chains::bitcoin::{BitcoinClient, TransactionBuilder},
    storage::{EtcdStorage, PostgresStorage},
    consensus::VoteProcessor,
};

#[derive(Debug, clap::Parser)]
struct Config {
    #[clap(long, env = "ETCD_ENDPOINTS")]
    etcd_endpoints: Vec<String>,

    #[clap(long, env = "POSTGRES_URL")]
    postgres_url: String,

    #[clap(long, env = "BITCOIN_RPC_URL")]
    bitcoin_rpc_url: String,

    #[clap(long, env = "BITCOIN_NETWORK", default_value = "regtest")]
    bitcoin_network: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let config = Config::parse();

    info!("Submitter Node başlatılıyor");

    // Storage'ı başlat
    let etcd = EtcdStorage::connect(&config.etcd_endpoints).await?;
    let postgres = PostgresStorage::connect(&config.postgres_url).await?;

    // Bitcoin client'ı başlat
    let bitcoin = BitcoinClient::new(&config.bitcoin_rpc_url, &config.bitcoin_network).await?;

    // Consensus'u başlat
    let vote_processor = VoteProcessor::new(etcd.clone(), postgres.clone());

    // Ana event loop
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Consensus'a ulaşan transaction'ları poll et
        let approved_txs = etcd.get_approved_transactions().await?;

        for tx in approved_txs {
            info!("Onaylanmış transaction işleniyor: {}", tx.id);

            // Distributed lock edin (exactly-once garantisi)
            let lock_key = format!("/locks/submission/{}", tx.id);
            if !etcd.acquire_lock(&lock_key, 300).await? {
                info!("Lock başka bir submitter tarafından tutulmuş, atlanıyor");
                continue;
            }

            // Bitcoin transaction oluştur
            let bitcoin_tx = TransactionBuilder::new()
                .add_input(tx.input_utxo)
                .add_output(tx.recipient_address, tx.amount)
                .add_change_output(tx.change_address, tx.change_amount)
                .finalize_with_signature(&tx.signature)?;

            // Bitcoin network'üne broadcast et
            match bitcoin.broadcast_transaction(&bitcoin_tx).await {
                Ok(txid) => {
                    info!("Transaction başarıyla broadcast edildi: {}", txid);

                    // etcd'de durumu güncelle
                    etcd.update_transaction_status(
                        &tx.id,
                        TransactionStatus::Confirmed { txid },
                    ).await?;

                    // PostgreSQL audit trail'e logla
                    postgres.log_blockchain_submission(
                        &tx.id,
                        &txid,
                        &tx.signature,
                        &tx.participants,
                    ).await?;
                },
                Err(e) => {
                    error!("Transaction broadcast başarısız: {}", e);

                    // Durumu başarısız olarak güncelle
                    etcd.update_transaction_status(
                        &tx.id,
                        TransactionStatus::Failed { reason: e.to_string() },
                    ).await?;

                    // Hatayı PostgreSQL'e logla
                    postgres.log_submission_error(&tx.id, &e.to_string()).await?;
                }
            }

            // Lock'u serbest bırak
            etcd.release_lock(&lock_key).await?;
        }
    }
}
```

### 5.2 Bitcoin Entegrasyonu

**Kaynak**: `torcus-wallet/crates/chains/src/bitcoin/`

**Hedef**: `crates/chains/src/bitcoin/`

```bash
# Tüm Bitcoin stack'ini kopyala
cp -r torcus-wallet/crates/chains/src/bitcoin/ crates/chains/src/
```

**Temel Dosyalar**:
- `address.rs` - P2WPKH, P2TR türetimi
- `hd.rs` - BIP32/BIP84/BIP86 key türetimi
- `client.rs` - Esplora API, Bitcoin RPC
- `transaction.rs` - TX oluşturma, imzalama, broadcast

**Dosya**: `crates/chains/src/bitcoin/transaction.rs`

```rust
use bitcoin::{Transaction, TxIn, TxOut, Script, Witness};
use bitcoin::secp256k1::{Secp256k1, Message, ecdsa::Signature};

pub struct TransactionBuilder {
    inputs: Vec<TxIn>,
    outputs: Vec<TxOut>,
    version: i32,
    lock_time: u32,
}

impl TransactionBuilder {
    pub fn new() -> Self {
        Self {
            inputs: vec![],
            outputs: vec![],
            version: 2,
            lock_time: 0,
        }
    }

    pub fn add_input(&mut self, utxo: UTXO) -> &mut Self {
        self.inputs.push(TxIn {
            previous_output: utxo.outpoint,
            script_sig: Script::new(),
            sequence: 0xffffffff,
            witness: Witness::default(),
        });
        self
    }

    pub fn add_output(&mut self, address: Address, amount: u64) -> &mut Self {
        self.outputs.push(TxOut {
            value: amount,
            script_pubkey: address.script_pubkey(),
        });
        self
    }

    pub fn finalize_with_signature(
        &self,
        signature: &SignatureEnum,
    ) -> Result<Transaction> {
        let mut tx = Transaction {
            version: self.version,
            lock_time: self.lock_time,
            input: self.inputs.clone(),
            output: self.outputs.clone(),
        };

        // Witness'a imza ekle (SegWit/Taproot)
        match signature {
            SignatureEnum::ECDSA(sig) => {
                // P2WPKH (SegWit) witness
                tx.input[0].witness.push(sig.to_der());
                tx.input[0].witness.push(pubkey.serialize());
            },
            SignatureEnum::Schnorr(sig) => {
                // P2TR (Taproot) witness
                tx.input[0].witness.push(sig.serialize());
            }
        }

        Ok(tx)
    }
}
```

---

## 🧪 Faz 6: Test & Doğrulama (Günler 20-25)

### 6.1 Entegrasyon Testleri

**Dosya**: `tests/integration_test.rs`

```rust
#[tokio::test]
async fn test_full_signing_flow_cggmp24() {
    // 4 node + submitter kur
    let (nodes, submitter, etcd, postgres) = setup_test_cluster(4).await;

    // DKG çalıştır
    let key_shares = run_distributed_keygen(&nodes, Protocol::CGGMP24).await.unwrap();

    // İmzalama isteği oluştur
    let message = b"Test transaction";
    let request_id = "test_req_001";

    etcd.create_signing_request(request_id, message, AddressType::P2WPKH).await.unwrap();

    // Node'lar otomatik işlemeli
    tokio::time::sleep(Duration::from_secs(3)).await;

    // İmza oluşturulduğunu kontrol et
    let tx_status = etcd.get_transaction_status(request_id).await.unwrap();
    assert_eq!(tx_status, TransactionStatus::Approved);

    // Submitter broadcast etmeli
    tokio::time::sleep(Duration::from_secs(2)).await;

    // PostgreSQL audit log'u kontrol et
    let submission = postgres.get_submission(request_id).await.unwrap();
    assert!(submission.txid.is_some());
}

#[tokio::test]
async fn test_byzantine_detection() {
    let (nodes, submitter, etcd, postgres) = setup_test_cluster(4).await;

    // Byzantine davranış enjekte et (node-3 farklı oylar)
    nodes[3].set_byzantine_mode(ByzantineMode::DoubleVote);

    // İmzalama isteği oluştur
    let request_id = "byzantine_test";
    etcd.create_signing_request(request_id, b"test", AddressType::P2WPKH).await.unwrap();

    // İşlemeyi bekle
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Byzantine tespitini kontrol et
    let violations = postgres.get_byzantine_violations(request_id).await.unwrap();
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].node_id, 3);
    assert_eq!(violations[0].violation_type, "DOUBLE_VOTE");

    // Transaction iptal edilmiş olmalı
    let tx_status = etcd.get_transaction_status(request_id).await.unwrap();
    assert_eq!(tx_status, TransactionStatus::Aborted);
}

#[tokio::test]
async fn test_frost_vs_cggmp24_performance() {
    let (nodes, _, _, _) = setup_test_cluster(4).await;

    // CGGMP24 imzalama
    let start = Instant::now();
    let cggmp24_sig = sign_message(b"test", Protocol::CGGMP24, &nodes).await.unwrap();
    let cggmp24_time = start.elapsed();

    // FROST imzalama
    let start = Instant::now();
    let frost_sig = sign_message(b"test", Protocol::FROST, &nodes).await.unwrap();
    let frost_time = start.elapsed();

    info!("CGGMP24: {:?}", cggmp24_time);  // ~1-2s
    info!("FROST: {:?}", frost_time);      // ~2-3ms

    // FROST 100-500x daha hızlı olmalı
    assert!(frost_time < cggmp24_time / 100);
}
```

### 6.2 Uçtan Uca Test

**Dosya**: `scripts/test-e2e.sh`

```bash
#!/bin/bash
set -e

echo "========================================="
echo "Uçtan Uca Entegrasyon Testi"
echo "========================================="

# 1. Altyapıyı başlat
echo "1. Altyapı başlatılıyor..."
docker-compose up -d etcd-1 etcd-2 etcd-3 postgres bitcoind
sleep 5

# 2. Node'ları başlat
echo "2. MPC node'ları başlatılıyor..."
docker-compose up -d node-1 node-2 node-3 node-4
sleep 10

# 3. DKG tamamlanmasını bekle
echo "3. DKG bekleniyor..."
for i in {1..60}; do
    if docker logs node-1 2>&1 | grep -q "DKG completed"; then
        echo "✓ DKG tamamlandı"
        break
    fi
    sleep 1
done

# 4. Submitter'ı başlat
echo "4. Submitter node başlatılıyor..."
docker-compose up -d submitter
sleep 2

# 5. Cüzdan oluştur ve fonla
echo "5. Taproot cüzdan oluşturuluyor (FROST)..."
WALLET_ID=$(cargo run --bin mpc-wallet -- taproot-create --name "E2E Test" | grep "wallet_id" | cut -d'"' -f4)
echo "Wallet ID: $WALLET_ID"

echo "6. Cüzdanı fonlamak için blok üretiliyor..."
cargo run --bin mpc-wallet -- mine --wallet-id $WALLET_ID --blocks 101
sleep 5

# 7. Bakiyeyi kontrol et
echo "7. Bakiye kontrol ediliyor..."
BALANCE=$(cargo run --bin mpc-wallet -- balance --wallet-id $WALLET_ID | grep "Balance" | awk '{print $2}')
echo "Bakiye: $BALANCE sats"
if [ "$BALANCE" -lt 5000000000 ]; then
    echo "✗ Yetersiz bakiye"
    exit 1
fi

# 8. Transaction gönder (FROST)
echo "8. Bitcoin gönderiliyor (FROST protokolü)..."
TO_ADDR=$(cargo run --bin mpc-wallet -- derive-addresses --wallet-id $WALLET_ID --count 1 | grep "address" | cut -d'"' -f4)
START=$(date +%s%N)
TXID=$(cargo run --bin mpc-wallet -- taproot-send --wallet-id $WALLET_ID --to $TO_ADDR --amount 100000000 | grep "txid" | cut -d'"' -f4)
END=$(date +%s%N)
FROST_TIME=$(( ($END - $START) / 1000000 ))
echo "✓ FROST transaction: $TXID (${FROST_TIME}ms)"

# 9. SegWit cüzdan oluştur (CGGMP24)
echo "9. SegWit cüzdan oluşturuluyor (CGGMP24)..."
WALLET_ID_2=$(cargo run --bin mpc-wallet -- cggmp24-create --name "E2E Test 2" | grep "wallet_id" | cut -d'"' -f4)
cargo run --bin mpc-wallet -- mine --wallet-id $WALLET_ID_2 --blocks 101
sleep 5

# 10. Transaction gönder (CGGMP24)
echo "10. Bitcoin gönderiliyor (CGGMP24 protokolü)..."
TO_ADDR_2=$(cargo run --bin mpc-wallet -- derive-addresses --wallet-id $WALLET_ID_2 --count 1 | grep "address" | cut -d'"' -f4)
START=$(date +%s%N)
TXID_2=$(cargo run --bin mpc-wallet -- cggmp24-send --wallet-id $WALLET_ID_2 --to $TO_ADDR_2 --amount 100000000 | grep "txid" | cut -d'"' -f4)
END=$(date +%s%N)
CGGMP24_TIME=$(( ($END - $START) / 1000000 ))
echo "✓ CGGMP24 transaction: $TXID_2 (${CGGMP24_TIME}ms)"

# 11. Byzantine tespitini doğrula
echo "11. Byzantine tespiti test ediliyor..."
docker exec node-3 pkill -USR1 mpc-node  # Byzantine davranışı tetikle
sleep 2
cargo run --bin mpc-wallet -- taproot-send --wallet-id $WALLET_ID --to $TO_ADDR --amount 50000000 || true
VIOLATIONS=$(docker exec postgres psql -U mpc -d mpc_wallet -tAc "SELECT COUNT(*) FROM byzantine_violations")
if [ "$VIOLATIONS" -gt 0 ]; then
    echo "✓ Byzantine tespiti çalışıyor ($VIOLATIONS ihlal loglandı)"
else
    echo "✗ Byzantine tespiti başarısız"
    exit 1
fi

# 12. Özet
echo "========================================="
echo "✅ Tüm testler geçti!"
echo "========================================="
echo "FROST imzalama:   ${FROST_TIME}ms"
echo "CGGMP24 imzalama: ${CGGMP24_TIME}ms"
echo "Hızlanma:         $(( CGGMP24_TIME / FROST_TIME ))x"
echo "Byzantine ihlalleri tespit edildi: $VIOLATIONS"
```

---

## 📊 Faz 7: Performans Benchmark'ları

### 7.1 Benchmark Suite

**Dosya**: `benches/signing_benchmark.rs`

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};

fn benchmark_cggmp24_cold(c: &mut Criterion) {
    let mut group = c.benchmark_group("cggmp24_cold");

    for threshold in [2, 3, 4] {
        group.bench_with_input(
            BenchmarkId::new("full_protocol", threshold),
            &threshold,
            |b, &t| {
                b.iter(|| {
                    // Tam CGGMP24 imzalama (presignature yok)
                    let sig = sign_cggmp24_full(black_box(b"test message"), t);
                    black_box(sig)
                });
            },
        );
    }
    group.finish();
}

fn benchmark_cggmp24_warm(c: &mut Criterion) {
    let mut group = c.benchmark_group("cggmp24_warm");

    // Presignature'ları önceden oluştur
    let presig_pool = generate_presignature_pool(20);

    for threshold in [2, 3, 4] {
        group.bench_with_input(
            BenchmarkId::new("fast_path", threshold),
            &threshold,
            |b, &t| {
                b.iter(|| {
                    // Hızlı CGGMP24 imzalama (presignature ile)
                    let presig = presig_pool.take().unwrap();
                    let sig = sign_cggmp24_fast(black_box(b"test message"), t, presig);
                    black_box(sig)
                });
            },
        );
    }
    group.finish();
}

fn benchmark_frost(c: &mut Criterion) {
    let mut group = c.benchmark_group("frost");

    for threshold in [2, 3, 4] {
        group.bench_with_input(
            BenchmarkId::new("signing", threshold),
            &threshold,
            |b, &t| {
                b.iter(|| {
                    // FROST imzalama (her zaman hızlı)
                    let sig = sign_frost(black_box(b"test message"), t);
                    black_box(sig)
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, benchmark_cggmp24_cold, benchmark_cggmp24_warm, benchmark_frost);
criterion_main!(benches);
```

**Beklenen Sonuçlar**:

```
cggmp24_cold/full_protocol/2    time:   [1.2s 1.5s 1.8s]
cggmp24_cold/full_protocol/3    time:   [2.0s 2.3s 2.6s]
cggmp24_cold/full_protocol/4    time:   [3.1s 3.5s 3.9s]

cggmp24_warm/fast_path/2        time:   [80ms 120ms 160ms]
cggmp24_warm/fast_path/3        time:   [100ms 150ms 200ms]
cggmp24_warm/fast_path/4        time:   [120ms 180ms 240ms]

frost/signing/2                 time:   [1.5ms 2.0ms 2.5ms]
frost/signing/3                 time:   [2.0ms 2.5ms 3.0ms]
frost/signing/4                 time:   [2.5ms 3.0ms 3.5ms]
```

---

## 🚀 Deployment Stratejisi

### Docker Compose Konfigürasyonu

**Dosya**: `docker-compose.yml`

```yaml
version: '3.8'

services:
  # Altyapı
  etcd-1:
    image: quay.io/coreos/etcd:v3.5.12
    environment:
      ETCD_NAME: etcd-1
      ETCD_INITIAL_CLUSTER: etcd-1=http://etcd-1:2380,etcd-2=http://etcd-2:2380,etcd-3=http://etcd-3:2380
      ETCD_INITIAL_CLUSTER_STATE: new
      ETCD_LISTEN_CLIENT_URLS: http://0.0.0.0:2379
      ETCD_ADVERTISE_CLIENT_URLS: http://etcd-1:2379
      ETCD_LISTEN_PEER_URLS: http://0.0.0.0:2380
      ETCD_INITIAL_ADVERTISE_PEER_URLS: http://etcd-1:2380
    ports:
      - "2379:2379"

  etcd-2:
    image: quay.io/coreos/etcd:v3.5.12
    environment:
      ETCD_NAME: etcd-2
      ETCD_INITIAL_CLUSTER: etcd-1=http://etcd-1:2380,etcd-2=http://etcd-2:2380,etcd-3=http://etcd-3:2380
      ETCD_INITIAL_CLUSTER_STATE: new
      ETCD_LISTEN_CLIENT_URLS: http://0.0.0.0:2379
      ETCD_ADVERTISE_CLIENT_URLS: http://etcd-2:2379
      ETCD_LISTEN_PEER_URLS: http://0.0.0.0:2380
      ETCD_INITIAL_ADVERTISE_PEER_URLS: http://etcd-2:2380

  etcd-3:
    image: quay.io/coreos/etcd:v3.5.12
    environment:
      ETCD_NAME: etcd-3
      ETCD_INITIAL_CLUSTER: etcd-1=http://etcd-1:2380,etcd-2=http://etcd-2:2380,etcd-3=http://etcd-3:2380
      ETCD_INITIAL_CLUSTER_STATE: new
      ETCD_LISTEN_CLIENT_URLS: http://0.0.0.0:2379
      ETCD_ADVERTISE_CLIENT_URLS: http://etcd-3:2379
      ETCD_LISTEN_PEER_URLS: http://0.0.0.0:2380
      ETCD_INITIAL_ADVERTISE_PEER_URLS: http://etcd-3:2380

  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_USER: mpc
      POSTGRES_PASSWORD: mpc_password
      POSTGRES_DB: mpc_wallet
    ports:
      - "5432:5432"
    volumes:
      - postgres-data:/var/lib/postgresql/data
      - ./scripts/init-db.sql:/docker-entrypoint-initdb.d/init-db.sql

  bitcoind:
    image: btcpayserver/bitcoin:26.0
    command: |
      bitcoind
      -regtest
      -server
      -rpcuser=bitcoin
      -rpcpassword=bitcoin
      -rpcallowip=0.0.0.0/0
      -rpcbind=0.0.0.0
      -zmqpubrawblock=tcp://0.0.0.0:28332
      -zmqpubrawtx=tcp://0.0.0.0:28333
      -txindex=1
    ports:
      - "18443:18443"

  # MPC Node'ları
  node-1:
    build:
      context: .
      dockerfile: docker/Dockerfile.node
    environment:
      NODE_ID: 0
      LISTEN_ADDR: /ip4/0.0.0.0/tcp/9000
      BOOTSTRAP_PEERS: /ip4/node-2:9000/p2p/...,/ip4/node-3:9000/p2p/...,/ip4/node-4:9000/p2p/...
      ETCD_ENDPOINTS: http://etcd-1:2379,http://etcd-2:2379,http://etcd-3:2379
      POSTGRES_URL: postgresql://mpc:mpc_password@postgres:5432/mpc_wallet
      KEY_STORE_PATH: /data/keystore.db
      TOTAL_NODES: 4
      THRESHOLD: 3
      RUST_LOG: info
    depends_on:
      - etcd-1
      - etcd-2
      - etcd-3
      - postgres
    volumes:
      - node-1-data:/data

  node-2:
    build:
      context: .
      dockerfile: docker/Dockerfile.node
    environment:
      NODE_ID: 1
      LISTEN_ADDR: /ip4/0.0.0.0/tcp/9000
      ETCD_ENDPOINTS: http://etcd-1:2379,http://etcd-2:2379,http://etcd-3:2379
      POSTGRES_URL: postgresql://mpc:mpc_password@postgres:5432/mpc_wallet
      KEY_STORE_PATH: /data/keystore.db
      TOTAL_NODES: 4
      THRESHOLD: 3
      RUST_LOG: info
    depends_on:
      - etcd-1
      - postgres
    volumes:
      - node-2-data:/data

  node-3:
    build:
      context: .
      dockerfile: docker/Dockerfile.node
    environment:
      NODE_ID: 2
      LISTEN_ADDR: /ip4/0.0.0.0/tcp/9000
      ETCD_ENDPOINTS: http://etcd-1:2379,http://etcd-2:2379,http://etcd-3:2379
      POSTGRES_URL: postgresql://mpc:mpc_password@postgres:5432/mpc_wallet
      KEY_STORE_PATH: /data/keystore.db
      TOTAL_NODES: 4
      THRESHOLD: 3
      RUST_LOG: info
    depends_on:
      - etcd-1
      - postgres
    volumes:
      - node-3-data:/data

  node-4:
    build:
      context: .
      dockerfile: docker/Dockerfile.node
    environment:
      NODE_ID: 3
      LISTEN_ADDR: /ip4/0.0.0.0/tcp/9000
      ETCD_ENDPOINTS: http://etcd-1:2379,http://etcd-2:2379,http://etcd-3:2379
      POSTGRES_URL: postgresql://mpc:mpc_password@postgres:5432/mpc_wallet
      KEY_STORE_PATH: /data/keystore.db
      TOTAL_NODES: 4
      THRESHOLD: 3
      RUST_LOG: info
    depends_on:
      - etcd-1
      - postgres
    volumes:
      - node-4-data:/data

  # Submitter Node
  submitter:
    build:
      context: .
      dockerfile: docker/Dockerfile.submitter
    environment:
      ETCD_ENDPOINTS: http://etcd-1:2379,http://etcd-2:2379,http://etcd-3:2379
      POSTGRES_URL: postgresql://mpc:mpc_password@postgres:5432/mpc_wallet
      BITCOIN_RPC_URL: http://bitcoin:bitcoin@bitcoind:18443
      BITCOIN_NETWORK: regtest
      RUST_LOG: info
    depends_on:
      - etcd-1
      - postgres
      - bitcoind

volumes:
  postgres-data:
  node-1-data:
  node-2-data:
  node-3-data:
  node-4-data:
```

---

## ✅ Doğrulama Kontrol Listesi

### Faz 1: Hazırlık
- [ ] Birleşik workspace oluşturuldu
- [ ] Tüm Cargo.toml dosyaları CGGMP24 `0.7.0-alpha.3` kullanıyor
- [ ] Dependency denetimi tamamlandı
- [ ] Döngüsel dependency yok

### Faz 2: Transport Layer
- [ ] libp2p transport adapter derleniyor
- [ ] Stream/Sink trait'leri doğru implement edildi
- [ ] GossipSub broadcast çalışıyor
- [ ] Request-Response P2P çalışıyor
- [ ] Test: 4-node mesaj roundtrip geçiyor

### Faz 3: Protokol Entegrasyonu
- [ ] CGGMP24 DKG libp2p üzerinde çalışıyor
- [ ] CGGMP24 signing libp2p üzerinde çalışıyor
- [ ] FROST DKG libp2p üzerinde çalışıyor
- [ ] FROST signing libp2p üzerinde çalışıyor
- [ ] Presignature pool fonksiyonel
- [ ] Test: Her iki protokol de geçerli imzalar üretiyor

### Faz 4: Node Implementasyonu
- [ ] Node binary derleniyor
- [ ] Key share yoksa başlangıçta DKG çalışıyor
- [ ] Key share'ler SQLite'a kaydediliyor
- [ ] Ana event loop imzalama isteklerini poll ediyor
- [ ] Byzantine voting entegre edildi
- [ ] Test: 4 node tam imzalama akışını tamamlıyor

### Faz 5: Submitter Node
- [ ] Submitter binary derleniyor
- [ ] Onaylanmış transaction'lar için etcd'yi poll ediyor
- [ ] Distributed lock'u doğru ediniyor
- [ ] Bitcoin transaction'ı imza ile oluşturuyor
- [ ] Bitcoin network'üne broadcast ediyor
- [ ] PostgreSQL audit trail'e loglama yapıyor
- [ ] Test: Transaction blockchain üzerinde görünüyor

### Faz 6: Testler
- [ ] Unit testler geçiyor (tüm crate'ler)
- [ ] Entegrasyon testleri geçiyor
- [ ] E2E test geçiyor
- [ ] Byzantine tespit testi geçiyor
- [ ] Performans benchmark'ları çalışıyor
- [ ] FROST'un CGGMP24'ten 100-500x daha hızlı olduğu doğrulandı

### Faz 7: Production Hazırlığı
- [ ] Docker Compose deployment çalışıyor
- [ ] Tüm container'lar başarıyla başlıyor
- [ ] etcd cluster sağlıklı (3 node)
- [ ] PostgreSQL şeması oluşturuldu
- [ ] Bitcoin RPC bağlantısı çalışıyor
- [ ] Loglar yapılandırılmış ve sorgulanabilir
- [ ] Metrikler export ediliyor (Prometheus-ready)

---

## 🎯 Başarı Kriterleri

**Olmazsa Olmaz**:
1. ✅ 4 MPC node libp2p ile iletişim kuruyor (HTTP polling yok)
2. ✅ CGGMP24 DKG + signing uçtan uca çalışıyor
3. ✅ FROST DKG + signing uçtan uca çalışıyor
4. ✅ Byzantine tespiti PostgreSQL'e loglama yapıyor
5. ✅ Submitter node transaction'ları Bitcoin'e broadcast ediyor
6. ✅ Exactly-once gönderim garantisi (distributed lock)
7. ✅ Tam audit trail (vote geçmişi, gönderimler)

**Performans Hedefleri**:
- FROST imzalama: <10ms (p95)
- CGGMP24 imzalama (fast path): <500ms (p95)
- Byzantine tespiti: <1s (p95)
- Uçtan uca TX: <3s (FROST), <5s (CGGMP24)

**Güvenlik Gereksinimleri**:
- ✅ libp2p Noise XX şifreleme
- ✅ Byzantine hata tespiti (4 tip)
- ✅ Distributed lock'lar (double-spend yok)
- ✅ Audit trail değiştirilemez (PostgreSQL)
- ✅ Key share'ler node'dan çıkmaz (SQLite local)

---

## 📚 Referanslar

**Makaleler**:
- CGGMP24: https://eprint.iacr.org/2021/060
- FROST: https://eprint.iacr.org/2020/852
- libp2p Noise: https://github.com/libp2p/specs/tree/master/noise
- Raft Consensus: https://raft.github.io/raft.pdf

**Repository'ler**:
- p2p-comm: `c:\Users\user\Desktop\MPC-WALLET\p2p-comm`
- threshold-signing: `c:\Users\user\Desktop\MPC-WALLET\threshold-signing (Copy)`
- torcus-wallet: `c:\Users\user\Desktop\MPC-WALLET\torcus-wallet`

**Crate'ler**:
- cggmp24: https://github.com/webb-tools/cggmp-threshold-ecdsa
- givre (FROST): https://github.com/ZenGo-X/givre
- libp2p-rust: https://github.com/libp2p/rust-libp2p
- etcd-client: https://docs.rs/etcd-client/latest/etcd_client/
- bitcoin: https://docs.rs/bitcoin/latest/bitcoin/

---

**Doküman Versiyonu**: 1.0.0
**Son Güncelleme**: 2026-01-16
**Yazar**: Integration Plan Generator
**Durum**: IMPLEMENTASYONA HAZIR
