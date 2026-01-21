//! QUIC session management with stream ID conventions.
//!
//! This module implements the stream ID convention for different protocol phases:
//! - 0-99: Control messages
//! - 100-999: DKG rounds
//! - 1000-9999: Signing rounds
//! - 10000+: Presignature generation

use serde::{Deserialize, Serialize};
use threshold_types::NodeId;
use crate::error::{NetworkError, NetworkResult};

/// Stream ID ranges for different message types.
pub mod stream_id {
    /// Control message stream IDs (0-99).
    pub const CONTROL_MIN: u64 = 0;
    pub const CONTROL_MAX: u64 = 99;

    /// DKG round stream IDs (100-999).
    pub const DKG_MIN: u64 = 100;
    pub const DKG_MAX: u64 = 999;

    /// Signing round stream IDs (1000-9999).
    pub const SIGNING_MIN: u64 = 1000;
    pub const SIGNING_MAX: u64 = 9999;

    /// Presignature generation stream IDs (10000+).
    pub const PRESIGNATURE_MIN: u64 = 10000;

    /// Get the base stream ID for a control message.
    pub fn control(seq: u64) -> u64 {
        CONTROL_MIN + (seq % (CONTROL_MAX - CONTROL_MIN + 1))
    }

    /// Get the base stream ID for a DKG round.
    pub fn dkg_round(round: u32) -> u64 {
        DKG_MIN + (round as u64 % (DKG_MAX - DKG_MIN + 1))
    }

    /// Get the base stream ID for a signing round.
    pub fn signing_round(round: u32) -> u64 {
        SIGNING_MIN + (round as u64 % (SIGNING_MAX - SIGNING_MIN + 1))
    }

    /// Get the base stream ID for presignature generation.
    pub fn presignature(seq: u64) -> u64 {
        PRESIGNATURE_MIN + seq
    }
}

/// Session control message types for P2P coordination.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionControlMessage {
    /// Initiator proposes a session to participants.
    Proposal(SessionProposal),
    /// Participant acknowledges a session proposal.
    Ack(SessionAck),
    /// Initiator signals that the session can start.
    Start(SessionStart),
    /// Any party signals session abort.
    Abort(SessionAbort),
}

impl SessionControlMessage {
    /// Get the session ID from the control message.
    pub fn session_id(&self) -> Option<String> {
        match self {
            SessionControlMessage::Proposal(p) => Some(p.session_id.clone()),
            SessionControlMessage::Ack(a) => Some(a.session_id.clone()),
            SessionControlMessage::Start(s) => Some(s.session_id.clone()),
            SessionControlMessage::Abort(a) => Some(a.session_id.clone()),
        }
    }
}

/// Session proposal sent by initiator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionProposal {
    /// Session identifier.
    pub session_id: String,
    /// Node ID of the initiator.
    pub initiator_node: NodeId,
    /// Protocol type (e.g., "dkg", "signing", "presignature").
    pub protocol: String,
    /// Unix timestamp when proposal was created.
    pub timestamp: u64,
    /// Additional session parameters.
    pub params: serde_json::Value,
}

impl SessionProposal {
    /// Create a new session proposal.
    pub fn new(
        session_id: String,
        initiator_node: NodeId,
        protocol: &str,
        params: serde_json::Value,
    ) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            session_id,
            initiator_node,
            protocol: protocol.to_string(),
            timestamp,
            params,
        }
    }
}

/// Session acknowledgment from a participant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionAck {
    /// Session ID being acknowledged.
    pub session_id: String,
    /// Node ID of the acknowledging node.
    pub node_id: NodeId,
    /// Whether the node accepts the proposal.
    pub accepted: bool,
    /// Error message if rejected.
    pub error: Option<String>,
    /// Unix timestamp when ack was created.
    pub timestamp: u64,
}

impl SessionAck {
    /// Create an accepting ack.
    pub fn accept(session_id: String, node_id: NodeId) -> Self {
        Self {
            session_id,
            node_id,
            accepted: true,
            error: None,
            timestamp: now(),
        }
    }

    /// Create a rejecting ack.
    pub fn reject(session_id: String, node_id: NodeId, reason: String) -> Self {
        Self {
            session_id,
            node_id,
            accepted: false,
            error: Some(reason),
            timestamp: now(),
        }
    }
}

/// Session start signal from initiator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStart {
    /// Session ID being started.
    pub session_id: String,
    /// Confirmed participants who acknowledged.
    pub participants: Vec<NodeId>,
    /// Unix timestamp when start was signaled.
    pub timestamp: u64,
}

impl SessionStart {
    /// Create a new session start message.
    pub fn new(session_id: String, participants: Vec<NodeId>) -> Self {
        Self {
            session_id,
            participants,
            timestamp: now(),
        }
    }
}

/// Session abort signal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionAbort {
    /// Session ID to abort.
    pub session_id: String,
    /// Node ID of the node sending abort.
    pub sender: NodeId,
    /// Reason for abort.
    pub reason: String,
    /// Unix timestamp when abort was signaled.
    pub timestamp: u64,
}

impl SessionAbort {
    /// Create a new session abort message.
    pub fn new(session_id: String, sender: NodeId, reason: String) -> Self {
        Self {
            session_id,
            sender,
            reason,
            timestamp: now(),
        }
    }
}

/// Session status for tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Proposal sent, waiting for acks.
    Proposed,
    /// Threshold acks received, session starting.
    Starting,
    /// Protocol execution in progress.
    InProgress,
    /// Session completed successfully.
    Completed,
    /// Session aborted or failed.
    Aborted,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Proposed => write!(f, "proposed"),
            Self::Starting => write!(f, "starting"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Completed => write!(f, "completed"),
            Self::Aborted => write!(f, "aborted"),
        }
    }
}

/// Validate a stream ID for a given message type.
pub fn validate_stream_id(stream_id: u64, message_type: &str) -> NetworkResult<()> {
    match message_type {
        "control" => {
            if stream_id < stream_id::CONTROL_MIN || stream_id > stream_id::CONTROL_MAX {
                return Err(NetworkError::InvalidStreamId(
                    stream_id,
                    format!("not in control range ({}-{})", stream_id::CONTROL_MIN, stream_id::CONTROL_MAX),
                ));
            }
        }
        "dkg" => {
            if stream_id < stream_id::DKG_MIN || stream_id > stream_id::DKG_MAX {
                return Err(NetworkError::InvalidStreamId(
                    stream_id,
                    format!("not in DKG range ({}-{})", stream_id::DKG_MIN, stream_id::DKG_MAX),
                ));
            }
        }
        "signing" => {
            if stream_id < stream_id::SIGNING_MIN || stream_id > stream_id::SIGNING_MAX {
                return Err(NetworkError::InvalidStreamId(
                    stream_id,
                    format!("not in signing range ({}-{})", stream_id::SIGNING_MIN, stream_id::SIGNING_MAX),
                ));
            }
        }
        "presignature" => {
            if stream_id < stream_id::PRESIGNATURE_MIN {
                return Err(NetworkError::InvalidStreamId(
                    stream_id,
                    format!("not in presignature range (>= {})", stream_id::PRESIGNATURE_MIN),
                ));
            }
        }
        _ => {
            return Err(NetworkError::InvalidStreamId(
                stream_id,
                format!("unknown message type: {}", message_type),
            ));
        }
    }
    Ok(())
}

/// Helper to get current unix timestamp.
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_id_ranges() {
        // Control messages
        assert_eq!(stream_id::control(0), 0);
        assert_eq!(stream_id::control(50), 50);
        assert!(stream_id::control(1000) <= stream_id::CONTROL_MAX);

        // DKG rounds
        assert_eq!(stream_id::dkg_round(0), stream_id::DKG_MIN);
        assert!(stream_id::dkg_round(1) >= stream_id::DKG_MIN);
        assert!(stream_id::dkg_round(1) <= stream_id::DKG_MAX);

        // Signing rounds
        assert_eq!(stream_id::signing_round(0), stream_id::SIGNING_MIN);
        assert!(stream_id::signing_round(1) >= stream_id::SIGNING_MIN);
        assert!(stream_id::signing_round(1) <= stream_id::SIGNING_MAX);

        // Presignature
        assert_eq!(stream_id::presignature(0), stream_id::PRESIGNATURE_MIN);
        assert_eq!(stream_id::presignature(100), stream_id::PRESIGNATURE_MIN + 100);
    }

    #[test]
    fn test_validate_stream_id() {
        // Valid control
        assert!(validate_stream_id(50, "control").is_ok());

        // Invalid control
        assert!(validate_stream_id(100, "control").is_err());

        // Valid DKG
        assert!(validate_stream_id(100, "dkg").is_ok());
        assert!(validate_stream_id(500, "dkg").is_ok());

        // Invalid DKG
        assert!(validate_stream_id(50, "dkg").is_err());
        assert!(validate_stream_id(1000, "dkg").is_err());

        // Valid signing
        assert!(validate_stream_id(1000, "signing").is_ok());
        assert!(validate_stream_id(5000, "signing").is_ok());

        // Invalid signing
        assert!(validate_stream_id(999, "signing").is_err());
        assert!(validate_stream_id(10000, "signing").is_err());

        // Valid presignature
        assert!(validate_stream_id(10000, "presignature").is_ok());
        assert!(validate_stream_id(100000, "presignature").is_ok());

        // Invalid presignature
        assert!(validate_stream_id(9999, "presignature").is_err());
    }

    #[test]
    fn test_session_proposal() {
        let proposal = SessionProposal::new(
            "session-123".to_string(),
            NodeId(0),
            "dkg",
            serde_json::json!({"threshold": 3}),
        );

        assert_eq!(proposal.session_id, "session-123");
        assert_eq!(proposal.initiator_node, NodeId(0));
        assert_eq!(proposal.protocol, "dkg");
        assert!(proposal.timestamp > 0);
    }

    #[test]
    fn test_session_ack() {
        let ack = SessionAck::accept("session-123".to_string(), NodeId(1));
        assert!(ack.accepted);
        assert!(ack.error.is_none());

        let ack = SessionAck::reject("session-123".to_string(), NodeId(2), "invalid".to_string());
        assert!(!ack.accepted);
        assert_eq!(ack.error, Some("invalid".to_string()));
    }
}
