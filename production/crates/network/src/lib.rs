//! Threshold Network Layer
//!
//! This crate provides the QUIC-based network layer for the threshold signature system.
//! It implements secure peer-to-peer communication with mTLS authentication and
//! connection pooling.
//!
//! ## Features
//!
//! - **QUIC Transport**: Fast, secure, multiplexed connections using QUIC protocol
//! - **mTLS Authentication**: Mutual TLS authentication using certificates from cert_manager
//! - **Connection Pooling**: Up to 50 concurrent connections per peer for high throughput
//! - **Stream ID Conventions**: Organized stream IDs for different protocol phases
//!   - 0-99: Control messages
//!   - 100-999: DKG rounds
//!   - 1000-9999: Signing rounds
//!   - 10000+: Presignature generation
//! - **Peer Discovery**: Dynamic peer discovery via registry service
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                     Network Layer                            │
//! │                                                              │
//! │  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐  │
//! │  │ QuicEngine   │───>│ ConnectionPool│───>│  Peer Node   │  │
//! │  │  (send/recv) │    │ (max 50/peer)│    │  (mTLS auth) │  │
//! │  └──────────────┘    └──────────────┘    └──────────────┘  │
//! │         │                                                    │
//! │         ▼                                                    │
//! │  ┌──────────────┐    ┌──────────────┐                      │
//! │  │QuicListener  │───>│IncomingQueue │                      │
//! │  │ (accept conn)│    │ (per-session)│                      │
//! │  └──────────────┘    └──────────────┘                      │
//! │                                                              │
//! │  ┌──────────────────────────────────────┐                  │
//! │  │      PeerRegistry (discovery)        │                  │
//! │  └──────────────────────────────────────┘                  │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use threshold_network::{QuicEngine, PeerRegistry, QuicListener, MtlsConfig};
//! use threshold_types::NodeId;
//!
//! // Create peer registry with optional registry service
//! let peer_registry = Arc::new(PeerRegistry::new(
//!     NodeId(0),
//!     Some("http://registry:3000".to_string()),
//! ));
//!
//! // TODO: Load mTLS configuration from cert_manager
//! let mtls_config = None; // Will be Some(MtlsConfig) after cert_manager integration
//!
//! // Create QUIC engine
//! let mut engine = QuicEngine::new(NodeId(0), peer_registry.clone(), mtls_config);
//! engine.init_client().await?;
//!
//! // Start listener for incoming connections
//! let incoming_queue = new_incoming_queue();
//! let server_config = engine.server_config()?;
//! let listener = spawn_quic_listener(
//!     "0.0.0.0:4001".parse()?,
//!     server_config,
//!     incoming_queue.clone(),
//! )?;
//!
//! // Send messages using appropriate stream IDs
//! use threshold_network::quic_session::stream_id;
//! let message = NetworkMessage::DkgRound(DkgMessage { /* ... */ });
//! engine.send(&NodeId(1), &message, stream_id::dkg_round(0)).await?;
//!
//! // Receive messages from queue
//! let messages = drain_session_messages(&incoming_queue, "session-id").await;
//! ```
//!
//! ## Stream ID Conventions
//!
//! Messages are sent on specific stream IDs based on their protocol phase:
//!
//! ```rust,ignore
//! use threshold_network::quic_session::stream_id;
//!
//! // Control messages (session setup, heartbeats, etc.)
//! let stream_id = stream_id::control(0);  // Returns 0-99
//!
//! // DKG protocol rounds
//! let stream_id = stream_id::dkg_round(1);  // Returns 100-999
//!
//! // Signing protocol rounds
//! let stream_id = stream_id::signing_round(1);  // Returns 1000-9999
//!
//! // Presignature generation
//! let stream_id = stream_id::presignature(0);  // Returns 10000+
//! ```
//!
//! ## Connection Pooling
//!
//! The QUIC engine maintains up to 50 concurrent connections per peer for high
//! throughput operations. Connections are automatically pooled and reused.
//!
//! ## mTLS Integration
//!
//! **Note**: This crate is ready for mTLS certificate integration from the
//! `cert_manager` crate. Currently, it uses temporary self-signed certificates
//! for development only. Once cert_manager is integrated:
//!
//! 1. Load certificates from cert_manager
//! 2. Create `MtlsConfig` with client and server rustls configurations
//! 3. Pass to `QuicEngine::new()`
//! 4. Remove the temporary certificate generation code
//!
//! ```rust,ignore
//! // After cert_manager integration:
//! let cert_config = cert_manager::load_node_certificate(node_id)?;
//! let mtls_config = MtlsConfig {
//!     client_config: cert_config.client_config()?,
//!     server_config: cert_config.server_config()?,
//! };
//! let engine = QuicEngine::new(node_id, peer_registry, Some(mtls_config));
//! ```

pub mod error;
pub mod peer_registry;
pub mod quic_engine;
pub mod quic_listener;
pub mod quic_session;

// Re-export commonly used types
pub use error::{NetworkError, NetworkResult};
pub use peer_registry::{PeerInfo, PeerRegistry, DEFAULT_QUIC_PORT};
pub use quic_engine::{MtlsConfig, QuicEngine};
pub use quic_listener::{
    drain_session_messages, new_incoming_queue, spawn_quic_listener, IncomingQueue, QuicListener,
};
pub use quic_session::{
    stream_id, SessionAbort, SessionAck, SessionControlMessage, SessionProposal, SessionStart,
    SessionStatus, validate_stream_id,
};
