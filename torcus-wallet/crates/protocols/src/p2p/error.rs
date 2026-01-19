//! P2P-specific error types.

use std::io;
use thiserror::Error;

/// Errors that can occur during P2P operations.
#[derive(Debug, Error)]
pub enum P2pError {
    /// Failed to connect to peer.
    #[error("failed to connect to peer {party_index}: {source}")]
    ConnectionFailed {
        party_index: u16,
        #[source]
        source: io::Error,
    },

    /// Connection to peer was lost.
    #[error("connection to peer {party_index} lost: {reason}")]
    ConnectionLost { party_index: u16, reason: String },

    /// Failed to send message.
    #[error("failed to send message to peer {party_index}: {source}")]
    SendFailed {
        party_index: u16,
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
    #[error("peer {party_index} not found in registry")]
    PeerNotFound { party_index: u16 },

    /// Failed to fetch peer list from registry.
    #[error("failed to fetch peer list: {0}")]
    RegistryError(String),

    /// Invalid peer address.
    #[error("invalid peer address for party {party_index}: {address}")]
    InvalidAddress { party_index: u16, address: String },

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
}

impl From<P2pError> for crate::transport::TransportError {
    fn from(err: P2pError) -> Self {
        crate::transport::TransportError::Other(err.to_string())
    }
}
