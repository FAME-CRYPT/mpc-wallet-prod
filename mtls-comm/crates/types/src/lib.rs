use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VotingError {
    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Double voting detected: node {node_id} voted {old_value} then {new_value}")]
    DoubleVoting {
        node_id: String,
        old_value: u64,
        new_value: u64,
    },

    #[error("Minority vote attack: node voted {voted_value} but majority voted {majority_value}")]
    MinorityVoteAttack {
        voted_value: u64,
        majority_value: u64,
    },

    #[error("Silent failure: node {node_id} timeout")]
    SilentFailure { node_id: String },

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Transaction already processed: {tx_id}")]
    TransactionAlreadyProcessed { tx_id: String },

    #[error("Node is banned: {peer_id}")]
    NodeBanned { peer_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransactionId(pub String);

impl fmt::Display for TransactionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for TransactionId {
    fn from(s: String) -> Self {
        TransactionId(s)
    }
}

impl From<&str> for TransactionId {
    fn from(s: &str) -> Self {
        TransactionId(s.to_string())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u64);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for NodeId {
    fn from(id: u64) -> Self {
        NodeId(id)
    }
}

impl From<String> for NodeId {
    fn from(s: String) -> Self {
        NodeId(s.parse().unwrap_or(0))
    }
}

impl From<&str> for NodeId {
    fn from(s: &str) -> Self {
        NodeId(s.parse().unwrap_or(0))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PeerId(pub String);

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for PeerId {
    fn from(s: String) -> Self {
        PeerId(s)
    }
}

impl From<&str> for PeerId {
    fn from(s: &str) -> Self {
        PeerId(s.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    pub tx_id: TransactionId,
    pub node_id: NodeId,
    pub peer_id: PeerId,
    pub value: u64,
    pub signature: Vec<u8>,
    pub public_key: Vec<u8>,
    pub timestamp: DateTime<Utc>,
}

impl Vote {
    pub fn new(
        tx_id: TransactionId,
        node_id: NodeId,
        peer_id: PeerId,
        value: u64,
        signature: Vec<u8>,
        public_key: Vec<u8>,
    ) -> Self {
        Self {
            tx_id,
            node_id,
            peer_id,
            value,
            signature,
            public_key,
            timestamp: Utc::now(),
        }
    }

    pub fn message_to_sign(&self) -> Vec<u8> {
        format!("{}||{}", self.tx_id, self.value)
            .as_bytes()
            .to_vec()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransactionState {
    Collecting,
    ThresholdReached,
    Submitted,
    Confirmed,
    AbortedByzantine,
}

impl fmt::Display for TransactionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransactionState::Collecting => write!(f, "COLLECTING"),
            TransactionState::ThresholdReached => write!(f, "THRESHOLD_REACHED"),
            TransactionState::Submitted => write!(f, "SUBMITTED"),
            TransactionState::Confirmed => write!(f, "CONFIRMED"),
            TransactionState::AbortedByzantine => write!(f, "ABORTED_BYZANTINE"),
        }
    }
}

impl TransactionState {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "COLLECTING" => Some(TransactionState::Collecting),
            "THRESHOLD_REACHED" => Some(TransactionState::ThresholdReached),
            "SUBMITTED" => Some(TransactionState::Submitted),
            "CONFIRMED" => Some(TransactionState::Confirmed),
            "ABORTED_BYZANTINE" => Some(TransactionState::AbortedByzantine),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ByzantineViolationType {
    DoubleVoting,
    MinorityVote,
    InvalidSignature,
    SilentFailure,
}

impl fmt::Display for ByzantineViolationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ByzantineViolationType::DoubleVoting => write!(f, "DOUBLE_VOTING"),
            ByzantineViolationType::MinorityVote => write!(f, "MINORITY_VOTE"),
            ByzantineViolationType::InvalidSignature => write!(f, "INVALID_SIGNATURE"),
            ByzantineViolationType::SilentFailure => write!(f, "SILENT_FAILURE"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ByzantineViolation {
    pub peer_id: PeerId,
    pub node_id: Option<NodeId>,
    pub tx_id: TransactionId,
    pub violation_type: ByzantineViolationType,
    pub evidence: serde_json::Value,
    pub detected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteCount {
    pub tx_id: TransactionId,
    pub value: u64,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    pub total_nodes: usize,
    pub threshold: usize,
    pub vote_timeout_secs: u64,
}

impl SystemConfig {
    pub fn new(total_nodes: usize, threshold: usize, vote_timeout_secs: u64) -> std::result::Result<Self, VotingError> {
        if threshold > total_nodes {
            return Err(VotingError::ConfigError(
                format!("Threshold {} cannot exceed total nodes {}", threshold, total_nodes)
            ));
        }

        if threshold == 0 {
            return Err(VotingError::ConfigError(
                "Threshold must be at least 1".to_string()
            ));
        }

        if total_nodes == 0 {
            return Err(VotingError::ConfigError(
                "Total nodes must be at least 1".to_string()
            ));
        }

        Ok(Self {
            total_nodes,
            threshold,
            vote_timeout_secs,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusResult {
    pub tx_id: TransactionId,
    pub value: u64,
    pub vote_count: u64,
    pub reached_at: DateTime<Utc>,
}

pub type Result<T> = std::result::Result<T, VotingError>;
