# 🔐 İkili (Peer-to-Peer) İletişim Rehberi

## 📡 SİSTEMDE İLETİŞİM PROTOKOLLERİ

### 1. **mTLS (Mutual TLS) NEDİR?**

**mTLS** = Mutual TLS (Çift Yönlü TLS)

Bu sistemde **mTLS KULLANILMIYOR**. Bunun yerine daha modern ve performanslı bir protokol kullanılıyor:

---

## 🛡️ **NOISE PROTOCOL (WireGuard Seviyesinde Güvenlik)**

### **Noise Protocol Nedir?**

Bu sistemde node'lar arası iletişim **Noise Protocol Framework** ile şifreleniyor. Bu, WireGuard VPN'de kullanılan aynı kriptografik protokoldür.

**Özellikler:**
- ✅ **Karşılıklı Kimlik Doğrulama** (mTLS gibi)
- ✅ **Ed25519 Public Key Cryptography**
- ✅ **Forward Secrecy** (geçmiş mesajlar deşifre edilemez)
- ✅ **TLS'den daha hızlı** (daha az handshake overhead)
- ✅ **Modern ve güvenli** (2018'den beri endüstri standardı)

### **Neden mTLS Değil de Noise?**

| Özellik | mTLS | Noise Protocol |
|---------|------|----------------|
| Sertifika Yönetimi | Gerekli (CA, cert rotation) | Gerekmez (public key yeterli) |
| Handshake Hızı | Yavaş (3+ round-trip) | Hızlı (1-2 round-trip) |
| Kod Karmaşıklığı | Yüksek | Düşük |
| P2P Uyumluluğu | Orta | Mükemmel |
| Kullanıldığı Yerler | Web sunucuları | WireGuard, libp2p, Tor |

---

## 🏗️ **İLETİŞİM MİMARİSİ**

```
┌─────────────────────────────────────────────────────────┐
│                    İLETİŞİM KATMANLARI                   │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  1. TCP Transport Layer (0.0.0.0:9000-9004)             │
│      └─> Temel ağ bağlantısı                            │
│                                                          │
│  2. Noise Protocol Encryption Layer                     │
│      ├─> Ed25519 key exchange                           │
│      ├─> ChaCha20-Poly1305 encryption                   │
│      └─> Perfect forward secrecy                        │
│                                                          │
│  3. Yamux Multiplexing Layer                            │
│      └─> Tek TCP bağlantısı üzerinde çoklu stream      │
│                                                          │
│  4. libp2p Protocols                                    │
│      ├─> GossipSub (Broadcast)                         │
│      ├─> Request-Response (İkili iletişim)             │
│      ├─> Kademlia DHT (Peer discovery)                 │
│      ├─> Identify (Peer bilgileri)                     │
│      └─> Ping (Keepalive)                              │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

---

## 🔄 **İKİ İLETİŞİM MODu**

### **Mod 1: BROADCAST (Şu Anda Kullanılan)**

**Protokol:** GossipSub (libp2p)

```rust
// Vote tüm node'lara gönderilir
node.broadcast_vote(vote)?;
```

**Nasıl Çalışır:**
```
Node 1 ──broadcast──┬──> Node 2
                    ├──> Node 3
                    ├──> Node 4
                    └──> Node 5
```

**Kullanım Alanları:**
- ✅ Oy gönderme (vote submission)
- ✅ Consensus mesajları
- ✅ Tüm node'ların bilmesi gereken bilgiler

---

### **Mod 2: DIRECT REQUEST-RESPONSE (Yeni Eklendi)**

**Protokol:** libp2p Request-Response

```rust
// Belirli bir peer'a istek gönder
let request_id = node.send_request(peer_id, DirectRequest::GetVoteStatus {
    tx_id: "tx_001".to_string()
})?;

// Yanıt geldiğinde:
// DirectResponse::VoteStatus { tx_id, voted, value }
```

**Nasıl Çalışır:**
```
Node 1 ─────request─────> Node 3
       <────response──────
```

**Kullanım Alanları:**
- ✅ Belirli bir node'dan bilgi isteme
- ✅ Vote durumu sorgulama
- ✅ Node reputation kontrolü
- ✅ Custom mesajlar

---

## 📝 **KULLANIM ÖRNEKLERİ**

### **Örnek 1: Vote Status Sorgulama**

```rust
use threshold_network::{DirectRequest, DirectResponse};

// Node 2'nin vote durumunu öğren
let peer_id = PeerId::from_str("12D3KooW...")?;
let request = DirectRequest::GetVoteStatus {
    tx_id: "tx_001".to_string()
};

let request_id = node.send_request(peer_id, request)?;

// Yanıt geldiğinde:
// DirectResponse::VoteStatus {
//     tx_id: "tx_001",
//     voted: true,
//     value: Some(42)
// }
```

### **Örnek 2: Public Key İsteme**

```rust
let request = DirectRequest::GetPublicKey;
let request_id = node.send_request(peer_id, request)?;

// Yanıt:
// DirectResponse::PublicKey { key: vec![...] }
```

### **Örnek 3: Node Reputation Kontrolü**

```rust
let request = DirectRequest::GetReputation {
    node_id: "node_3".to_string()
};
let request_id = node.send_request(peer_id, request)?;

// Yanıt:
// DirectResponse::Reputation {
//     node_id: "node_3",
//     score: 100
// }
```

### **Örnek 4: Custom Mesaj Gönderme**

```rust
let request = DirectRequest::CustomMessage {
    message: "Hello from Node 1!".to_string()
};
let request_id = node.send_request(peer_id, request)?;

// Yanıt:
// DirectResponse::CustomMessage {
//     message: "Hello back from Node 3!"
// }
```

---

## 🔧 **TEKNİK DETAYLAR**

### **1. Transport Security**

```rust
// Noise Protocol Configuration (node.rs)
SwarmBuilder::with_existing_identity(keypair)
    .with_tcp(
        tcp::Config::default(),
        noise::Config::new,      // ← Noise Protocol burada
        yamux::Config::default,
    )
```

**Şifreleme Stack:**
```
Plaintext Data
    ↓
JSON Serialization
    ↓
Noise Protocol Encryption (ChaCha20-Poly1305)
    ↓
Yamux Framing
    ↓
TCP Transmission
```

### **2. Peer Authentication**

Her node:
1. **Ed25519 Key Pair** oluşturur
2. **PeerId** = SHA-256(PublicKey)
3. **Noise handshake** sırasında public key doğrulanır
4. Sadece doğrulanmış peer'lar bağlanabilir

### **3. Message Format**

```json
// DirectRequest örneği
{
    "GetVoteStatus": {
        "tx_id": "tx_001"
    }
}

// DirectResponse örneği
{
    "VoteStatus": {
        "tx_id": "tx_001",
        "voted": true,
        "value": 42
    }
}
```

---

## 🧪 **TEST SENARYOLARI**

### **Test 1: Bağlı Peer'ları Görüntüle**

```bash
# Node'un loglarında ara
docker logs threshold-node1 2>&1 | grep "Connection established"
```

**Beklenen:**
```json
{"level":"INFO","message":"Connection established with peer: 12D3KooW... at /ip4/172.18.0.7/tcp/9000"}
```

### **Test 2: Direct Request Gönder (Kod ile)**

Henüz CLI komutu yok, ancak kod seviyesinde şöyle kullanabilirsiniz:

```rust
// app.rs içinde
pub async fn send_direct_request(
    &mut self,
    peer_id: PeerId,
    request: DirectRequest
) -> Result<()> {
    self.p2p_node.send_request(peer_id, request)?;
    Ok(())
}
```

### **Test 3: Network Monitoring**

```bash
# Her node'un kaç peer'a bağlı olduğunu kontrol et
docker logs threshold-node1 2>&1 | grep -c "Connection established"
docker logs threshold-node2 2>&1 | grep -c "Connection established"
```

---

## 🔒 **GÜVENLİK GARANTİLERİ**

### **Noise Protocol Tarafından Sağlanan:**

1. **Confidentiality (Gizlilik)**
   - Tüm mesajlar ChaCha20-Poly1305 ile şifrelenir
   - Üçüncü taraflar mesajları okuyamaz

2. **Authentication (Kimlik Doğrulama)**
   - Her peer Ed25519 imzası ile doğrulanır
   - Sahte peer'lar bağlanamaz

3. **Integrity (Bütünlük)**
   - Poly1305 MAC mesaj değiştirilmesini önler
   - Mesaj bozulması tespit edilir

4. **Forward Secrecy (İleri Gizlilik)**
   - Oturum anahtarları geçici
   - Eski mesajlar deşifre edilemez

5. **Replay Protection (Tekrar Saldırısı Koruması)**
   - Her mesaj benzersiz nonce ile işaretlenir
   - Eski mesajlar tekrar gönderilemez

---

## 📊 **PERFORMANS**

### **Handshake Karşılaştırması**

| Protokol | Round-Trips | Süre | CPU |
|----------|-------------|------|-----|
| mTLS 1.3 | 1-2 RTT | ~50ms | Orta |
| Noise XX | 1.5 RTT | ~30ms | Düşük |

### **Encryption Overhead**

- **Noise encryption**: ~5-10% CPU overhead
- **Throughput**: 100+ MB/s (modern CPU'da)
- **Latency**: <1ms ekleme

---

## 🎯 **ÖZET**

### **Kullanılan Protokoller:**

1. **TCP** - Transport layer
2. **Noise Protocol** - Encryption (mTLS alternatifi, daha iyi)
3. **Yamux** - Multiplexing
4. **GossipSub** - Broadcast mesajlar
5. **Request-Response** - İkili mesajlar (yeni eklendi)

### **mTLS vs Noise**

❌ **mTLS kullanılmıyor çünkü:**
- Sertifika yönetimi gerektirir
- P2P sistemlerde pratik değil
- Daha yavaş

✅ **Noise Protocol kullanılıyor çünkü:**
- Sertifika gerekmez (public key yeterli)
- P2P için tasarlanmış
- Daha hızlı ve modern
- WireGuard ile aynı güvenlik seviyesi

---

## 🚀 **SONRAKI ADIMLAR**

1. ✅ Request-Response protokolü eklendi
2. ⏳ CLI'dan direct request gönderme komutu eklenebilir
3. ⏳ Web UI ile peer'ları görüntüleme
4. ⏳ Grafana dashboard ile network monitoring

---

## 📚 **KAYNAKLAR**

- [Noise Protocol Framework](https://noiseprotocol.org/)
- [libp2p Documentation](https://docs.libp2p.io/)
- [WireGuard Protocol](https://www.wireguard.com/protocol/)
- [Request-Response Spec](https://github.com/libp2p/specs/blob/master/pubsub/gossipsub/README.md)
