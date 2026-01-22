//! Error types for the orchestration service

use thiserror::Error;

/// Result type for orchestration operations
pub type Result<T> = std::result::Result<T, OrchestrationError>;

/// Errors that can occur during orchestration
#[derive(Error, Debug)]
pub enum OrchestrationError {
    #[error("Storage error: {0}")]
    Storage(#[from] anyhow::Error),

    #[error("Consensus error: {0}")]
    Consensus(String),

    #[error("P2P protocol error: {0}")]
    Protocol(String),

    #[error("Bitcoin client error: {0}")]
    Bitcoin(String),

    #[error("Transaction {0} not found")]
    TransactionNotFound(String),

    #[error("Transaction {0} in invalid state for operation: {1}")]
    InvalidState(String, String),

    #[error("Timeout waiting for {0}")]
    Timeout(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Shutdown signal received")]
    Shutdown,

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("DKG ceremony already in progress: {0}")]
    CeremonyInProgress(String),

    #[error("DKG ceremony not found: {0}")]
    CeremonyNotFound(uuid::Uuid),

    #[error("Invalid public key: {0}")]
    InvalidPublicKey(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),
}

impl From<tokio::task::JoinError> for OrchestrationError {
    fn from(err: tokio::task::JoinError) -> Self {
        OrchestrationError::Internal(format!("Task join error: {}", err))
    }
}
