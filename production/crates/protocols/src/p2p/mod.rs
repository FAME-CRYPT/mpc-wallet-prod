//! P2P messaging module for direct peer-to-peer communication.
//!
//! This module provides alternatives to the HTTP relay transport,
//! enabling nodes to communicate directly with each other.
//!
//! ## Transport Options
//!
//! 1. **TCP Transport** (P2pTransport): Simple TCP connections
//! 2. **QUIC Transport** (QuicTransport): Secure QUIC with TLS 1.3 and CA-signed certificates
//!
//! ## Architecture (QUIC - Recommended)
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                        Node                                  │
//! │  ┌─────────────┐    ┌──────────────┐    ┌────────────────┐  │
//! │  │  Protocol   │───>│QuicTransport │───>│ QUIC Connection│──┼──> Peers
//! │  │             │<───│   (TLS 1.3)  │<───│    Manager     │  │
//! │  └─────────────┘    └──────────────┘    └────────────────┘  │
//! │                            │                    ↑            │
//! │                    ┌───────▼────────┐          │            │
//! │                    │ Incoming Queue │   ┌──────┴──────┐     │
//! │                    │  (per-session) │<──│QuicListener │     │
//! │                    └────────────────┘   │(mTLS auth)  │     │
//! │                                         └─────────────┘     │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Security Model (QUIC)
//!
//! - All connections use TLS 1.3 (mandatory in QUIC)
//! - Mutual TLS: both client and server present CA-signed certificates
//! - Each node's certificate contains its party index for identification
//! - Certificate hierarchy: Root CA -> Node Certificates
//!
//! ## Components
//!
//! ### TCP (Legacy)
//! - **P2pTransport**: Implements `Transport` trait for TCP P2P messaging
//! - **ConnectionManager**: Manages TCP connections to peer nodes
//! - **Receiver**: Background task that accepts incoming TCP connections
//!
//! ### QUIC (Recommended)
//! - **QuicTransport**: Implements `Transport` trait with TLS 1.3
//! - **QuicConnectionManager**: Manages QUIC connections with mTLS
//! - **QuicListener**: Accepts incoming QUIC connections
//! - **Certificates**: CA and node certificate management
//!
//! ## Usage (QUIC - Recommended)
//!
//! ```rust,ignore
//! use protocols::p2p::{QuicTransport, QuicTransportBuilder};
//! use protocols::p2p::certs::{CaCertificate, NodeCertificate};
//!
//! // Load or generate certificates
//! let ca = CaCertificate::generate()?;
//! let node_cert = ca.sign_node_cert(party_index, &["mpc-node-1".to_string()])?;
//!
//! // Create QUIC transport
//! let transport = QuicTransportBuilder::new(party_index, "http://mpc-coordinator:3000")
//!     .with_cert(node_cert)
//!     .with_listen_addr("0.0.0.0:4001".parse()?)
//!     .build()?;
//!
//! // Initialize (starts listener)
//! transport.init().await?;
//!
//! // Refresh peer list from registry
//! transport.refresh_peers().await?;
//!
//! // Use transport for protocol execution
//! transport.send_message(msg).await?;
//! let response = transport.poll_messages(&session_id).await?;
//!
//! // Shutdown when done
//! transport.shutdown().await;
//! ```
//!
//! ## Usage (TCP - Legacy)
//!
//! ```rust,ignore
//! use protocols::p2p::{P2pTransport, receiver, new_incoming_queue};
//!
//! // Create incoming message queue
//! let incoming_queue = new_incoming_queue();
//!
//! // Spawn receiver to listen for incoming messages
//! let listen_addr = "0.0.0.0:4000".parse().unwrap();
//! let _receiver = receiver::spawn_receiver(listen_addr, party_index, incoming_queue.clone())?;
//!
//! // Create P2P transport
//! let transport = P2pTransport::new(
//!     party_index,
//!     "http://mpc-coordinator:3000",
//!     incoming_queue,
//!     Some(4000),
//! );
//!
//! // Refresh peer list and use transport
//! transport.refresh_peers().await?;
//! transport.send_message(msg).await?;
//! ```

// TCP transport modules
pub mod connection;
pub mod error;
pub mod protocol;
pub mod receiver;
pub mod transport;

// QUIC transport modules (secure, recommended)
pub mod certs;
pub mod quic;
pub mod quic_listener;
pub mod quic_transport;

// P2P session coordination (Phase 4)
pub mod coordinator;
pub mod session;

// Re-export TCP types (legacy)
pub use connection::{ConnectionManager, PeerInfo, DEFAULT_P2P_PORT};
pub use error::P2pError;
pub use protocol::{P2pMessage, P2pMessageType};
pub use receiver::{drain_session_messages, new_incoming_queue, spawn_receiver, IncomingQueue};
pub use transport::P2pTransport;

// Re-export QUIC types (recommended)
pub use certs::{CaCertificate, NodeCertificate, StoredCaCert, StoredNodeCert};
pub use quic::{QuicConnectionManager, DEFAULT_QUIC_PORT};
pub use quic_listener::{spawn_quic_listener, QuicListener};
pub use quic_transport::{QuicTransport, QuicTransportBuilder};

// Re-export session control types (Phase 4)
pub use coordinator::{CoordinatorError, P2pSessionCoordinator, SessionHandle};
pub use session::{
    P2pSessionStatus, SessionAbort, SessionAck, SessionControlMessage, SessionProposal,
    SessionStart,
};
