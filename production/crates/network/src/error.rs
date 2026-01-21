//! Network error types for the threshold network layer.

use std::io;
use thiserror::Error;
use threshold_types::NodeId;

/// Errors that can occur during network operations.
#[derive(Debug, Error)]
pub enum NetworkError {
    /// Failed to connect to peer.
    #[error("failed to connect to peer {node_id}: {source}")]
    ConnectionFailed {
        node_id: NodeId,
        #[source]
        source: io::Error,
    },

    /// Connection to peer was lost.
    #[error("connection to peer {node_id} lost: {reason}")]
    ConnectionLost { node_id: NodeId, reason: String },

    /// Failed to send message.
    #[error("failed to send message to peer {node_id}: {source}")]
    SendFailed {
        node_id: NodeId,
        #[source]
        source: io::Error,
    },

    /// Failed to receive message.
    #[error("failed to receive message: {0}")]
    ReceiveFailed(#[from] io::Error),

    /// Message encoding/decoding error.
    #[error("message codec error: {0}")]
    CodecError(String),

    /// Peer not found in registry.
    #[error("peer {node_id} not found in registry")]
    PeerNotFound { node_id: NodeId },

    /// Failed to fetch peer list from registry.
    #[error("failed to fetch peer list: {0}")]
    RegistryError(String),

    /// Invalid peer address.
    #[error("invalid peer address for node {node_id}: {address}")]
    InvalidAddress { node_id: NodeId, address: String },

    /// Session not found.
    #[error("session {session_id} not found")]
    SessionNotFound { session_id: String },

    /// Listener bind failed.
    #[error("failed to bind listener on {address}: {source}")]
    BindFailed {
        address: String,
        #[source]
        source: io::Error,
    },

    /// Channel closed unexpectedly.
    #[error("internal channel closed")]
    ChannelClosed,

    /// Timeout waiting for operation.
    #[error("operation timed out: {0}")]
    Timeout(String),

    /// Certificate error.
    #[error("certificate error: {0}")]
    CertificateError(String),

    /// TLS configuration error.
    #[error("TLS configuration error: {0}")]
    TlsConfigError(String),

    /// Connection pool exhausted.
    #[error("connection pool exhausted for node {node_id}")]
    PoolExhausted { node_id: NodeId },

    /// Invalid stream ID.
    #[error("invalid stream ID {0}: {1}")]
    InvalidStreamId(u64, String),
}

/// Result type for network operations.
pub type NetworkResult<T> = Result<T, NetworkError>;
