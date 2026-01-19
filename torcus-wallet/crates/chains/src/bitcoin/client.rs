//! Bitcoin blockchain API client (Esplora/Blockstream compatible).
//!
//! Provides async access to:
//! - Address info and balances
//! - UTXOs
//! - Transaction broadcasting
//! - Fee estimation

use common::MpcWalletError;
use serde::{Deserialize, Serialize};

// ============================================================================
// Blockchain API Types (Blockstream/Esplora compatible)
// ============================================================================

/// A UTXO (Unspent Transaction Output).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Utxo {
    pub txid: String,
    pub vout: u32,
    pub value: u64, // satoshis
    #[serde(default)]
    pub status: UtxoStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UtxoStatus {
    pub confirmed: bool,
    #[serde(default)]
    pub block_height: Option<u64>,
}

/// Address balance information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressInfo {
    pub address: String,
    pub chain_stats: ChainStats,
    pub mempool_stats: MempoolStats,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChainStats {
    pub funded_txo_count: u64,
    pub funded_txo_sum: u64,
    pub spent_txo_count: u64,
    pub spent_txo_sum: u64,
    pub tx_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MempoolStats {
    pub funded_txo_count: u64,
    pub funded_txo_sum: u64,
    pub spent_txo_count: u64,
    pub spent_txo_sum: u64,
    pub tx_count: u64,
}

impl AddressInfo {
    /// Get confirmed balance in satoshis.
    pub fn confirmed_balance(&self) -> u64 {
        self.chain_stats
            .funded_txo_sum
            .saturating_sub(self.chain_stats.spent_txo_sum)
    }

    /// Get unconfirmed balance in satoshis.
    pub fn unconfirmed_balance(&self) -> i64 {
        (self.mempool_stats.funded_txo_sum as i64) - (self.mempool_stats.spent_txo_sum as i64)
    }

    /// Get total balance in satoshis.
    pub fn total_balance(&self) -> u64 {
        let confirmed = self.confirmed_balance() as i64;
        let unconfirmed = self.unconfirmed_balance();
        (confirmed + unconfirmed).max(0) as u64
    }
}

/// Transaction to broadcast.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastResult {
    pub txid: String,
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request to send Bitcoin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendBitcoinRequest {
    /// Wallet ID to send from.
    pub wallet_id: String,
    /// Destination Bitcoin address.
    pub to_address: String,
    /// Amount to send in satoshis.
    pub amount_sats: u64,
    /// Fee rate in sat/vB (optional, will estimate if not provided).
    pub fee_rate: Option<u64>,
}

/// Response from sending Bitcoin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendBitcoinResponse {
    pub txid: String,
    pub amount_sats: u64,
    pub fee_sats: u64,
    pub to_address: String,
    pub message: String,
}

/// Balance response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceResponse {
    pub address: String,
    pub confirmed_sats: u64,
    pub unconfirmed_sats: i64,
    pub total_sats: u64,
    pub confirmed_btc: String,
    pub total_btc: String,
}

impl BalanceResponse {
    pub fn from_address_info(address: String, info: &AddressInfo) -> Self {
        let confirmed = info.confirmed_balance();
        let unconfirmed = info.unconfirmed_balance();
        let total = info.total_balance();

        Self {
            address,
            confirmed_sats: confirmed,
            unconfirmed_sats: unconfirmed,
            total_sats: total,
            confirmed_btc: format!("{:.8}", confirmed as f64 / 100_000_000.0),
            total_btc: format!("{:.8}", total as f64 / 100_000_000.0),
        }
    }
}

// ============================================================================
// Blockchain API Client
// ============================================================================

/// Bitcoin network for API calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitcoinNetwork {
    Mainnet,
    Testnet,
    Regtest,
}

impl BitcoinNetwork {
    /// Parse from string (environment variable).
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "mainnet" | "main" => BitcoinNetwork::Mainnet,
            "regtest" | "reg" => BitcoinNetwork::Regtest,
            _ => BitcoinNetwork::Testnet, // Default to testnet
        }
    }

    /// Get the Blockstream API base URL (for Testnet/Mainnet).
    pub fn api_url(&self) -> Option<&'static str> {
        match self {
            BitcoinNetwork::Mainnet => Some("https://blockstream.info/api"),
            BitcoinNetwork::Testnet => Some("https://blockstream.info/testnet/api"),
            BitcoinNetwork::Regtest => None, // Uses RPC instead
        }
    }

    /// Get the block explorer URL.
    pub fn explorer_url(&self) -> Option<&'static str> {
        match self {
            BitcoinNetwork::Mainnet => Some("https://blockstream.info"),
            BitcoinNetwork::Testnet => Some("https://blockstream.info/testnet"),
            BitcoinNetwork::Regtest => None, // No explorer for regtest
        }
    }

    /// Check if this network uses RPC (vs Esplora API).
    pub fn uses_rpc(&self) -> bool {
        matches!(self, BitcoinNetwork::Regtest)
    }

    /// Get the bitcoin crate Network type.
    pub fn to_bitcoin_network(&self) -> bitcoin::Network {
        match self {
            BitcoinNetwork::Mainnet => bitcoin::Network::Bitcoin,
            BitcoinNetwork::Testnet => bitcoin::Network::Testnet,
            BitcoinNetwork::Regtest => bitcoin::Network::Regtest,
        }
    }

    /// Get faucet URLs for testnet.
    pub fn faucet_urls() -> Vec<&'static str> {
        vec![
            "https://coinfaucet.eu/en/btc-testnet/",
            "https://testnet-faucet.mempool.co/",
            "https://bitcoinfaucet.uo1.net/",
        ]
    }
}

/// Async client for blockchain API (Esplora).
/// For regtest, use `BitcoinRpcClient` instead.
pub struct BlockchainClient {
    network: BitcoinNetwork,
    api_base: String,
    client: reqwest::Client,
}

impl BlockchainClient {
    pub fn new(network: BitcoinNetwork) -> Result<Self, MpcWalletError> {
        let api_base = network.api_url().ok_or_else(|| {
            MpcWalletError::Configuration(
                "Esplora API not available for regtest. Use BitcoinRpcClient instead.".to_string(),
            )
        })?;

        Ok(Self {
            network,
            api_base: api_base.to_string(),
            client: reqwest::Client::new(),
        })
    }

    /// Get address info (balance, tx count, etc).
    pub async fn get_address_info(&self, address: &str) -> Result<AddressInfo, MpcWalletError> {
        let url = format!("{}/address/{}", self.api_base, address);

        let response =
            self.client.get(&url).send().await.map_err(|e| {
                MpcWalletError::NodeCommunication(format!("API request failed: {}", e))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(MpcWalletError::NodeCommunication(format!(
                "API error {}: {}",
                status, body
            )));
        }

        let mut info: AddressInfo = response.json().await.map_err(|e| {
            MpcWalletError::Serialization(format!("Failed to parse response: {}", e))
        })?;

        info.address = address.to_string();
        Ok(info)
    }

    /// Get UTXOs for an address.
    pub async fn get_utxos(&self, address: &str) -> Result<Vec<Utxo>, MpcWalletError> {
        let url = format!("{}/address/{}/utxo", self.api_base, address);

        let response =
            self.client.get(&url).send().await.map_err(|e| {
                MpcWalletError::NodeCommunication(format!("API request failed: {}", e))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(MpcWalletError::NodeCommunication(format!(
                "API error {}: {}",
                status, body
            )));
        }

        response
            .json()
            .await
            .map_err(|e| MpcWalletError::Serialization(format!("Failed to parse UTXOs: {}", e)))
    }

    /// Broadcast a signed transaction.
    pub async fn broadcast_tx(&self, tx_hex: &str) -> Result<String, MpcWalletError> {
        let url = format!("{}/tx", self.api_base);

        let response = self
            .client
            .post(&url)
            .body(tx_hex.to_string())
            .send()
            .await
            .map_err(|e| MpcWalletError::NodeCommunication(format!("Broadcast failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(MpcWalletError::NodeCommunication(format!(
                "Broadcast error {}: {}",
                status, body
            )));
        }

        // Response is just the txid as plain text
        response
            .text()
            .await
            .map_err(|e| MpcWalletError::Serialization(format!("Failed to read txid: {}", e)))
    }

    /// Get current fee estimates (sat/vB).
    pub async fn get_fee_estimates(&self) -> Result<FeeEstimates, MpcWalletError> {
        let url = format!("{}/fee-estimates", self.api_base);

        let response = self.client.get(&url).send().await.map_err(|e| {
            MpcWalletError::NodeCommunication(format!("Fee estimate failed: {}", e))
        })?;

        if !response.status().is_success() {
            // Return defaults if API fails
            return Ok(FeeEstimates::default());
        }

        response
            .json()
            .await
            .map_err(|e| MpcWalletError::Serialization(format!("Failed to parse fees: {}", e)))
    }

    /// Get transaction URL for block explorer.
    pub fn tx_url(&self, txid: &str) -> String {
        if let Some(explorer) = self.network.explorer_url() {
            format!("{}/tx/{}", explorer, txid)
        } else {
            format!("txid:{}", txid)
        }
    }

    /// Get address URL for block explorer.
    pub fn address_url(&self, address: &str) -> String {
        if let Some(explorer) = self.network.explorer_url() {
            format!("{}/address/{}", explorer, address)
        } else {
            address.to_string()
        }
    }
}

/// Fee estimates by confirmation target (blocks).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FeeEstimates {
    #[serde(rename = "1", default)]
    pub fastest: f64,
    #[serde(rename = "3", default)]
    pub fast: f64,
    #[serde(rename = "6", default)]
    pub medium: f64,
    #[serde(rename = "144", default)]
    pub slow: f64,
    /// Minimum relay fee rate (sat/vB) from the node
    #[serde(default)]
    pub min_relay_fee: f64,
}

impl FeeEstimates {
    /// Get recommended fee rate for normal transactions.
    /// Returns the higher of recommended and minimum relay fee.
    pub fn recommended(&self) -> u64 {
        let base = if self.medium > 0.0 {
            self.medium.ceil() as u64
        } else if self.fast > 0.0 {
            self.fast.ceil() as u64
        } else {
            1
        };
        // Ensure we're above minimum relay fee
        let min_relay = self.min_relay_fee.ceil() as u64;
        base.max(min_relay).max(1)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_balance_calculation() {
        let info = AddressInfo {
            address: "tb1qtest".to_string(),
            chain_stats: ChainStats {
                funded_txo_sum: 100_000,
                spent_txo_sum: 30_000,
                ..Default::default()
            },
            mempool_stats: MempoolStats {
                funded_txo_sum: 5_000,
                spent_txo_sum: 0,
                ..Default::default()
            },
        };

        assert_eq!(info.confirmed_balance(), 70_000);
        assert_eq!(info.unconfirmed_balance(), 5_000);
        assert_eq!(info.total_balance(), 75_000);
    }

    #[test]
    fn test_fee_estimates_default() {
        let fees = FeeEstimates::default();
        assert_eq!(fees.recommended(), 1); // Default minimum
    }
}
