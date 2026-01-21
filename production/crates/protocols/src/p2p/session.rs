//! P2P Session Control Messages
//!
//! This module defines the message types used for coordinating signing sessions
//! in P2P mode. These messages enable nodes to propose, acknowledge, start,
//! and abort sessions without coordinator involvement.
//!
//! ## Session Lifecycle
//!
//! ```text
//! 1. Initiator receives grant from CLI/external
//! 2. Initiator broadcasts SessionProposal to participants
//! 3. Participants validate grant and send SessionAck
//! 4. When threshold acks received, initiator broadcasts SessionStart
//! 5. Nodes execute signing protocol via P2P
//! 6. On error, any node can send SessionAbort
//! ```

use common::grant::SigningGrant;
use serde::{Deserialize, Serialize};

/// P2P control messages for session coordination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionControlMessage {
    /// Initiator proposes a signing session to participants.
    ///
    /// Sent by the deterministically-selected initiator to all participants.
    /// Recipients validate the grant and respond with SessionAck.
    Proposal(SessionProposal),

    /// Participant acknowledges a session proposal.
    ///
    /// Sent by each participant back to the initiator.
    /// Once threshold acks are received, initiator sends SessionStart.
    Ack(SessionAck),

    /// Initiator signals that the session can start.
    ///
    /// Sent after threshold participants have acknowledged.
    /// Triggers protocol execution on all nodes.
    Start(SessionStart),

    /// Any party signals session abort.
    ///
    /// Can be sent by any participant if an error occurs.
    /// All nodes should stop processing for this session.
    Abort(SessionAbort),
}

impl SessionControlMessage {
    /// Get the session ID from the control message.
    ///
    /// For proposals, derives the session ID from the grant.
    /// For other message types, returns the embedded session_id.
    pub fn session_id(&self) -> Option<String> {
        match self {
            SessionControlMessage::Proposal(p) => Some(p.session_id()),
            SessionControlMessage::Ack(a) => Some(a.session_id.clone()),
            SessionControlMessage::Start(s) => Some(s.session_id.clone()),
            SessionControlMessage::Abort(a) => Some(a.session_id.clone()),
        }
    }
}

/// Session proposal sent by initiator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionProposal {
    /// The signed grant authorizing this session.
    pub grant: SigningGrant,

    /// Party index of the initiator.
    pub initiator_party: u16,

    /// Protocol type (e.g., "cggmp24", "frost").
    pub protocol: String,

    /// Unix timestamp when proposal was created.
    pub timestamp: u64,
}

impl SessionProposal {
    /// Create a new session proposal.
    pub fn new(grant: SigningGrant, initiator_party: u16, protocol: &str) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            grant,
            initiator_party,
            protocol: protocol.to_string(),
            timestamp,
        }
    }

    /// Get the session ID derived from the grant.
    pub fn session_id(&self) -> String {
        self.grant.session_id()
    }
}

/// Session acknowledgment from a participant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionAck {
    /// Session ID (derived from grant).
    pub session_id: String,

    /// Party index of the acknowledging node.
    pub party_index: u16,

    /// Whether the node accepts the proposal.
    pub accepted: bool,

    /// Error message if rejected.
    pub error: Option<String>,

    /// Unix timestamp when ack was created.
    pub timestamp: u64,
}

impl SessionAck {
    /// Create an accepting ack.
    pub fn accept(session_id: String, party_index: u16) -> Self {
        Self {
            session_id,
            party_index,
            accepted: true,
            error: None,
            timestamp: now(),
        }
    }

    /// Create a rejecting ack.
    pub fn reject(session_id: String, party_index: u16, reason: String) -> Self {
        Self {
            session_id,
            party_index,
            accepted: false,
            error: Some(reason),
            timestamp: now(),
        }
    }
}

/// Session start signal from initiator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStart {
    /// Session ID (derived from grant).
    pub session_id: String,

    /// Confirmed participants who acknowledged.
    pub participants: Vec<u16>,

    /// Unix timestamp when start was signaled.
    pub timestamp: u64,
}

impl SessionStart {
    /// Create a new session start message.
    pub fn new(session_id: String, participants: Vec<u16>) -> Self {
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

    /// Party index of the node sending abort.
    pub sender: u16,

    /// Reason for abort.
    pub reason: String,

    /// Unix timestamp when abort was signaled.
    pub timestamp: u64,
}

impl SessionAbort {
    /// Create a new session abort message.
    pub fn new(session_id: String, sender: u16, reason: String) -> Self {
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
pub enum P2pSessionStatus {
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

impl std::fmt::Display for P2pSessionStatus {
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
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn create_test_grant() -> SigningGrant {
        let signing_key = SigningKey::generate(&mut OsRng);
        SigningGrant::new(
            "test-wallet".to_string(),
            [0u8; 32],
            3,
            vec![0, 1, 2],
            &signing_key,
        )
    }

    #[test]
    fn test_session_proposal_creation() {
        let grant = create_test_grant();
        let proposal = SessionProposal::new(grant.clone(), 0, "cggmp24");

        assert_eq!(proposal.initiator_party, 0);
        assert_eq!(proposal.protocol, "cggmp24");
        assert_eq!(proposal.session_id(), grant.session_id());
        assert!(proposal.timestamp > 0);
    }

    #[test]
    fn test_session_ack_accept() {
        let ack = SessionAck::accept("session-123".to_string(), 1);

        assert_eq!(ack.session_id, "session-123");
        assert_eq!(ack.party_index, 1);
        assert!(ack.accepted);
        assert!(ack.error.is_none());
    }

    #[test]
    fn test_session_ack_reject() {
        let ack = SessionAck::reject("session-123".to_string(), 2, "invalid grant".to_string());

        assert_eq!(ack.session_id, "session-123");
        assert_eq!(ack.party_index, 2);
        assert!(!ack.accepted);
        assert_eq!(ack.error, Some("invalid grant".to_string()));
    }

    #[test]
    fn test_session_start() {
        let start = SessionStart::new("session-456".to_string(), vec![0, 1, 2]);

        assert_eq!(start.session_id, "session-456");
        assert_eq!(start.participants, vec![0, 1, 2]);
        assert!(start.timestamp > 0);
    }

    #[test]
    fn test_session_abort() {
        let abort = SessionAbort::new("session-789".to_string(), 1, "timeout".to_string());

        assert_eq!(abort.session_id, "session-789");
        assert_eq!(abort.sender, 1);
        assert_eq!(abort.reason, "timeout");
    }

    #[test]
    fn test_session_control_message_serialization() {
        let grant = create_test_grant();
        let proposal = SessionProposal::new(grant, 0, "cggmp24");
        let msg = SessionControlMessage::Proposal(proposal);

        // Should serialize/deserialize cleanly
        let json = serde_json::to_string(&msg).unwrap();
        let restored: SessionControlMessage = serde_json::from_str(&json).unwrap();

        match restored {
            SessionControlMessage::Proposal(p) => {
                assert_eq!(p.initiator_party, 0);
                assert_eq!(p.protocol, "cggmp24");
            }
            _ => panic!("Expected Proposal"),
        }
    }

    #[test]
    fn test_p2p_session_status_display() {
        assert_eq!(P2pSessionStatus::Proposed.to_string(), "proposed");
        assert_eq!(P2pSessionStatus::InProgress.to_string(), "in_progress");
        assert_eq!(P2pSessionStatus::Completed.to_string(), "completed");
    }
}
