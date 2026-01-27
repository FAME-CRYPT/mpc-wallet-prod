use serde::{Deserialize, Serialize};
use std::fmt;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Unique identifier for a node in the MPC network
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u64);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "node-{}", self.0)
    }
}

impl From<u64> for NodeId {
    fn from(id: u64) -> Self {
        NodeId(id)
    }
}

/// Bitcoin transaction identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TxId(pub String);

impl fmt::Display for TxId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for TxId {
    fn from(s: String) -> Self {
        TxId(s)
    }
}

impl From<&str> for TxId {
    fn from(s: &str) -> Self {
        TxId(s.to_string())
    }
}

/// Alias for backward compatibility with mtls-comm code
pub type TransactionId = TxId;

/// Peer identifier (network identity)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

/// Vote from a node on a transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    pub tx_id: TxId,
    pub node_id: NodeId,
    pub peer_id: PeerId,
    pub round_id: u64,
    pub approve: bool,
    pub value: u64,
    pub signature: Vec<u8>,
    pub public_key: Vec<u8>,
    pub timestamp: DateTime<Utc>,
}

impl Vote {
    pub fn new(node_id: NodeId, tx_id: TxId, round_id: u64, approve: bool, value: Option<u64>) -> Self {
        Self {
            tx_id,
            node_id,
            peer_id: PeerId::from(format!("peer-{}", node_id.0)),
            round_id,
            approve,
            value: value.unwrap_or(0),
            signature: Vec::new(),
            public_key: Vec::new(),
            timestamp: Utc::now(),
        }
    }

    pub fn with_signature(mut self, signature: Vec<u8>) -> Self {
        self.signature = signature;
        self
    }

    pub fn with_public_key(mut self, public_key: Vec<u8>) -> Self {
        self.public_key = public_key;
        self
    }
}

/// Transaction state in the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionState {
    Pending,
    Voting,
    Collecting,
    ThresholdReached,
    Approved,
    Rejected,
    Signing,
    Signed,
    Submitted,
    Broadcasting,
    Confirmed,
    Failed,
    AbortedByzantine,
}

impl fmt::Display for TransactionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransactionState::Pending => write!(f, "pending"),
            TransactionState::Voting => write!(f, "voting"),
            TransactionState::Collecting => write!(f, "collecting"),
            TransactionState::ThresholdReached => write!(f, "threshold_reached"),
            TransactionState::Approved => write!(f, "approved"),
            TransactionState::Rejected => write!(f, "rejected"),
            TransactionState::Signing => write!(f, "signing"),
            TransactionState::Signed => write!(f, "signed"),
            TransactionState::Submitted => write!(f, "submitted"),
            TransactionState::Broadcasting => write!(f, "broadcasting"),
            TransactionState::Confirmed => write!(f, "confirmed"),
            TransactionState::Failed => write!(f, "failed"),
            TransactionState::AbortedByzantine => write!(f, "aborted_byzantine"),
        }
    }
}

/// Transaction record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: i64,
    pub txid: TxId,
    pub state: TransactionState,
    pub unsigned_tx: Vec<u8>,
    pub signed_tx: Option<Vec<u8>>,
    pub recipient: String,
    pub amount_sats: u64,
    pub fee_sats: u64,
    pub metadata: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Byzantine violation types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationType {
    DoubleVote,
    InvalidSignature,
    Timeout,
    MalformedMessage,
    MinorityVote,
}

impl fmt::Display for ViolationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ViolationType::DoubleVote => write!(f, "double_vote"),
            ViolationType::InvalidSignature => write!(f, "invalid_signature"),
            ViolationType::Timeout => write!(f, "timeout"),
            ViolationType::MalformedMessage => write!(f, "malformed_message"),
            ViolationType::MinorityVote => write!(f, "minority_vote"),
        }
    }
}

/// Alias for backward compatibility with mtls-comm code
pub type ByzantineViolationType = ViolationType;

/// Byzantine violation record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ByzantineViolation {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub id: Option<i64>,
    pub peer_id: PeerId,
    pub node_id: Option<NodeId>,
    pub tx_id: TxId,
    pub violation_type: ViolationType,
    pub evidence: serde_json::Value,
    pub detected_at: DateTime<Utc>,
}

impl ByzantineViolation {
    pub fn new(
        peer_id: PeerId,
        node_id: NodeId,
        tx_id: TxId,
        violation_type: ViolationType,
        evidence: serde_json::Value,
    ) -> Self {
        Self {
            id: None,
            peer_id,
            node_id: Some(node_id),
            tx_id,
            violation_type,
            evidence,
            detected_at: Utc::now(),
        }
    }
}

/// Request for nodes to vote on a transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteRequest {
    pub tx_id: TxId,
    pub round_id: i64,
    pub round_number: u32,
    pub threshold: u32,
    pub timeout_at: DateTime<Utc>,
}

/// Voting round record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VotingRound {
    pub id: i64,
    pub tx_id: TxId,
    pub round_number: u32,
    pub total_nodes: u32,
    pub threshold: u32,
    pub votes_received: u32,
    pub approved: bool,
    pub completed: bool,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub timeout_at: DateTime<Utc>,
}

/// Consensus result when threshold is reached
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusResult {
    pub tx_id: TxId,
    pub value: u64,
    pub vote_count: u64,
    pub reached_at: DateTime<Utc>,
}

/// Presignature identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PresignatureId(pub Uuid);

impl fmt::Display for PresignatureId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl PresignatureId {
    pub fn new() -> Self {
        PresignatureId(Uuid::new_v4())
    }
}

impl Default for PresignatureId {
    fn default() -> Self {
        Self::new()
    }
}

/// Presignature usage record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresignatureUsage {
    pub id: i64,
    pub presig_id: PresignatureId,
    pub transaction_id: i64,
    pub used_at: DateTime<Utc>,
    pub generation_time_ms: i32,
}

/// Network message types
/// Note: Using JSON for serialization (supports serde tags and deserialize_any)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NetworkMessage {
    Vote(VoteMessage),
    Heartbeat(HeartbeatMessage),
    DkgRound(DkgMessage),
    SigningRound(SigningMessage),
    PresignatureGen(PresignatureMessage),
    /// Generic protocol message for message router
    Protocol {
        session_id: String,
        from: NodeId,
        to: NodeId,
        payload: Vec<u8>,
        is_broadcast: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteMessage {
    pub vote: Vote,
    pub message_id: Uuid,
}

impl VoteMessage {
    pub fn new(vote: Vote) -> Self {
        Self {
            vote,
            message_id: Uuid::new_v4(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatMessage {
    pub node_id: NodeId,
    pub timestamp: DateTime<Utc>,
    pub sequence: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DkgMessage {
    pub session_id: Uuid,
    pub round: u32,
    pub from: NodeId,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigningMessage {
    pub tx_id: TxId,
    pub round: u32,
    pub from: NodeId,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresignatureMessage {
    pub presig_id: PresignatureId,
    pub round: u32,
    pub from: NodeId,
    pub payload: Vec<u8>,
}

/// Configuration for the node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub node_id: u64,
    pub listen_addr: String,
    pub total_nodes: u32,
    pub threshold: u32,
}

/// Consensus configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusConfig {
    pub total_nodes: u32,
    pub threshold: u32,
    pub vote_timeout_secs: u64,
}

/// QUIC configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuicConfig {
    pub max_idle_timeout_ms: u64,
    pub keep_alive_interval_ms: u64,
    pub max_concurrent_streams: u64,
    pub max_stream_data: u64,
}

/// mTLS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MtlsConfig {
    pub ca_cert_path: String,
    pub node_cert_path: String,
    pub node_key_path: String,
    pub tls_version: String,
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub bootstrap_peers: Vec<String>,
    pub heartbeat_interval_secs: u64,
    pub reconnect_delay_secs: u64,
}

/// etcd configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtcdConfig {
    pub endpoints: Vec<String>,
    pub connect_timeout_secs: u64,
    pub request_timeout_secs: u64,
}

/// PostgreSQL configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresConfig {
    pub url: String,
    pub max_connections: u32,
    pub connect_timeout_secs: u64,
}

/// Bitcoin configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitcoinConfig {
    pub network: String,
    pub esplora_url: String,
    pub rpc_url: Option<String>,
    pub rpc_user: Option<String>,
    pub rpc_password: Option<String>,
}

/// Full application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub node: NodeConfig,
    pub quic: QuicConfig,
    pub mtls: MtlsConfig,
    pub network: NetworkConfig,
    pub consensus: ConsensusConfig,
    pub etcd: EtcdConfig,
    pub postgres: PostgresConfig,
    pub bitcoin: BitcoinConfig,
}

/// Result type for operations
pub type Result<T> = std::result::Result<T, Error>;

/// Common error types
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Node not found: {0}")]
    NodeNotFound(NodeId),

    #[error("Transaction not found: {0}")]
    TransactionNotFound(TxId),

    #[error("Invalid vote: {0}")]
    InvalidVote(String),

    #[error("Byzantine violation detected: {0}")]
    ByzantineViolation(String),

    #[error("Consensus failed: {0}")]
    ConsensusFailed(String),

    #[error("Signing failed: {0}")]
    SigningFailed(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Cryptographic error: {0}")]
    CryptoError(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Node banned: {peer_id}")]
    NodeBanned { peer_id: String },

    #[error("Transaction already processed: {tx_id}")]
    TransactionAlreadyProcessed { tx_id: String },

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Voting-specific error type (alias for backward compatibility)
pub type VotingError = Error;
