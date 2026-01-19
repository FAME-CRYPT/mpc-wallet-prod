//! Protocol messages for coordinator-node communication.
//!
//! The DKG protocol works in rounds. The coordinator orchestrates message
//! passing between nodes by:
//! 1. Collecting messages from all nodes for a round
//! 2. Distributing messages to appropriate recipients
//! 3. Repeating until protocol completion

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::WalletType;

/// A protocol message exchanged between parties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolMessage {
    /// Session this message belongs to.
    pub session_id: Uuid,
    /// Sender's party index (0-based).
    pub sender: u16,
    /// Recipient's party index (None = broadcast to all).
    pub recipient: Option<u16>,
    /// Whether this is a broadcast message.
    pub is_broadcast: bool,
    /// The serialized message payload.
    pub payload: Vec<u8>,
}

/// Request to start a new DKG session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartDkgRequest {
    pub session_id: Uuid,
    pub wallet_id: Uuid,
    pub wallet_type: WalletType,
    /// This node's party index (0-based).
    pub party_index: u16,
    /// Total number of parties.
    pub n_parties: u16,
    /// Threshold for signing.
    pub threshold: u16,
}

/// Response from starting DKG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartDkgResponse {
    pub session_id: Uuid,
    pub party_index: u16,
    /// Initial outgoing messages (round 1).
    pub outgoing_messages: Vec<ProtocolMessage>,
    pub status: String,
}

/// Request to process a DKG round.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessRoundRequest {
    pub session_id: Uuid,
    pub wallet_id: Uuid,
    /// Current round number (1-based).
    pub round: u16,
    /// Incoming messages for this party.
    pub incoming_messages: Vec<ProtocolMessage>,
}

/// Response from processing a round.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessRoundResponse {
    pub session_id: Uuid,
    pub party_index: u16,
    pub round: u16,
    /// Outgoing messages for other parties.
    pub outgoing_messages: Vec<ProtocolMessage>,
    /// Whether the protocol is complete.
    pub is_complete: bool,
    /// The shared public key (only set when complete).
    pub public_key: Option<String>,
    pub status: String,
}

/// Simple keygen request (legacy, for mock implementation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeygenRequest {
    pub session_id: Uuid,
    pub wallet_id: Uuid,
    pub wallet_type: WalletType,
}

/// Response from keygen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeygenResponse {
    pub session_id: Uuid,
    pub wallet_id: Uuid,
    pub party_index: u16,
    pub public_key: String,
    pub status: String,
}

// ============================================================================
// Signing Protocol Messages
// ============================================================================

/// Request to start a signing session on a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartSigningRequest {
    pub session_id: Uuid,
    pub wallet_id: Uuid,
    /// Message hash to sign (32 bytes, hex encoded).
    pub message_hash: String,
    /// Indices of all participants.
    pub participant_indices: Vec<u16>,
}

/// Response from starting a signing session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartSigningResponse {
    pub session_id: Uuid,
    pub party_index: u16,
    /// Round 1 outgoing messages.
    pub outgoing_messages: Vec<ProtocolMessage>,
    pub status: String,
}

/// Request to process a signing round.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSigningRoundRequest {
    pub session_id: Uuid,
    pub wallet_id: Uuid,
    pub round: u16,
    /// Incoming messages for this party.
    pub incoming_messages: Vec<ProtocolMessage>,
}

/// Response from processing a signing round.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessSigningRoundResponse {
    pub session_id: Uuid,
    pub party_index: u16,
    pub round: u16,
    /// Outgoing messages for other parties.
    pub outgoing_messages: Vec<ProtocolMessage>,
    /// Whether signing is complete.
    pub is_complete: bool,
    /// Final signature (only set when complete). Contains r and s as hex.
    pub signature: Option<SignatureData>,
    pub status: String,
}

/// ECDSA signature data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureData {
    /// r component (hex encoded, 32 bytes)
    pub r: String,
    /// s component (hex encoded, 32 bytes)
    pub s: String,
}

/// Legacy sign request (simple, non-multi-round).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignRequest {
    pub session_id: Uuid,
    pub wallet_id: Uuid,
    /// Message hash to sign (32 bytes, hex encoded).
    pub message_hash: String,
    /// Indices of all participants (for Lagrange coefficients).
    pub participant_indices: Vec<u16>,
}

/// Legacy sign response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignResponse {
    pub session_id: Uuid,
    pub party_index: u16,
    /// Partial signature (Lagrange-weighted), hex encoded.
    pub partial_signature: String,
    /// This party's public key share (for verification).
    pub public_key_share: String,
    pub status: String,
}
