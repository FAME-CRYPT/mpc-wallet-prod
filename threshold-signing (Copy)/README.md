# Threshold Signing System

> Distributed threshold ECDSA signature system implementing **3-of-4 threshold scheme** using **CGGMP24 protocol** on **secp256k1 curve**

## Table of Contents

- [Overview](#overview)
- [System Architecture](#system-architecture)
- [Components Deep Dive](#components-deep-dive)
  - [Node (Rust)](#node-rust)
  - [API Gateway (Go)](#api-gateway-go)
  - [Message Board (Go)](#message-board-go)
- [Cryptographic Operations](#cryptographic-operations)
- [Communication Flows](#communication-flows)
- [Quick Start](#quick-start)
- [Testing](#testing)
- [Performance](#performance)
- [Security Model](#security-model)
- [Production Considerations](#production-considerations)

---

## Overview

Bu proje, **threshold ECDSA imzalama sistemi**nin tam işlevsel bir implementasyonudur. Sistem, 4 düğümden herhangi 3'ünün işbirliği yaparak, **özel anahtarı hiçbir zaman yeniden oluşturmadan** imza üretmesini sağlar.

### Temel Özellikler

- **3-of-4 Threshold Scheme**: Herhangi 3 düğüm imza üretebilir, 1 düğüm çevrimdışı olabilir
- **CGGMP24 Protocol**: State-of-art threshold ECDSA protokolü
- **secp256k1 Curve**: Bitcoin/Ethereum ile aynı eğri
- **Presignature Pool**: Online imzalama için 10-50x hız artışı (~100ms'ye düşer)
- **Distributed Key Generation**: Özel anahtar hiçbir zaman tek bir yerde bulunmaz
- **Auto-verification**: Her imza otomatik olarak doğrulanır
- **RESTful API**: Kolay entegrasyon için HTTP API

### Ne Zaman Kullanılır?

- **Multi-party Custody**: Kripto varlıklar için çoklu imza güvenliği
- **Distributed Wallets**: Decentralize cüzdan sistemleri
- **High-security Signing**: Kritik işlemler için güvenli imzalama
- **Byzantine Fault Tolerance**: 1 düğüm hatalı/kötü niyetli olsa bile sistem çalışır

---

## System Architecture

```
┌─────────────┐
│   Client    │
│  (External) │
└──────┬──────┘
       │ HTTP :8000
       ▼
┌─────────────────┐
│  API Gateway    │ ◄─── External-facing REST API
│  (Go)           │      - POST /sign
│  Port: 8000     │      - GET /status/{id}
└────────┬────────┘      - GET /publickey
         │
         │ HTTP
         ▼
┌─────────────────────────────────────────┐
│       Message Board (Go)                │ ◄─── Central coordination server
│       Port: 8080 (internal)             │      - Request management
│  ┌────────────────────────────────┐     │      - Message exchange
│  │  In-Memory Store               │     │      - Partial signature collection
│  │  - Signing requests            │     │
│  │  - Protocol messages           │     │
│  │  - Partial signatures          │     │
│  │  - Presignature requests       │     │
│  │  - Public key                  │     │
│  └────────────────────────────────┘     │
└─────────┬───────────────────────────────┘
          │
    ┌─────┴─────┬─────────┬─────────┐
    │           │         │         │
    ▼           ▼         ▼         ▼
┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐
│ Node-1 │ │ Node-2 │ │ Node-3 │ │ Node-4 │ ◄─── Threshold signing nodes
│ (Rust) │ │ (Rust) │ │ (Rust) │ │ (Rust) │      - CGGMP24 protocol
│        │ │        │ │        │ │        │      - Presignature pool
│  idx=0 │ │  idx=1 │ │  idx=2 │ │  idx=3 │      - Background tasks
└────────┘ └────────┘ └────────┘ └────────┘
    │           │         │         │
    └───────────┴─────────┴─────────┘
         Internal Network (threshold-net)
```

### Mimari Kararlar

| Karar | Seçim | Gerekçe | Trade-off |
|-------|-------|---------|-----------|
| **Transport** | HTTP + Central MessageBoard | Basitlik, firewall-friendly, debug kolaylığı | Merkezi hata noktası |
| **Communication** | Polling (100-500ms) | Basit implementasyon, yeterli performans | Daha yüksek latency, bandwidth |
| **Storage** | In-memory | Demo için yeterli, hızlı, dependency yok | Persistence yok, restart = data loss |
| **Presignature** | Background pool (20-30) | 10-50x hız artışı, industry standard | Memory overhead, complexity |
| **Leadership** | Node-0 initiates presig | Duplicate work önleme, basit koordinasyon | Node-0'a dependency |

---

## Components Deep Dive

### Node (Rust)

**Dizin**: [`node/`](node/)
**Main**: [`src/main.rs`](node/src/main.rs)
**Language**: Rust
**Dependencies**: cggmp24, tokio, serde, sha2

#### Başlangıç Akışı

```rust
// 1. Konfigürasyon (Environment variables)
NODE_ID="node-1"              → Internal index: 0
MESSAGE_BOARD_URL="http://..."
RUST_LOG="info"

// 2. Distributed Key Generation (İlk çalıştırma)
// Dosya: src/keygen.rs:31-72
let incomplete_share = cggmp24::keygen::<Secp256k1>(eid, my_index, n=4)
    .set_threshold(t=3)
    .start(&mut rng, party)
    .await?;

// Kaydedilir: /tmp/key_share_{node_id}.json

// 3. Auxiliary Info Generation (Yavaş - ~30-60s)
// Dosya: src/aux_info.rs:23-58
let primes = PregeneratedPrimes::generate(&mut rng);  // Safe prime generation
let aux_info = cggmp24::aux_info_gen(eid, my_index, n=4)
    .start(&mut rng, party)
    .await?;

let complete_share = incomplete_share.complete(aux_info);
// Kaydedilir: /tmp/aux_info_{node_id}.json

// 4. Presignature Pool Başlatma
// Dosya: src/main.rs:165-180
let pool = PresignaturePool::new(target: 20, max: 30);
tokio::spawn(maintain_presignature_pool(...));  // Background task

// 5. Ana Imzalama Döngüsü
// Dosya: src/main.rs:146-250
loop {
    // 100ms'de bir MessageBoard'u poll et
    let pending = client.get_pending_requests().await?;

    for request in pending {
        if is_participant(request_id, my_index) {
            // Hızlı veya tam protokol
            sign_and_submit(request).await?;
        }
    }

    sleep(Duration::from_millis(100)).await;
}
```

#### Dosya Yapısı

```
node/
├── Cargo.toml              # Dependencies, binary definitions
├── Dockerfile              # Multi-stage build (rust → debian)
├── src/
│   ├── main.rs            # Entry point, main loop (272 lines)
│   ├── keygen.rs          # Distributed key generation (72 lines)
│   ├── aux_info.rs        # Auxiliary info generation (58 lines)
│   ├── signing.rs         # Full signing protocol (104 lines)
│   ├── signing_fast.rs    # Fast signing with presignatures (248 lines)
│   ├── presignature.rs    # Presignature generation (122 lines)
│   ├── presignature_pool.rs # Thread-safe pool management (124 lines)
│   ├── http_transport.rs  # HTTP adapter for CGGMP24 (336 lines)
│   ├── message_board_client.rs # MessageBoard HTTP client (272 lines)
│   ├── utils.rs           # Node ID parsing, data types (140 lines)
│   └── bin/
│       ├── verify_signature.rs    # Standalone verification (158 lines)
│       ├── verify_from_api.rs     # End-to-end verification (124 lines)
│       └── export_pubkey.rs       # Extract public key (56 lines)
```

#### Key Data Structures

```rust
// Complete key share (after DKG + AuxGen)
pub struct KeyShare<E: Curve> {
    incomplete: IncompleteKeyShare<E>,  // Core cryptographic material
    aux: AuxInfo<E>,                    // Zero-knowledge proof parameters
}

// Presignature için thread-safe pool
pub struct PresignaturePool<E, L> {
    presignatures: Arc<Mutex<VecDeque<StoredPresignature<E, L>>>>,
    target_size: usize,  // 20 - hedef boyut
    max_size: usize,     // 30 - maksimum boyut
}

// Stored presignature metadata
pub struct StoredPresignature<E, L> {
    presignature: Presignature<E>,           // Secret data (SINGLE-USE!)
    public_data: PresignaturePublicData<E>,  // Verification data
    participants: Vec<u16>,                  // [0, 1, 2] for node-1,2,3
    generated_at: SystemTime,                // Timestamp
}
```

#### HTTP Transport Adaptörü

**Dosya**: [`src/http_transport.rs`](node/src/http_transport.rs)

CGGMP24 protokolünün `Stream`/`Sink` trait'lerini HTTP'ye bridge eder:

```rust
// Gelen mesajlar için background task
// Dosya: src/http_transport.rs:123-180
async fn poll_messages(
    client: MessageBoardClient,
    request_id: String,
    sender: mpsc::Sender<ProtocolMessage>,
) {
    let mut seen_ids = HashSet::new();

    loop {
        let messages = client.get_messages(&request_id).await?;

        for msg in messages {
            if seen_ids.insert(msg.id.clone()) {  // Deduplication
                let proto_msg = serde_json::from_str(&msg.payload)?;
                sender.send(proto_msg).await?;
            }
        }

        sleep(Duration::from_millis(500)).await;
    }
}

// Giden mesajlar için background task
// Dosya: src/http_transport.rs:182-223
async fn post_messages(
    client: MessageBoardClient,
    request_id: String,
    mut receiver: mpsc::Receiver<ProtocolMessage>,
) {
    while let Some(msg) = receiver.recv().await {
        let to_node = match msg.destination {
            MessageDestination::AllParties => "*".to_string(),
            MessageDestination::OneParty(idx) => format!("node-{}", idx + 1),
        };

        client.post_message(
            &request_id,
            &to_node,
            msg.round,
            &serde_json::to_string(&msg.payload)?,
        ).await?;
    }
}
```

**Önemli Detay**: Node ID formatları
- **External**: "node-1", "node-2", "node-3", "node-4" (1-based)
- **Internal**: 0, 1, 2, 3 (0-based index)
- **Conversion**: [`src/utils.rs:12-29`](node/src/utils.rs#L12-L29)

```rust
pub fn parse_node_index(node_id: &str) -> Result<u16> {
    // "node-1" → 0
    let num: u16 = node_id.strip_prefix("node-")
        .and_then(|s| s.parse().ok())
        .ok_or(...)?;
    Ok(num - 1)  // 1-based → 0-based
}

pub fn format_node_id(index: u16) -> String {
    format!("node-{}", index + 1)  // 0-based → 1-based
}
```

#### Presignature Pool Management

**Dosya**: [`src/presignature_pool.rs`](node/src/presignature_pool.rs)

```rust
impl<E, L> PresignaturePool<E, L> {
    // FIFO pool
    pub fn take(&self) -> Option<StoredPresignature<E, L>> {
        self.presignatures.lock().unwrap().pop_front()
    }

    pub fn add(&self, presig: StoredPresignature<E, L>) -> Result<()> {
        let mut pool = self.presignatures.lock().unwrap();
        if pool.len() < self.max_size {
            pool.push_back(presig);
            Ok(())
        } else {
            Err(anyhow!("Pool full"))
        }
    }

    pub fn needs_refill(&self) -> bool {
        self.presignatures.lock().unwrap().len() < self.target_size
    }

    pub fn stats(&self) -> (usize, usize, f64) {
        let size = self.presignatures.lock().unwrap().len();
        let utilization = (size as f64 / self.target_size as f64) * 100.0;
        (size, self.target_size, utilization)
    }
}
```

**Background Maintenance Task**:
[`src/main.rs:165-180`](node/src/main.rs#L165-L180)

```rust
async fn maintain_presignature_pool(
    my_index: u16,
    pool: Arc<PresignaturePool<Secp256k1, 3>>,
    // ... other params
) {
    if my_index != 0 { return; }  // Sadece node-0 başlatır

    loop {
        sleep(Duration::from_secs(10)).await;  // 10s check interval

        if pool.needs_refill() {
            info!("Presignature pool needs refill ({}/{})...",
                  pool.current(), pool.target());

            // Generate new presignature
            match presignature::generate_presignature(...).await {
                Ok(presig) => pool.add(presig)?,
                Err(e) => error!("Failed to generate presignature: {}", e),
            }
        }
    }
}
```

---

### API Gateway (Go)

**Dizin**: [`api-gateway/`](api-gateway/)
**Main**: [`main.go`](api-gateway/main.go)
**Language**: Go
**Dependencies**: net/http, encoding/json

#### Purpose

External-facing REST API layer. Client'ların imzalama isteği göndermesi ve sonuç alması için tek giriş noktası.

#### Endpoints

##### `POST /sign`

**Request**:
```json
{
  "message": "Hello, threshold signing!"
}
```

**Response**:
```json
{
  "request_id": "req_1705067890_abc123",
  "status": "pending"
}
```

**Implementation**: [`main.go:71-107`](api-gateway/main.go#L71-L107)

```go
func handleSign(w http.ResponseWriter, r *http.Request) {
    var req SignRequest
    if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
        http.Error(w, "Invalid request", 400)
        return
    }

    // Validate
    if req.Message == "" {
        http.Error(w, "Message cannot be empty", 400)
        return
    }

    // Forward to MessageBoard
    mbReq := MessageBoardRequest{Message: req.Message}
    resp, err := http.Post(
        messageBoardURL + "/requests",
        "application/json",
        marshal(mbReq),
    )
    // ... error handling

    // Return request_id to client
    json.NewEncoder(w).Encode(result)
}
```

##### `GET /status/{request_id}`

**Response**:
```json
{
  "request_id": "req_1705067890_abc123",
  "status": "completed",
  "signature": "{\"r\":\"0de3...\", \"s\":\"7a2f...\"}"
}
```

**Implementation**: [`main.go:109-147`](api-gateway/main.go#L109-L147)

```go
func handleStatus(w http.ResponseWriter, r *http.Request) {
    requestID := mux.Vars(r)["request_id"]

    // Query MessageBoard
    resp, err := http.Get(
        messageBoardURL + "/requests/" + requestID,
    )

    if resp.StatusCode == 404 {
        json.NewEncoder(w).Encode(StatusResponse{
            RequestID: requestID,
            Status:    "not_found",
        })
        return
    }

    // Forward response
    var mbResp MessageBoardRequestResponse
    json.NewDecoder(resp.Body).Decode(&mbResp)

    json.NewEncoder(w).Encode(StatusResponse{
        RequestID: mbResp.ID,
        Status:    mbResp.Status,
        Signature: mbResp.Signature,
    })
}
```

##### `GET /publickey`

**Response**:
```json
{
  "public_key": "03de333b4a8f2e17c5a8b9d3f7e2c9a4d6e8f1a2b3c4d5e6f7a8b9c0d1e2f3a4"
}
```

**Implementation**: [`main.go:149-178`](api-gateway/main.go#L149-L178)

#### Data Types

**File**: [`types.go`](api-gateway/types.go)

```go
type SignRequest struct {
    Message string `json:"message"`  // UTF-8 string to sign
}

type SignResponse struct {
    RequestID string `json:"request_id"`
    Status    string `json:"status"`  // "pending"
}

type StatusResponse struct {
    RequestID string `json:"request_id"`
    Status    string `json:"status"`     // pending, completed, failed
    Signature string `json:"signature"`  // JSON string of signature object
}

type PublicKeyResponse struct {
    PublicKey string `json:"public_key"`  // Hex-encoded secp256k1 pubkey
}
```

#### Error Handling

| Status Code | Condition | Example |
|-------------|-----------|---------|
| **200** | Success | Request created, status retrieved |
| **400** | Bad Request | Missing message, invalid JSON |
| **404** | Not Found | Request ID doesn't exist |
| **500** | Internal Error | MessageBoard unavailable |

---

### Message Board (Go)

**Dizin**: [`message-board/`](message-board/)
**Main**: [`main.go`](message-board/main.go)
**Language**: Go
**Dependencies**: net/http, sync, encoding/json

#### Purpose

Central coordination server. Node'ların mesaj alışverişi yapması ve imzalama isteklerini yönetmesi için hub görevi görür.

#### Data Store

**File**: [`store.go`](message-board/store.go)

```go
type Store struct {
    mu sync.RWMutex  // Thread-safe access

    // Signing requests
    requests map[string]*SigningRequest

    // Presignature generation requests
    presignatureRequests map[string]*PresignatureRequest

    // Protocol messages (for signing)
    messages map[string]*NodeMessage
    messagesByRequest map[string][]*NodeMessage

    // Protocol messages (for presignature generation)
    presignatureMessages map[string]*NodeMessage
    presignatureMessagesByRequest map[string][]*NodeMessage

    // Partial signatures (fast signing mode)
    partialSignatures map[string]*PartialSignatureMessage
    partialSignaturesByRequest map[string][]*PartialSignatureMessage

    // Shared public key
    publicKey string
}
```

**Thread Safety**: [`store.go:78-93`](message-board/store.go#L78-L93)

```go
// Read operations
func (s *Store) GetRequest(id string) (*SigningRequest, error) {
    s.mu.RLock()  // Multiple readers allowed
    defer s.mu.RUnlock()
    // ...
}

// Write operations
func (s *Store) CreateRequest(message string) (*SigningRequest, error) {
    s.mu.Lock()  // Exclusive lock
    defer s.mu.Unlock()
    // ...
}
```

#### Endpoints

##### Request Management

**POST /requests**
[`main.go:65-86`](message-board/main.go#L65-L86)

```go
func handleCreateRequest(store *Store) http.HandlerFunc {
    return func(w http.ResponseWriter, r *http.Request) {
        var req CreateRequestPayload
        json.NewDecoder(r.Body).Decode(&req)

        request, err := store.CreateRequest(req.Message)
        // Returns: {id, message, status="pending", created_at}

        json.NewEncoder(w).Encode(request)
    }
}
```

**GET /requests?status=pending**
[`main.go:88-113`](message-board/main.go#L88-L113)

```go
func handleGetRequests(store *Store) http.HandlerFunc {
    return func(w http.ResponseWriter, r *http.Request) {
        statusFilter := r.URL.Query().Get("status")

        requests := store.ListRequests()

        // Filter by status if specified
        if statusFilter != "" {
            filtered := []SigningRequest{}
            for _, req := range requests {
                if req.Status == statusFilter {
                    filtered = append(filtered, req)
                }
            }
            requests = filtered
        }

        json.NewEncoder(w).Encode(requests)
    }
}
```

Node'lar bu endpoint'i poll ederek bekleyen işleri bulur:
```bash
GET /requests?status=pending
# Returns: [{id: "req_123", message: "...", status: "pending"}, ...]
```

**GET /requests/{id}**
[`main.go:115-135`](message-board/main.go#L115-L135)

```go
func handleGetRequest(store *Store) http.HandlerFunc {
    return func(w http.ResponseWriter, r *http.Request) {
        vars := mux.Vars(r)
        requestID := vars["id"]

        request, err := store.GetRequest(requestID)
        if err != nil {
            http.Error(w, "Request not found", 404)
            return
        }

        json.NewEncoder(w).Encode(request)
    }
}
```

**PUT /requests/{id}**
[`main.go:137-175`](message-board/main.go#L137-L175)

```go
func handleUpdateRequest(store *Store) http.HandlerFunc {
    return func(w http.ResponseWriter, r *http.Request) {
        requestID := mux.Vars(r)["id"]

        var update UpdateRequestPayload
        json.NewDecoder(r.Body).Decode(&update)

        // Update status
        if update.Status != "" {
            store.UpdateRequestStatus(requestID, update.Status)
        }

        // Set signature (auto-completes)
        if update.Signature != "" {
            store.SetRequestSignature(requestID, update.Signature)
        }

        // Return updated request
        request, _ := store.GetRequest(requestID)
        json.NewEncoder(w).Encode(request)
    }
}
```

Node'lar imzayı şöyle submit eder:
```bash
PUT /requests/req_123
{
  "signature": "{\"r\": \"0de3...\", \"s\": \"7a2f...\"}"
}
# Auto-sets status="completed"
```

##### Message Exchange (Protocol Communication)

**POST /messages**
[`main.go:187-217`](message-board/main.go#L187-L217)

```go
func handlePostMessage(store *Store) http.HandlerFunc {
    return func(w http.ResponseWriter, r *http.Request) {
        var msg PostMessagePayload
        json.NewDecoder(r.Body).Decode(&msg)

        // Create message with unique ID
        message := store.CreateMessage(
            msg.RequestID,
            msg.FromNode,
            msg.ToNode,    // "node-2" or "*" for broadcast
            msg.Round,
            msg.Payload,   // JSON-serialized protocol data
        )

        json.NewEncoder(w).Encode(message)
    }
}
```

**GET /messages?request_id=X&to_node=Y**
[`main.go:219-267`](message-board/main.go#L219-L267)

```go
func handleGetMessages(store *Store) http.HandlerFunc {
    return func(w http.ResponseWriter, r *http.Request) {
        requestID := r.URL.Query().Get("request_id")
        toNode := r.URL.Query().Get("to_node")

        // Auto-create request if doesn't exist (for keygen/auxgen)
        if !store.RequestExists(requestID) {
            store.CreateRequest("")  // Empty message for protocol-only
        }

        messages := store.GetMessages(requestID, toNode)
        json.NewEncoder(w).Encode(messages)
    }
}
```

**Filtering Logic**: [`store.go:176-197`](message-board/store.go#L176-L197)

```go
func (s *Store) GetMessages(requestID, toNode string) []*NodeMessage {
    s.mu.RLock()
    defer s.mu.RUnlock()

    allMessages := s.messagesByRequest[requestID]
    filtered := []*NodeMessage{}

    for _, msg := range allMessages {
        // Include if:
        // 1. Direct message to this node (to_node == "node-1")
        // 2. Broadcast message (to_node == "*")
        if msg.ToNode == toNode || msg.ToNode == "*" {
            filtered = append(filtered, msg)
        }
    }

    return filtered
}
```

##### Presignature Messages (Separate Namespace)

**POST /presignature-messages**
**GET /presignature-messages?request_id=X&to_node=Y**

[`main.go:269-361`](message-board/main.go#L269-L361)

Aynı logic, ama ayrı storage namespace kullanır:
- `presignatureMessages` map
- `presignatureMessagesByRequest` map

**Neden Ayrı?**
- Concurrent signing + presignature generation yapılabilsin
- Request ID collision önleme
- Daha temiz separation of concerns

##### Presignature Requests

**POST /presignature-requests**
[`main.go:363-384`](message-board/main.go#L363-L384)

```go
func handleCreatePresignatureRequest(store *Store) http.HandlerFunc {
    return func(w http.ResponseWriter, r *http.Request) {
        var req CreatePresignatureRequestPayload
        json.NewDecoder(r.Body).Decode(&req)

        request := store.CreatePresignatureRequest(req.RequestID)
        json.NewEncoder(w).Encode(request)
    }
}
```

Node-0 bu endpoint'i implicitly kullanır (presignature generation başlatırken).

**GET /presignature-requests?status=pending**
[`main.go:386-411`](message-board/main.go#L386-L411)

Diğer node'lar poll ederek participate ederler.

##### Partial Signatures (Fast Signing)

**POST /partial-signatures**
[`main.go:413-443`](message-board/main.go#L413-L443)

```go
func handlePostPartialSignature(store *Store) http.HandlerFunc {
    return func(w http.ResponseWriter, r *http.Request) {
        var msg PostPartialSignaturePayload
        json.NewDecoder(r.Body).Decode(&msg)

        partialSig := store.CreatePartialSignature(
            msg.RequestID,
            msg.FromNode,
            msg.Payload,  // JSON: {"s": "...", "e": "..."}
        )

        json.NewEncoder(w).Encode(partialSig)
    }
}
```

Her node kendi partial signature'ını post eder.

**GET /partial-signatures?request_id=X**
[`main.go:445-473`](message-board/main.go#L445-L473)

```go
func handleGetPartialSignatures(store *Store) http.HandlerFunc {
    return func(w http.ResponseWriter, r *http.Request) {
        requestID := r.URL.Query().Get("request_id")

        partialSigs := store.GetPartialSignatures(requestID)
        json.NewEncoder(w).Encode(partialSigs)
    }
}
```

Node'lar poll ederek tüm partial signature'ları toplar.

##### Public Key

**POST /publickey**
[`main.go:475-502`](message-board/main.go#L475-L502)

```go
func handlePostPublicKey(store *Store) http.HandlerFunc {
    return func(w http.ResponseWriter, r *http.Request) {
        var req PublicKeyPayload
        json.NewDecoder(r.Body).Decode(&req)

        // Idempotent: same value OK, different value = conflict
        err := store.SetPublicKey(req.PublicKey)
        if err != nil {
            http.Error(w, "Public key conflict", 409)
            return
        }

        json.NewEncoder(w).Encode(map[string]string{
            "public_key": req.PublicKey,
        })
    }
}
```

İlk keygen'i tamamlayan node public key'i post eder.

**GET /publickey**
[`main.go:504-528`](message-board/main.go#L504-L528)

Client'lar ve verification tool'ları public key'i buradan alır.

#### Data Types

**File**: [`types.go`](message-board/types.go)

```go
type SigningRequest struct {
    ID        string    `json:"id"`
    Message   string    `json:"message"`
    Status    string    `json:"status"`  // pending, in_progress, completed, failed
    Signature string    `json:"signature,omitempty"`
    CreatedAt time.Time `json:"created_at"`
    UpdatedAt time.Time `json:"updated_at"`
}

type NodeMessage struct {
    ID        string    `json:"id"`
    RequestID string    `json:"request_id"`
    FromNode  string    `json:"from_node"`  // "node-1"
    ToNode    string    `json:"to_node"`    // "node-2" or "*"
    Round     int       `json:"round"`
    Payload   string    `json:"payload"`    // JSON-serialized CGGMP24 message
    CreatedAt time.Time `json:"created_at"`
}

type PartialSignatureMessage struct {
    ID        string    `json:"id"`
    RequestID string    `json:"request_id"`
    FromNode  string    `json:"from_node"`
    Payload   string    `json:"payload"`  // {"s": "...", "e": "..."}
    CreatedAt time.Time `json:"created_at"`
}

type PresignatureRequest struct {
    ID        string    `json:"id"`
    RequestID string    `json:"request_id"`  // presig_gen_N
    Status    string    `json:"status"`
    CreatedAt time.Time `json:"created_at"`
}
```

---

## Cryptographic Operations

### 1. Distributed Key Generation (DKG)

**Protocol**: CGGMP24 Threshold Keygen
**File**: [`node/src/keygen.rs`](node/src/keygen.rs)
**Line**: [31-72](node/src/keygen.rs#L31-L72)

```rust
pub async fn run_keygen(
    eid: ExecutionId<'_>,
    my_index: u16,  // 0, 1, 2, 3
    n: u16,         // 4 total nodes
    t: u16,         // 3 threshold
    client: MessageBoardClient,
) -> Result<IncompleteKeyShare<Secp256k1>> {

    info!("Starting keygen with {} parties, threshold {}", n, t);

    // HTTP transport adapter
    let party = http_transport(eid, my_index, client, endpoint="/messages").await?;

    // CGGMP24 DKG protocol
    let key_share = cggmp24::keygen::<Secp256k1>(eid, my_index, n)
        .set_threshold(Some(t))
        .start(&mut rng, party)
        .await?;

    info!("Keygen completed. Public key: {:?}",
          key_share.shared_public_key);

    Ok(key_share)
}
```

**Communication Pattern**:
1. **Round 1**: Her node commitment broadcast eder
2. **Round 2**: Decommitment ve Feldman VSS shares
3. **Round 3**: Zero-knowledge proofs
4. **Round 4**: Complaint handling (eğer varsa)
5. **Result**: Her node unique private share + aynı public key alır

**Output**:
```rust
pub struct IncompleteKeyShare<E> {
    pub shared_public_key: Point<E>,  // Aynı tüm node'larda
    pub my_share: Scalar<E>,          // Farklı her node'da
    pub threshold: u16,               // 3
    pub total_parties: u16,           // 4
}
```

**Neden "Incomplete"?**
Çünkü henüz `AuxInfo` yok. AuxInfo olmadan imza oluşturamazsın (zero-knowledge proof'lar için gerekli).

### 2. Auxiliary Info Generation

**File**: [`node/src/aux_info.rs`](node/src/aux_info.rs)
**Line**: [23-58](node/src/aux_info.rs#L23-L58)

```rust
pub async fn run_aux_info_gen(
    eid: ExecutionId<'_>,
    my_index: u16,
    n: u16,
    client: MessageBoardClient,
) -> Result<AuxInfo<Secp256k1>> {

    info!("Generating safe primes (this may take ~30-60 seconds)...");

    // Safe prime generation (SLOW!)
    let primes = PregeneratedPrimes::generate(&mut rng);
    info!("Primes generated");

    // AuxInfo protocol
    let party = http_transport(eid, my_index, client, endpoint="/messages").await?;

    let aux_info = cggmp24::aux_info_gen::<Secp256k1>(eid, my_index, n)
        .start(&mut rng, party)
        .await?;

    info!("AuxInfo generation completed");

    Ok(aux_info)
}
```

**Neden Yavaş?**

Safe prime generation CPU-intensive:
- 2048-bit güçlü primler gerekli
- Probabilistic Miller-Rabin test
- Her node 30-60 saniye sürer

**Complete KeyShare**:

```rust
let complete_share = incomplete_share.complete(aux_info);
// Artık imzalama yapılabilir!
```

### 3. Presignature Generation (Offline Phase)

**File**: [`node/src/presignature.rs`](node/src/presignature.rs)
**Line**: [30-87](node/src/presignature.rs#L30-L87)

```rust
pub async fn generate_presignature(
    request_id: String,
    my_index: u16,
    participants: Vec<u16>,  // [0, 1, 2]
    key_share: &KeyShare<Secp256k1>,
    client: MessageBoardClient,
) -> Result<StoredPresignature<Secp256k1, 3>> {

    let eid = ExecutionId::new(request_id.as_bytes());

    info!("Generating presignature with participants {:?}", participants);

    // Separate presignature transport
    let party = http_transport_presignature(
        eid,
        my_index,
        client,
        endpoint="/presignature-messages"
    ).await?;

    // Run presignature protocol
    let (presignature, public_data) = cggmp24::signing(
        eid,
        my_index,
        &participants,
        key_share,
    )
    .generate_presignature(&mut rng, party)
    .await?;

    info!("Presignature generated successfully");

    Ok(StoredPresignature {
        presignature,
        public_data,
        participants,
        generated_at: SystemTime::now(),
    })
}
```

**Timing**: ~4 saniye (network-intensive)

**Critical Property**: **SINGLE-USE ONLY!**
```rust
// ⚠️ Presignature'ı iki kez kullanmak private key'i leak eder!
let sig1 = presignature.issue_partial_signature(message1);
let sig2 = presignature.issue_partial_signature(message2); // 🔥 DANGER!
```

Bu yüzden pool'dan `take()` ile alınır (remove), `borrow()` değil.

### 4. Fast Signing (Online Phase)

**File**: [`node/src/signing_fast.rs`](node/src/signing_fast.rs)
**Line**: [52-210](node/src/signing_fast.rs#L52-L210)

#### Fast Path (Presignature Available)

```rust
pub async fn sign_message_fast(
    request_id: String,
    message: String,
    my_index: u16,
    key_share: &KeyShare<Secp256k1>,
    presignature_pool: Arc<PresignaturePool<Secp256k1, 3>>,
    client: MessageBoardClient,
) -> Result<SignatureRecid> {

    // Try to take presignature from pool
    let stored_presig = match presignature_pool.take() {
        Some(p) => {
            info!("Using presignature from pool");
            p
        },
        None => {
            warn!("No presignature available, falling back to full signing");
            return signing::sign_message(...).await;  // Slow path
        }
    };

    // Hash message
    let data_to_sign = DataToSign::digest::<Sha256>(message.as_bytes());

    // Issue partial signature (INSTANT!)
    let partial_signature = stored_presig.presignature
        .issue_partial_signature(&data_to_sign);

    info!("Partial signature issued locally");

    // Post to MessageBoard
    let payload = serde_json::to_string(&PartialSignaturePayload {
        s: partial_signature.s,
        e: partial_signature.e,
    })?;

    client.post_partial_signature(&request_id, &payload).await?;

    // Collect all partial signatures (POLL)
    let all_partials = collect_partial_signatures(
        &request_id,
        &stored_presig.participants,  // [0, 1, 2]
        &client,
    ).await?;

    info!("All {} partial signatures collected", all_partials.len());

    // Combine into full signature
    let signature = PartialSignature::combine(
        &all_partials,
        &data_to_sign,
        &stored_presig.public_data,
    )?;

    // Verify before returning
    signature.verify(&key_share.shared_public_key(), &data_to_sign)?;

    info!("Fast signature generated and verified successfully");

    Ok(signature)
}
```

**Timing Breakdown**:
- Partial signature: **instant** (local computation)
- Collect partials: **100-500ms** (HTTP polling)
- Combine: **instant** (aggregation)
- **Total: ~100-500ms** (10-50x faster than full protocol)

#### Collecting Partial Signatures

[`signing_fast.rs:164-210`](node/src/signing_fast.rs#L164-L210)

```rust
async fn collect_partial_signatures(
    request_id: &str,
    participants: &[u16],  // [0, 1, 2]
    client: &MessageBoardClient,
) -> Result<Vec<PartialSignature<Secp256k1>>> {

    let expected_count = participants.len();  // 3
    let mut collected = Vec::new();

    // Poll for up to 30 seconds
    for attempt in 0..300 {
        let partials = client.get_partial_signatures(request_id).await?;

        if partials.len() >= expected_count {
            // Sort by participant order (important!)
            let mut sorted: Vec<_> = partials.into_iter()
                .map(|p| (parse_node_index(&p.from_node), p.payload))
                .collect();
            sorted.sort_by_key(|(idx, _)| *idx);

            // Deserialize
            for (_, payload) in sorted.iter().take(expected_count) {
                let ps: PartialSignaturePayload = serde_json::from_str(payload)?;
                collected.push(PartialSignature {
                    s: ps.s,
                    e: ps.e,
                });
            }

            return Ok(collected);
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    Err(anyhow!("Timeout waiting for partial signatures"))
}
```

**Önemli**: Partial signature'lar participant order'da olmalı!
- Participant [0, 1, 2] ise, partial_sigs[0] node-0'dan gelmelidir.
- Yanlış sıra → combine fail

### 5. Full Signing Protocol (Fallback)

**File**: [`node/src/signing.rs`](node/src/signing.rs)
**Line**: [28-104](node/src/signing.rs#L28-L104)

```rust
pub async fn sign_message(
    request_id: String,
    message: String,
    my_index: u16,
    participants: Vec<u16>,
    key_share: &KeyShare<Secp256k1>,
    client: MessageBoardClient,
) -> Result<SignatureRecid> {

    let eid = ExecutionId::new(request_id.as_bytes());

    info!("Starting full signing protocol with participants {:?}", participants);

    // Hash message
    let data_to_sign = DataToSign::digest::<Sha256>(message.as_bytes());

    // HTTP transport
    let party = http_transport(eid, my_index, client, endpoint="/messages").await?;

    // Full CGGMP24 signing
    let signature = cggmp24::signing(eid, my_index, &participants, key_share)
        .sign(&mut rng, party, &data_to_sign)
        .await?;

    // Verify
    signature.verify(&key_share.shared_public_key(), &data_to_sign)?;

    info!("Full signature generated and verified");

    Ok(signature)
}
```

**Timing**: ~4-6 saniye

**Ne Zaman Kullanılır?**
- Presignature pool boş
- İlk imza (pool henüz dolmadı)
- Fallback mechanism

**Communication Rounds**:
1. Round 1: Nonce commitments
2. Round 2: Nonce reveals + proof of knowledge
3. Round 3: Partial signature shares
4. Round 4: Final aggregation + verification

---

## Communication Flows

### Flow 1: Sistem Başlatma (System Startup)

```
─────────────────────────────────────────────────────────────────────
Time: T=0 (podman-compose up)
─────────────────────────────────────────────────────────────────────

1. MessageBoard starts
   └─> Listening on :8080
   └─> In-memory store initialized

2. API Gateway starts
   └─> Depends on MessageBoard
   └─> Listening on :8000 (external)

3. Nodes start (node-1, node-2, node-3, node-4)
   └─> Depends on MessageBoard
   └─> Each reads NODE_ID from env
   └─> Sleep 100ms for coordination

─────────────────────────────────────────────────────────────────────
Time: T=100ms (Keygen begins)
─────────────────────────────────────────────────────────────────────

4. All 4 nodes check for key_share files
   ├─> /tmp/key_share_node-1.json - NOT FOUND
   ├─> /tmp/key_share_node-2.json - NOT FOUND
   ├─> /tmp/key_share_node-3.json - NOT FOUND
   └─> /tmp/key_share_node-4.json - NOT FOUND

5. All 4 nodes start keygen simultaneously
   ├─> ExecutionId: "threshold_keygen_initial_2024___"
   ├─> n=4, t=3
   └─> cggmp24::keygen() protocol

   Node-1 ──┐
   Node-2 ──┼─> POST /messages (broadcast commitments)
   Node-3 ──┤   └─> MessageBoard stores
   Node-4 ──┘

   Node-1 ──┐
   Node-2 ──┼─> GET /messages?request_id=...&to_node=node-X
   Node-3 ──┤   └─> MessageBoard returns messages for each node
   Node-4 ──┘

   [Multiple rounds of message exchange...]

6. Keygen completes (~5-10 seconds)
   ├─> Each node saves /tmp/key_share_{node_id}.json
   └─> All nodes have same public key, different shares

─────────────────────────────────────────────────────────────────────
Time: T=10s (AuxInfo generation begins)
─────────────────────────────────────────────────────────────────────

7. All 4 nodes check for aux_info files
   └─> NOT FOUND → start aux_info generation

8. Prime generation (LOCAL, NO NETWORK)
   ├─> Node-1: Generating 2048-bit safe primes... ~30-50s
   ├─> Node-2: Generating 2048-bit safe primes... ~30-50s
   ├─> Node-3: Generating 2048-bit safe primes... ~30-50s
   └─> Node-4: Generating 2048-bit safe primes... ~30-50s

9. AuxInfo protocol (NETWORK)
   └─> Similar message exchange pattern as keygen
   └─> ExecutionId: "threshold_auxgen_initial_2024__"

10. AuxInfo completes (~30-60s total)
    ├─> Each node saves /tmp/aux_info_{node_id}.json
    ├─> Combines with key_share → complete KeyShare
    └─> Node-1 (first to finish) POSTs public key to MessageBoard

    Node-1 ──> POST /publickey
               └─> Body: {"public_key": "03de333b..."}
               └─> MessageBoard stores (idempotent)

─────────────────────────────────────────────────────────────────────
Time: T=60s (System ready for signing)
─────────────────────────────────────────────────────────────────────

11. Presignature pool initialization
    └─> Only Node-0 (node-1) starts background task
    └─> Target: 20, Max: 30
    └─> Begins generating first presignature

12. Main signing loop starts (all nodes)
    └─> Poll GET /requests?status=pending every 100ms
    └─> Waiting for signing requests...

─────────────────────────────────────────────────────────────────────
System ready to accept signing requests!
─────────────────────────────────────────────────────────────────────
```

### Flow 2: İmzalama İsteği (Signing Request - Fast Path)

```
─────────────────────────────────────────────────────────────────────
Client submits signing request
─────────────────────────────────────────────────────────────────────

1. Client → API Gateway
   POST http://localhost:8000/sign
   Body: {"message": "Hello, threshold signing!"}

2. API Gateway → MessageBoard
   POST http://message-board:8080/requests
   Body: {"message": "Hello, threshold signing!"}

   MessageBoard creates request:
   {
     "id": "req_1705067890_abc123",
     "message": "Hello, threshold signing!",
     "status": "pending",
     "created_at": "2024-01-12T10:24:50Z"
   }

3. API Gateway → Client
   Response: {
     "request_id": "req_1705067890_abc123",
     "status": "pending"
   }

─────────────────────────────────────────────────────────────────────
Nodes detect and process request (T=0-100ms)
─────────────────────────────────────────────────────────────────────

4. All nodes poll MessageBoard
   Node-1,2,3,4 ──> GET /requests?status=pending
                    └─> MessageBoard returns [request]

5. Nodes check if they are participants
   Request ID: "req_1705067890_abc123"
   Hash → determines participants

   is_participant(req_id, node_index):
     hash = blake2b(req_id)
     participant_indices = hash[0..3]  # First 3 bytes
     return node_index in participants

   Result:
   ├─> Node-0: ✓ PARTICIPANT (processes)
   ├─> Node-1: ✓ PARTICIPANT (processes)
   ├─> Node-2: ✓ PARTICIPANT (processes)
   └─> Node-3: ✗ NOT PARTICIPANT (skips)

─────────────────────────────────────────────────────────────────────
Fast signing begins (T=100ms)
─────────────────────────────────────────────────────────────────────

6. Node-0 (node-1) checks presignature pool
   └─> pool.take() → Some(presignature) ✓
   └─> Fast path!

7. Node-0 computes partial signature (INSTANT)
   data_to_sign = SHA256("Hello, threshold signing!")
   partial_sig = presignature.issue_partial_signature(data_to_sign)

8. Node-0 posts partial signature
   POST /partial-signatures
   Body: {
     "request_id": "req_1705067890_abc123",
     "from_node": "node-1",
     "payload": "{\"s\": \"...\", \"e\": \"...\"}"
   }

9. Node-1 and Node-2 do the same
   ├─> Node-1 → POST /partial-signatures (from_node: "node-2")
   └─> Node-2 → POST /partial-signatures (from_node: "node-3")

─────────────────────────────────────────────────────────────────────
Collecting partial signatures (T=100-500ms)
─────────────────────────────────────────────────────────────────────

10. All nodes poll for partial signatures
    Node-0,1,2 ──> GET /partial-signatures?request_id=req_...
                   └─> Wait for 3 partial signatures

    [Poll every 100ms]

    Attempt 1: 1 partial signature found (from node-1)
    Attempt 2: 2 partial signatures found (from node-1, node-2)
    Attempt 3: 3 partial signatures found (from node-1, node-2, node-3) ✓

11. First node to collect (say, Node-0) combines
    partial_sigs = [node-1's, node-2's, node-3's]
    signature = PartialSignature::combine(
        partial_sigs,
        data_to_sign,
        public_data
    )

12. Node-0 verifies signature
    signature.verify(shared_public_key, data_to_sign)?
    ✓ Verification passed

13. Node-0 submits signature
    PUT /requests/req_1705067890_abc123
    Body: {
      "signature": "{\"r\": \"0de3...\", \"s\": \"7a2f...\"}"
    }

    MessageBoard auto-updates:
    {
      "id": "req_1705067890_abc123",
      "status": "completed",  ← Auto-set
      "signature": "{...}"
    }

─────────────────────────────────────────────────────────────────────
Client retrieves result (T=500ms-2s)
─────────────────────────────────────────────────────────────────────

14. Client polls status
    [Retry loop]

    GET http://localhost:8000/status/req_1705067890_abc123

    Attempt 1: {"status": "pending"}
    Attempt 2: {"status": "pending"}
    Attempt 3: {"status": "completed", "signature": "{...}"}

    ✓ Signature received!

─────────────────────────────────────────────────────────────────────
Total latency: ~500ms-2s (Fast path)
─────────────────────────────────────────────────────────────────────
```

### Flow 3: Presignature Generation (Background)

```
─────────────────────────────────────────────────────────────────────
Node-0 detects pool needs refill
─────────────────────────────────────────────────────────────────────

1. Background task checks pool (every 10s)
   Node-0 ──> pool.needs_refill()?
              └─> size=15, target=20 → YES, needs refill

2. Node-0 starts presignature generation
   request_id = format!("presig_gen_{}", next_counter++)
   eid = ExecutionId::new(request_id.as_bytes())

   participants = [0, 1, 2]  # First 3 nodes

   info!("Starting presignature generation: {}", request_id)

─────────────────────────────────────────────────────────────────────
Protocol execution (T=0-4s)
─────────────────────────────────────────────────────────────────────

3. Node-0 → MessageBoard (implicitly via http_transport_presignature)
   POST /presignature-messages
   Body: {
     "request_id": "presig_gen_42",
     "from_node": "node-1",
     "to_node": "node-2",  # or "*" for broadcast
     "round": 1,
     "payload": "{...cggmp24 protocol data...}"
   }

4. Other nodes poll for presignature requests
   Node-1,2,3 ──> GET /presignature-requests?status=pending
                  └─> Sees "presig_gen_42"

   Node-1 and Node-2 (participants) join:
   ├─> Node-1: I'm participant! Starting presignature protocol
   └─> Node-2: I'm participant! Starting presignature protocol

   Node-3 (not participant):
   └─> Not in participants list, skipping

5. All participants exchange messages
   Node-0,1,2 ──┐
                ├─> POST /presignature-messages
                └─> GET /presignature-messages?request_id=presig_gen_42&to_node=node-X

   [Multiple rounds, ~4 seconds]

─────────────────────────────────────────────────────────────────────
Presignature completed (T=4s)
─────────────────────────────────────────────────────────────────────

6. Each participant (Node-0, Node-1, Node-2) generates locally
   (presignature, public_data) = protocol_result

7. Each adds to their LOCAL pool
   Node-0 ──> pool.add(presignature) → size=16
   Node-1 ──> pool.add(presignature) → size=16
   Node-2 ──> pool.add(presignature) → size=16

   ⚠️ Presignature NOT sent to MessageBoard
   ⚠️ Each node has independent pool

8. Pool stats update
   Node-0: Pool size: 16/20 (80% utilization)

   Still needs more, will generate again in 10s

─────────────────────────────────────────────────────────────────────
This repeats until pool reaches target (20 presignatures)
─────────────────────────────────────────────────────────────────────
```

### Flow 4: Full Signing Protocol (Fallback)

```
─────────────────────────────────────────────────────────────────────
Scenario: No presignature available
─────────────────────────────────────────────────────────────────────

1. Node-0 receives signing request
   request_id = "req_1705067999_xyz789"

2. Node-0 checks pool
   └─> pool.take() → None ✗
   └─> Fallback to full signing protocol

   warn!("No presignature available, using full protocol")

─────────────────────────────────────────────────────────────────────
Full protocol execution (T=0-6s)
─────────────────────────────────────────────────────────────────────

3. Node-0,1,2 start full signing
   eid = ExecutionId::new(request_id.as_bytes())
   data_to_sign = SHA256(message)

4. Round 1: Nonce commitments (broadcast)
   Node-0,1,2 ──> POST /messages
                  └─> to_node="*", round=1, payload={commitment}

5. Round 2: Nonce reveals (broadcast)
   Node-0,1,2 ──> POST /messages
                  └─> to_node="*", round=2, payload={nonce, proof}

6. Round 3: Partial signature shares (P2P)
   Node-0 ──> POST /messages → to_node="node-2"
   Node-0 ──> POST /messages → to_node="node-3"
   Node-1 ──> POST /messages → to_node="node-1"
   Node-1 ──> POST /messages → to_node="node-3"
   ... (each node sends to others)

7. Round 4: Final aggregation
   Each node collects all messages:
   ├─> GET /messages?request_id=...&to_node=node-1
   └─> Aggregates partial shares → full signature

8. Verification (same as fast path)
   signature.verify(shared_public_key, data_to_sign)?

9. Submit signature
   First node to complete submits:
   PUT /requests/req_1705067999_xyz789
   Body: {"signature": "{...}"}

─────────────────────────────────────────────────────────────────────
Total latency: ~4-6s (10-50x slower than fast path)
─────────────────────────────────────────────────────────────────────
```

---

## Quick Start

### Prerequisites

```bash
# Container runtime
podman --version  # or docker --version

# Rust (for building utilities)
rustc --version

# curl (for testing)
curl --version
```

### 1. Build and Start

```bash
# Build all containers
podman-compose build

# Start system (4 nodes + MessageBoard + API Gateway)
podman-compose up -d

# Check logs
podman-compose logs -f
```

**Expected startup time**: ~60 seconds (keygen + aux_info)

### 2. Wait for System Ready

```bash
# Watch node logs
./scripts/logs.sh

# Look for:
# "Keygen completed"
# "AuxInfo generation completed"
# "Presignature pool initialized"
# "Main signing loop started"
```

### 3. Submit Signing Request

```bash
# Using API Gateway
curl -X POST http://localhost:8000/sign \
  -H "Content-Type: application/json" \
  -d '{"message": "Hello, threshold signing!"}'

# Response:
# {
#   "request_id": "req_1705067890_abc123",
#   "status": "pending"
# }
```

### 4. Check Status

```bash
# Poll for completion
curl http://localhost:8000/status/req_1705067890_abc123

# When completed:
# {
#   "request_id": "req_1705067890_abc123",
#   "status": "completed",
#   "signature": "{\"r\": \"...\", \"s\": \"...\"}"
# }
```

### 5. Verify Signature

```bash
# Get public key
curl http://localhost:8000/publickey

# Verify using utility
cd node
cargo run --bin verify-from-api -- \
  --message "Hello, threshold signing!" \
  --request-id req_1705067890_abc123

# Output: ✓ Signature verified successfully
```

---

## Testing

### Automated Tests

```bash
# Unit tests (MessageBoard API)
./scripts/test-message-board.sh

# Integration tests (API Gateway)
./scripts/test-api-gateway.sh

# End-to-end test (Full signing flow)
./scripts/test-e2e.sh

# All tests
./scripts/test-message-board.sh && \
./scripts/test-api-gateway.sh && \
./scripts/test-e2e.sh
```

### Interactive Demo

```bash
./scripts/demo.sh
```

**Demo içeriği**:
1. System health check
2. Public key retrieval
3. Signing request submission
4. Status polling
5. Signature extraction
6. Visual flow diagram

### Manual Testing

```bash
# Test MessageBoard directly
curl -X POST http://localhost:8080/requests \
  -d '{"message": "test"}' \
  -H "Content-Type: application/json"

# Check pending requests
curl http://localhost:8080/requests?status=pending

# Post protocol message
curl -X POST http://localhost:8080/messages \
  -d '{
    "request_id": "test_123",
    "from_node": "node-1",
    "to_node": "*",
    "round": 1,
    "payload": "{\"test\": \"data\"}"
  }' \
  -H "Content-Type: application/json"

# Retrieve messages
curl "http://localhost:8080/messages?request_id=test_123&to_node=node-1"
```

### Load Testing

```bash
# Submit 10 concurrent signing requests
for i in {1..10}; do
  curl -X POST http://localhost:8000/sign \
    -d "{\"message\": \"Load test $i\"}" \
    -H "Content-Type: application/json" &
done
wait

# Monitor node logs
./scripts/logs.sh | grep "signature generated"
```

---

## Performance

### Timing Benchmarks

| Operation | Cold Start | Warm Start | Notes |
|-----------|------------|------------|-------|
| **System Startup** | 60s | N/A | Keygen (10s) + AuxInfo (50s) |
| **Keygen** | 5-10s | N/A | One-time, all nodes |
| **AuxInfo Generation** | 30-60s | N/A | One-time, prime generation |
| **Full Signing** | 4-6s | 4-6s | Network-intensive, multi-round |
| **Fast Signing** | 2-4s | **100-500ms** | With presignature |
| **Presignature Gen** | 4s | 4s | Background, async |

### Throughput

**Fast Path (with pool)**:
- Latency: ~100-500ms per signature
- Throughput: ~2-10 signatures/second (with 20-presig pool)
- Bottleneck: Pool refill rate (~1 presig/4s)

**Full Protocol**:
- Latency: ~4-6s per signature
- Throughput: ~0.2 signatures/second
- Bottleneck: Protocol rounds + network latency

### Optimization Strategies

1. **Larger Presignature Pool**
   ```rust
   // Increase target/max
   let pool = PresignaturePool::new(target: 50, max: 100);
   ```
   - Trade-off: More memory, longer startup

2. **Parallel Presignature Generation**
   ```rust
   // Multiple concurrent generations
   for _ in 0..5 {
       tokio::spawn(generate_presignature(...));
   }
   ```
   - Trade-off: More CPU, faster pool fill

3. **WebSocket Transport**
   - Replace HTTP polling with WebSockets
   - Reduce latency by ~100-200ms
   - More complex implementation

4. **Database Storage**
   - Persistent MessageBoard state
   - Reduces memory usage
   - Enables horizontal scaling

---

## Security Model

### Threat Model

**Assumptions**:
- **Honest Majority**: At least 3 of 4 nodes are honest
- **Byzantine Tolerance**: System tolerates 1 malicious/faulty node
- **Network**: Asynchronous network, messages can be delayed
- **Coordination**: MessageBoard is trusted (for this demo)

**Attacks Defended Against**:
- ✓ Single node compromise (can't sign alone)
- ✓ Signature forgery (ECDSA security)
- ✓ Key extraction (threshold property)
- ✓ Replay attacks (unique request IDs)

**Attacks NOT Defended Against** (demo limitations):
- ✗ MessageBoard compromise (central trust)
- ✗ Network-level attacks (no encryption)
- ✗ Denial of service (no rate limiting)
- ✗ Persistent adversary (no key rotation)

### Cryptographic Security

**CGGMP24 Protocol**:
- Provably secure in UC model
- Security parameter: 128-bit (secp256k1)
- Zero-knowledge proofs for verifiability

**Key Properties**:
- **Confidentiality**: No single node knows full private key
- **Integrity**: Signatures verifiable with public key
- **Non-repudiation**: Signatures mathematically binding

**Presignature Security**:
```rust
// ⚠️ CRITICAL: Single-use only!
// Reusing presignature leaks private key via nonce reuse

// Safe:
let presig = pool.take();  // Removes from pool
let sig = presig.issue_partial_signature(msg);

// Unsafe:
let presig = pool.borrow();  // Hypothetical - don't implement!
let sig1 = presig.issue_partial_signature(msg1);
let sig2 = presig.issue_partial_signature(msg2);  // 🔥 Private key leaked!
```

### Network Security

**Current State** (Demo):
- HTTP plaintext communication
- No authentication
- No authorization
- Internal Docker network (isolated)

**Production Requirements**:
- **mTLS**: Mutual TLS for node authentication
- **API Keys**: Client authentication for API Gateway
- **Message Signing**: Cryptographic signatures on all messages
- **Rate Limiting**: Per-client, per-node throttling
- **Encryption**: TLS 1.3 for all communication

### Operational Security

**Key Storage**:
```bash
# Current: Ephemeral storage
/tmp/key_share_node-1.json
/tmp/aux_info_node-1.json

# Production: Encrypted persistent storage
# Options:
# - Hardware Security Module (HSM)
# - Encrypted filesystem (LUKS, EncFS)
# - Secret management (Vault, AWS Secrets Manager)
```

**Backup Strategy**:
- Each key share must be backed up independently
- Requires t=3 shares to recover signing capability
- **Never store 3+ shares together** (defeats threshold property)

**Disaster Recovery**:
1. **Scenario**: 2 nodes lost (still have 2 shares)
   - Solution: Can't sign anymore, need key resharing protocol

2. **Scenario**: 1 node lost (have 3 shares)
   - Solution: System operational, add new node via key addition protocol

3. **Scenario**: MessageBoard lost
   - Solution: No persistence, need to restart keygen (demo limitation)

---

## Production Considerations

### Critical Changes Needed

#### 1. Persistent Storage

**Problem**: In-memory storage lost on restart

**Solution**:
```go
// MessageBoard with PostgreSQL
type Store struct {
    db *sql.DB
}

func (s *Store) CreateRequest(message string) (*SigningRequest, error) {
    var req SigningRequest
    err := s.db.QueryRow(`
        INSERT INTO requests (id, message, status, created_at)
        VALUES ($1, $2, 'pending', NOW())
        RETURNING id, message, status, created_at
    `, generateID(), message).Scan(&req.ID, &req.Message, &req.Status, &req.CreatedAt)
    return &req, err
}
```

**Benefits**:
- Survives restarts
- Audit trail
- Analytics

#### 2. Authentication & Authorization

**Node Authentication**:
```rust
// mTLS configuration
let client_cert = tokio::fs::read("node-1-cert.pem").await?;
let client_key = tokio::fs::read("node-1-key.pem").await?;
let ca_cert = tokio::fs::read("ca-cert.pem").await?;

let client = reqwest::Client::builder()
    .identity(Identity::from_pem(&client_cert, &client_key)?)
    .add_root_certificate(Certificate::from_pem(&ca_cert)?)
    .build()?;
```

**API Authentication**:
```go
func authMiddleware(next http.Handler) http.Handler {
    return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
        apiKey := r.Header.Get("X-API-Key")
        if !isValidAPIKey(apiKey) {
            http.Error(w, "Unauthorized", 401)
            return
        }
        next.ServeHTTP(w, r)
    })
}
```

#### 3. High Availability

**MessageBoard Replication**:
```yaml
# Kubernetes deployment
apiVersion: apps/v1
kind: Deployment
metadata:
  name: message-board
spec:
  replicas: 3  # 3 instances
  selector:
    matchLabels:
      app: message-board
  template:
    spec:
      containers:
      - name: message-board
        image: message-board:latest
        env:
        - name: DATABASE_URL
          value: postgresql://...  # Shared database
```

**Load Balancing**:
```yaml
apiVersion: v1
kind: Service
metadata:
  name: message-board
spec:
  type: LoadBalancer
  selector:
    app: message-board
  ports:
  - port: 8080
    targetPort: 8080
```

#### 4. Monitoring & Observability

**Metrics (Prometheus)**:
```rust
use prometheus::{Counter, Histogram, register_counter, register_histogram};

lazy_static! {
    static ref SIGNATURES_TOTAL: Counter =
        register_counter!("signatures_total", "Total signatures generated").unwrap();

    static ref SIGNING_DURATION: Histogram =
        register_histogram!("signing_duration_seconds", "Signing latency").unwrap();
}

// In signing code
let timer = SIGNING_DURATION.start_timer();
let signature = sign_message(...).await?;
timer.observe_duration();
SIGNATURES_TOTAL.inc();
```

**Distributed Tracing (OpenTelemetry)**:
```rust
use opentelemetry::trace::{Tracer, TracerProvider};

#[instrument(skip(key_share, client))]
async fn sign_message_fast(
    request_id: String,
    message: String,
    // ...
) -> Result<SignatureRecid> {
    let span = tracer.start("sign_message_fast");
    span.set_attribute("request_id", request_id.clone());

    // ... signing logic

    span.end();
    Ok(signature)
}
```

**Alerting**:
```yaml
# Prometheus alert rules
groups:
- name: threshold_signing
  rules:
  - alert: HighSigningLatency
    expr: histogram_quantile(0.95, signing_duration_seconds) > 2
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: "95th percentile signing latency > 2s"

  - alert: PresignaturePoolLow
    expr: presignature_pool_size < 5
    for: 1m
    labels:
      severity: warning
    annotations:
      summary: "Presignature pool below 5"
```

#### 5. Rate Limiting

**API Gateway**:
```go
import "golang.org/x/time/rate"

var limiter = rate.NewLimiter(rate.Limit(10), 20)  // 10 req/s, burst 20

func rateLimitMiddleware(next http.Handler) http.Handler {
    return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
        if !limiter.Allow() {
            http.Error(w, "Rate limit exceeded", 429)
            return
        }
        next.ServeHTTP(w, r)
    })
}
```

**MessageBoard**:
```go
// Per-node rate limiting
var nodeLimiters = make(map[string]*rate.Limiter)

func getNodeLimiter(nodeID string) *rate.Limiter {
    if limiter, exists := nodeLimiters[nodeID]; exists {
        return limiter
    }
    limiter := rate.NewLimiter(rate.Limit(100), 200)  // 100 msg/s per node
    nodeLimiters[nodeID] = limiter
    return limiter
}
```

#### 6. Key Management

**HSM Integration**:
```rust
use aws_kms::Client as KmsClient;

async fn save_key_share_encrypted(share: &KeyShare) -> Result<()> {
    let kms = KmsClient::new(&config);

    // Encrypt key share
    let plaintext = serde_json::to_vec(share)?;
    let encrypted = kms.encrypt()
        .key_id("arn:aws:kms:...")
        .plaintext(Blob::new(plaintext))
        .send()
        .await?;

    // Store encrypted blob
    tokio::fs::write("/secure/key_share.enc", encrypted.ciphertext_blob).await?;

    Ok(())
}
```

**Key Rotation** (Advanced):
```rust
// Proactive refresh protocol
// Generates new shares without changing public key
async fn refresh_key_shares(
    old_share: KeyShare,
    // ...
) -> Result<KeyShare> {
    let eid = ExecutionId::new(b"key_refresh_2024");

    let new_share = cggmp24::refresh(eid, my_index, n)
        .start(&mut rng, party, &old_share)
        .await?;

    // New share, same public key
    assert_eq!(new_share.shared_public_key(), old_share.shared_public_key());

    Ok(new_share)
}
```

### Nice-to-Have Improvements

#### 1. WebSocket Transport

```rust
use tokio_tungstenite::connect_async;

async fn websocket_transport(
    // ...
) -> Result<(impl Stream, impl Sink)> {
    let (ws_stream, _) = connect_async("ws://message-board:8080/ws").await?;

    let (write, read) = ws_stream.split();

    // Stream: Incoming messages
    let incoming = read.filter_map(|msg| {
        // Parse WebSocket message → Protocol message
    });

    // Sink: Outgoing messages
    let outgoing = write.with(|msg| {
        // Serialize Protocol message → WebSocket frame
    });

    Ok((incoming, outgoing))
}
```

**Benefits**:
- Lower latency (~100-200ms improvement)
- Reduced bandwidth (no polling overhead)
- Real-time updates

#### 2. Dynamic Threshold Configuration

```rust
// Support different thresholds
pub struct ThresholdConfig {
    pub n: u16,       // Total parties
    pub t: u16,       // Threshold
    pub participants: Vec<u16>,  // Dynamic selection
}

// Example: 5-of-7 scheme
let config = ThresholdConfig {
    n: 7,
    t: 5,
    participants: vec![0, 1, 2, 3, 4],  // Any 5 of 7
};
```

#### 3. Request Prioritization

```go
type Priority int

const (
    PriorityLow Priority = iota
    PriorityNormal
    PriorityHigh
    PriorityCritical
)

type SigningRequest struct {
    // ... existing fields
    Priority Priority `json:"priority"`
}

func (s *Store) GetPendingRequestsByPriority() []*SigningRequest {
    // Sort by priority DESC, then created_at ASC
    sort.Slice(requests, func(i, j int) bool {
        if requests[i].Priority != requests[j].Priority {
            return requests[i].Priority > requests[j].Priority
        }
        return requests[i].CreatedAt.Before(requests[j].CreatedAt)
    })
    return requests
}
```

#### 4. Metrics Dashboard

```bash
# Grafana dashboard
docker run -d -p 3000:3000 grafana/grafana

# Prometheus scraping
scrape_configs:
  - job_name: 'threshold_nodes'
    static_configs:
      - targets: ['node-1:9090', 'node-2:9090', 'node-3:9090', 'node-4:9090']

  - job_name: 'message_board'
    static_configs:
      - targets: ['message-board:9090']
```

**Key Metrics**:
- Signing latency (p50, p95, p99)
- Presignature pool utilization
- Request success rate
- Node uptime
- Message exchange rate

---

## Project Structure

```
threshold-signing/
│
├── api-gateway/                 # External REST API (Go)
│   ├── Dockerfile              # Multi-stage build
│   ├── go.mod                  # Dependencies
│   ├── main.go                 # HTTP server, endpoints (178 lines)
│   └── types.go                # Request/response structs (48 lines)
│
├── message-board/               # Central coordination (Go)
│   ├── Dockerfile              # Multi-stage build
│   ├── go.mod                  # Dependencies
│   ├── main.go                 # HTTP server, routing (528 lines)
│   ├── store.go                # In-memory data store (305 lines)
│   └── types.go                # Data structures (156 lines)
│
├── node/                        # Threshold signing node (Rust)
│   ├── Cargo.toml              # Dependencies, binary definitions
│   ├── Dockerfile              # Multi-stage build
│   └── src/
│       ├── main.rs             # Entry point, main loop (272 lines)
│       ├── keygen.rs           # Distributed key generation (72 lines)
│       ├── aux_info.rs         # Auxiliary info protocol (58 lines)
│       ├── signing.rs          # Full signing protocol (104 lines)
│       ├── signing_fast.rs     # Fast signing with presigs (248 lines)
│       ├── presignature.rs     # Presignature generation (122 lines)
│       ├── presignature_pool.rs # Thread-safe pool (124 lines)
│       ├── http_transport.rs   # CGGMP24 HTTP adapter (336 lines)
│       ├── message_board_client.rs # HTTP client (272 lines)
│       ├── utils.rs            # Utilities, data types (140 lines)
│       └── bin/
│           ├── verify_signature.rs    # Standalone verifier (158 lines)
│           ├── verify_from_api.rs     # E2E verification (124 lines)
│           └── export_pubkey.rs       # Public key export (56 lines)
│
├── scripts/                     # Testing and deployment scripts
│   ├── start.sh                # Start containers
│   ├── stop.sh                 # Stop containers
│   ├── logs.sh                 # View logs
│   ├── demo.sh                 # Interactive demo (263 lines)
│   ├── test-message-board.sh   # Unit tests for MessageBoard (243 lines)
│   ├── test-api-gateway.sh     # Integration tests for API (132 lines)
│   ├── test-e2e.sh             # End-to-end signing test (173 lines)
│   └── test-sign-and-verify.sh # Signing + verification (89 lines)
│
├── podman-compose.yml           # Container orchestration (100 lines)
├── .gitignore                  # Git ignore rules
└── README.md                    # This file

Total Lines of Code (excluding comments):
- Rust: ~2,086 lines
- Go: ~1,215 lines
- Shell: ~900 lines
- Total: ~4,200 lines
```

### Key Files Explained

| File | Purpose | Important Functions/Sections |
|------|---------|------------------------------|
| **node/src/main.rs** | Node entry point | `main()`: System initialization<br>`maintain_presignature_pool()`: Background task<br>Main signing loop: Line 146-250 |
| **node/src/http_transport.rs** | Protocol adapter | `http_transport()`: Creates Stream/Sink<br>`poll_messages()`: Background receiver<br>`post_messages()`: Background sender |
| **node/src/signing_fast.rs** | Fast signing | `sign_message_fast()`: Main fast path<br>`collect_partial_signatures()`: Polling collector |
| **node/src/presignature_pool.rs** | Pool management | `take()`: Remove presignature<br>`add()`: Add presignature<br>`needs_refill()`: Check size |
| **message-board/main.go** | MessageBoard server | `handleCreateRequest()`: Line 65-86<br>`handlePostMessage()`: Line 187-217<br>`handleGetMessages()`: Line 219-267 |
| **message-board/store.go** | Data storage | `CreateRequest()`: Line 78-93<br>`GetMessages()`: Line 176-197<br>`SetPublicKey()`: Line 270-285 |
| **api-gateway/main.go** | External API | `handleSign()`: Line 71-107<br>`handleStatus()`: Line 109-147<br>`handlePublicKey()`: Line 149-178 |

---

## Troubleshooting

### Common Issues

#### 1. System doesn't start

**Symptom**:
```bash
podman-compose up -d
# Containers exit immediately
```

**Diagnosis**:
```bash
podman-compose logs
# Check for error messages
```

**Common Causes**:
- Port conflict (8000 already in use)
  ```bash
  # Solution: Stop conflicting service
  sudo lsof -i :8000
  sudo kill -9 <PID>
  ```

- Build failure
  ```bash
  # Solution: Rebuild
  podman-compose build --no-cache
  ```

#### 2. Keygen takes too long (>2 minutes)

**Symptom**:
```bash
./scripts/logs.sh
# "Generating safe primes..." stuck for >2 minutes
```

**Cause**: CPU-intensive prime generation

**Solutions**:
- **Wait**: Normal on slower machines (up to 5 minutes)
- **Pre-generate**: Use faster machine for initial keygen
- **Docker resources**: Increase CPU allocation

#### 3. Signing requests timeout

**Symptom**:
```bash
curl http://localhost:8000/status/req_123
# {"status": "pending"} forever
```

**Diagnosis**:
```bash
./scripts/logs.sh | grep "req_123"
# Check if nodes are processing
```

**Common Causes**:
- Presignature pool empty + full protocol failure
  ```bash
  # Check pool size in logs
  grep "pool size" logs
  ```

- Node not participant (expected for node-4)
  ```bash
  # Check participant calculation
  grep "is_participant" logs
  ```

- MessageBoard unreachable
  ```bash
  # Test connectivity
  podman exec node-1 curl http://message-board:8080/health
  ```

#### 4. Signature verification fails

**Symptom**:
```bash
cargo run --bin verify-signature -- ...
# Error: Signature verification failed
```

**Causes**:
- Wrong public key
  ```bash
  # Ensure using correct public key
  curl http://localhost:8000/publickey
  ```

- Wrong message
  ```bash
  # Message must be EXACT (case-sensitive, whitespace)
  ```

- Corrupted signature
  ```bash
  # Check signature JSON structure
  cat signature.json | jq .
  ```

#### 5. Presignature pool not refilling

**Symptom**:
```bash
# All fast signatures fail
grep "No presignature available" logs
```

**Diagnosis**:
```bash
# Check node-0 (leader) logs
podman logs node-1 | grep "presignature"
```

**Solutions**:
- Node-0 crashed: Restart it
  ```bash
  podman restart node-1
  ```

- Background task stuck: Check for errors
  ```bash
  podman logs node-1 | grep "maintain_presignature_pool"
  ```

### Debug Commands

```bash
# Check all container status
podman ps -a

# Check specific node logs
podman logs node-1 --tail 100 --follow

# Enter node container
podman exec -it node-1 bash

# Check MessageBoard data
curl http://localhost:8080/requests | jq .
curl http://localhost:8080/messages | jq .

# Check network connectivity
podman exec node-1 ping message-board

# Check file permissions
podman exec node-1 ls -la /tmp/key_share_*

# Manual signing test
curl -X POST http://localhost:8000/sign \
  -d '{"message": "debug test"}' \
  -H "Content-Type: application/json" \
  -v  # Verbose output
```

---

## FAQ

### Q: Neden 3-of-4 threshold? Neden 2-of-3 veya 5-of-7 değil?

**A**: Demo için optimal denge:
- **2-of-3**: Daha basit ama daha az fault-tolerant (sadece 1 node kaybına dayanır)
- **3-of-4**: İyi denge (1 node failure tolere eder, 4 node yönetimi kolay)
- **5-of-7**: Daha güvenli ama daha karmaşık (deployment, koordinasyon)

Kod değiştirilerek farklı threshold'lar desteklenebilir:
```rust
// src/keygen.rs
let key_share = cggmp24::keygen::<Secp256k1>(eid, my_index, n=7)
    .set_threshold(Some(t=5))  // 5-of-7
    .start(&mut rng, party)
    .await?;
```

### Q: Presignature pool neden gerekli? Tam protokol her zaman kullanılamaz mı?

**A**: Kullanılabilir ama **çok yavaş**:
- **Tam protokol**: ~4-6 saniye (network rounds gerekli)
- **Fast path**: ~100-500ms (10-50x hız artışı)

Production sistemlerde latency kritik:
- Payment processing: <500ms hedefi
- Trading: <100ms hedefi
- User-facing apps: <1s tolere edilebilir

Presignature pool bu latency gereksinimlerini karşılar.

### Q: Private key hiçbir zaman reconstruct edilemiyor mu?

**A**: Doğru, **asla**. Bu threshold signing'in ana faydası:

**Traditional multi-sig (Bitcoin)**:
```
┌─────────┐
│ Key 1   │  Complete private key
│ Key 2   │  Complete private key
│ Key 3   │  Complete private key
└─────────┘
Any 2 can sign independently
⚠️ Each key is a single point of failure
```

**Threshold ECDSA (bu sistem)**:
```
┌─────────┐
│ Share 1 │  Useless alone
│ Share 2 │  Useless alone
│ Share 3 │  Useless alone
│ Share 4 │  Useless alone
└─────────┘
Need 3 shares to sign (never combined!)
✓ No single point of failure
```

Matematiksel olarak:
- Private key `d` Shamir Secret Sharing ile split edilir
- Her node share `d_i` alır
- `d = d_1 + d_2 + d_3` (simplification)
- Ancak bu toplama **hiçbir zaman yapılmaz**
- İmza işlemi distributed olarak çalışır: `sig = f(d_1, d_2, d_3)` ama `d` oluşturmaz

### Q: Node-4 neden hiç participate etmiyor?

**A**: Participant selection deterministik ama rotasyonel değil (demo simplification):

```rust
// src/utils.rs:67-76
pub fn is_participant(request_id: &str, my_index: u16, t: u16) -> bool {
    let hash = blake2b_simd::blake2b(request_id.as_bytes());
    let bytes = hash.as_bytes();

    // First t bytes as participant indices
    for i in 0..t {
        if bytes[i as usize] as u16 % 4 == my_index {
            return true;
        }
    }
    false
}
```

Mevcut implementasyon genellikle **ilk 3 node**'u seçiyor (hash collision nedeniyle).

**Production fix**: Round-robin veya explicit participant selection:
```rust
pub fn select_participants_round_robin(
    request_id: &str,
    n: u16,
    t: u16,
) -> Vec<u16> {
    let hash = blake2b_simd::blake2b(request_id.as_bytes());
    let offset = u64::from_le_bytes(hash.as_bytes()[0..8].try_into().unwrap());

    let mut participants = Vec::new();
    for i in 0..t {
        participants.push(((offset + i as u64) % n as u64) as u16);
    }
    participants
}
```

### Q: MessageBoard çökerse sistem ne olur?

**A**: Mevcut implementasyonda **sistem durur** çünkü:
- In-memory storage → restart = data loss
- Central coordination → single point of failure

**Production çözümleri**:

1. **High Availability**:
   ```yaml
   # 3 MessageBoard replicas + shared database
   replicas: 3
   database: postgresql://...
   ```

2. **Peer-to-Peer Mode** (gelişmiş):
   ```rust
   // Direct P2P communication without MessageBoard
   let peer_addresses = vec![
       "node-1:9000",
       "node-2:9000",
       "node-3:9000",
   ];

   let transport = p2p_transport(my_index, peer_addresses).await?;
   ```

3. **Blockchain-based Coordination** (en decentralized):
   ```rust
   // Post messages to smart contract
   let transport = blockchain_transport(
       contract_address,
       my_wallet,
   ).await?;
   ```

### Q: Bu sistem production-ready mi?

**A**: **Hayır**, bu bir **demo/proof-of-concept**. Production için gerekli:

**Critical**:
- [ ] Persistent storage (database)
- [ ] Authentication (mTLS, API keys)
- [ ] High availability (replication)
- [ ] Monitoring (metrics, logs, tracing)
- [ ] Rate limiting
- [ ] Key management (HSM, backup, rotation)

**Important**:
- [ ] WebSocket transport (lower latency)
- [ ] Dynamic threshold configuration
- [ ] Request prioritization
- [ ] Automatic failover
- [ ] Audit logging

**Nice-to-have**:
- [ ] Metrics dashboard
- [ ] Admin UI
- [ ] Multi-tenancy
- [ ] Geographic distribution

Ancak **cryptographic core** (CGGMP24) production-grade. Sadece infrastructure layer eksik.

---

## Contributing

Bu bir demo projedir ancak katkılar memnuniyetle karşılanır!

**İlgi alanları**:
- Performance optimizations
- Production hardening
- Additional tests
- Documentation improvements
- Security audits

---

## License

[Projenizin lisansı buraya]

---

## References

### Papers

- **CGGMP24**: Canetti et al. "UC Non-Interactive, Proactive, Threshold ECDSA" (2024)
- **GG20**: Gennaro & Goldfeder. "Fast Multiparty Threshold ECDSA" (2020)
- **Lindell17**: Lindell. "Fast Secure Two-Party ECDSA Signing" (2017)

### Libraries

- **cggmp24**: https://github.com/webb-tools/cggmp-threshold-ecdsa
- **secp256k1**: Bitcoin/Ethereum curve
- **Tokio**: Async runtime for Rust

### Tools
- **Podman/Docker**: Container runtime
- **Golang**: API Gateway & MessageBoard
- **Rust**: Cryptographic node implementation
---
## Contact

[İletişim bilgileriniz buraya]

---

**Son güncelleme**: 2024-01-12
**Versiyon**: 1.0.0
**Maintainer**: [Adınız]
