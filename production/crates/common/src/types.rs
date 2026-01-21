//! API types for client-coordinator communication.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Supported wallet/blockchain types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WalletType {
    /// Native SegWit (P2WPKH) - uses ECDSA signatures (CGGMP24)
    #[default]
    Bitcoin,
    /// Taproot (P2TR) - uses Schnorr signatures (FROST)
    Taproot,
    /// Ethereum (for future use)
    Ethereum,
}

impl std::fmt::Display for WalletType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WalletType::Bitcoin => write!(f, "bitcoin"),
            WalletType::Taproot => write!(f, "taproot"),
            WalletType::Ethereum => write!(f, "ethereum"),
        }
    }
}

impl std::str::FromStr for WalletType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "bitcoin" | "btc" | "segwit" => Ok(WalletType::Bitcoin),
            "taproot" | "tr" | "p2tr" => Ok(WalletType::Taproot),
            "ethereum" | "eth" => Ok(WalletType::Ethereum),
            _ => Err(format!("Unknown wallet type: {}", s)),
        }
    }
}

/// Request to create a new MPC wallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWalletRequest {
    /// Human-readable name for the wallet.
    pub name: String,
    /// Type of wallet to create.
    #[serde(default)]
    pub wallet_type: WalletType,
}

/// Response from wallet creation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWalletResponse {
    /// Unique identifier for the wallet.
    pub wallet_id: Uuid,
    /// The generated public key (hex encoded, compressed).
    pub public_key: String,
    /// The blockchain address derived from the public key.
    pub address: String,
    /// Type of wallet created.
    pub wallet_type: WalletType,
    /// Status message.
    pub message: String,
}

/// Status of an MPC node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatus {
    pub node_id: Uuid,
    pub party_index: u16,
    pub is_ready: bool,
    pub has_aux_info: bool,
}

/// Error types for the MPC wallet system.
#[derive(Debug, thiserror::Error)]
pub enum MpcWalletError {
    #[error("Node communication error: {0}")]
    NodeCommunication(String),

    #[error("Wallet not found: {0}")]
    WalletNotFound(Uuid),

    #[error("Not enough nodes available: got {got}, need {need}")]
    NotEnoughNodes { got: u16, need: u16 },

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Invalid public key: {0}")]
    InvalidPublicKey(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Configuration error: {0}")]
    Configuration(String),
}
