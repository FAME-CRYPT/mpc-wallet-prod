//! MPC protocol implementations.
//!
//! This crate provides threshold signature protocols:
//! - CGGMP24: Production-grade threshold ECDSA
//! - FROST: Threshold Schnorr signatures for Taproot
//!
//! Each protocol includes:
//! - Key generation (distributed)
//! - Threshold signing
//! - Supporting infrastructure (message relay, channel adapters)
//!
//! ## Transport Abstraction
//!
//! The `transport` module provides a transport-agnostic interface for message
//! passing. Two transports are available:
//! - HTTP relay via coordinator (default)
//! - P2P direct messaging between nodes
//!
//! ## P2P Messaging
//!
//! The `p2p` module provides direct peer-to-peer messaging, eliminating the
//! coordinator relay bottleneck. Nodes communicate directly over TCP.

pub mod bench;
pub mod cggmp24;
pub mod frost;
pub mod p2p;
pub mod relay;
pub mod transport;

// Re-export commonly used types
pub use bench::{BenchmarkRecorder, BenchmarkReport, StepTiming};
pub use cggmp24::{
    generate_presignature, sign_message_fast, AuxInfoGenResult, AuxInfoStatus, KeygenResult,
    PoolStats, PresignaturePool, PresignaturePoolHandle, PresignatureResult, SignatureData,
    SigningResult, StoredAuxInfo, StoredKeyShare, StoredPresignature, StoredPrimes,
};
pub use frost::{FrostKeyShare, FrostKeygenResult, FrostSigningResult, SchnorrSignature};
pub use relay::{RelayClient, RelayMessage, SessionMessageQueue};
pub use transport::{
    create_transport, HttpTransport, SharedTransport, Transport, TransportConfig, TransportError,
    TransportType,
};

// Re-export P2P types
pub use p2p::{
    new_incoming_queue, spawn_receiver, ConnectionManager, IncomingQueue, P2pError, P2pMessage,
    P2pTransport, DEFAULT_P2P_PORT,
};
