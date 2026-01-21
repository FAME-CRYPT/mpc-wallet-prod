//! Bitcoin blockchain API client (Esplora/Blockstream compatible).
//!
//! Provides async access to:
//! - Address info and balances
//! - UTXOs
//! - Transaction broadcasting
//! - Fee estimation
//! - Confirmation checking

use crate::types::{AddressInfo, FeeEstimates, Utxo};
use serde::Deserialize;
use thiserror::Error;

/// Bitcoin network configuration.
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

/// Errors that can occur when interacting with the Bitcoin blockchain.
#[derive(Debug, Error)]
pub enum BitcoinError {
    #[error("API request failed: {0}")]
    ApiRequest(String),

    #[error("API error {status}: {body}")]
    ApiError { status: u16, body: String },

    #[error("Failed to parse response: {0}")]
    ParseResponse(String),

    #[error("Broadcast error {status}: {body}")]
    Broadcast { status: u16, body: String },

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Invalid transaction hex: {0}")]
    InvalidTxHex(String),
}

/// Async client for blockchain API (Esplora).
/// For regtest, use `BitcoinRpcClient` instead.
pub struct BitcoinClient {
    network: BitcoinNetwork,
    api_base: String,
    client: reqwest::Client,
}

impl BitcoinClient {
    /// Create a new Bitcoin client.
    pub fn new(network: BitcoinNetwork) -> Result<Self, BitcoinError> {
        let api_base = network.api_url().ok_or_else(|| {
            BitcoinError::Configuration(
                "Esplora API not available for regtest. Use BitcoinRpcClient instead.".to_string(),
            )
        })?;

        Ok(Self {
            network,
            api_base: api_base.to_string(),
            client: reqwest::Client::new(),
        })
    }

    /// Create a client with a custom API URL (for self-hosted Esplora).
    pub fn with_api_url(network: BitcoinNetwork, api_url: String) -> Self {
        Self {
            network,
            api_base: api_url,
            client: reqwest::Client::new(),
        }
    }

    /// Get the network this client is configured for.
    pub fn network(&self) -> BitcoinNetwork {
        self.network
    }

    /// Get address info (balance, tx count, etc).
    pub async fn get_address_info(&self, address: &str) -> Result<AddressInfo, BitcoinError> {
        let url = format!("{}/address/{}", self.api_base, address);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| BitcoinError::ApiRequest(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(BitcoinError::ApiError { status, body });
        }

        let mut info: AddressInfo = response
            .json()
            .await
            .map_err(|e| BitcoinError::ParseResponse(e.to_string()))?;

        info.address = address.to_string();
        Ok(info)
    }

    /// Get UTXOs for an address.
    pub async fn get_utxos(&self, address: &str) -> Result<Vec<Utxo>, BitcoinError> {
        let url = format!("{}/address/{}/utxo", self.api_base, address);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| BitcoinError::ApiRequest(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(BitcoinError::ApiError { status, body });
        }

        response
            .json()
            .await
            .map_err(|e| BitcoinError::ParseResponse(e.to_string()))
    }

    /// Broadcast a signed transaction.
    /// Returns the transaction ID (txid).
    pub async fn broadcast_tx(&self, tx_hex: &str) -> Result<String, BitcoinError> {
        let url = format!("{}/tx", self.api_base);

        let response = self
            .client
            .post(&url)
            .body(tx_hex.to_string())
            .send()
            .await
            .map_err(|e| BitcoinError::ApiRequest(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(BitcoinError::Broadcast { status, body });
        }

        // Response is just the txid as plain text
        response
            .text()
            .await
            .map_err(|e| BitcoinError::ParseResponse(e.to_string()))
    }

    /// Get current fee estimates (sat/vB).
    pub async fn get_fee_estimates(&self) -> Result<FeeEstimates, BitcoinError> {
        let url = format!("{}/fee-estimates", self.api_base);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| BitcoinError::ApiRequest(e.to_string()))?;

        if !response.status().is_success() {
            // Return defaults if API fails
            return Ok(FeeEstimates::default());
        }

        response
            .json()
            .await
            .map_err(|e| BitcoinError::ParseResponse(e.to_string()))
    }

    /// Check if a transaction has been confirmed.
    /// Returns `Some(block_height)` if confirmed, `None` if still in mempool or not found.
    pub async fn get_tx_confirmation(&self, txid: &str) -> Result<Option<u64>, BitcoinError> {
        let url = format!("{}/tx/{}/status", self.api_base, txid);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| BitcoinError::ApiRequest(e.to_string()))?;

        if !response.status().is_success() {
            // Transaction might not exist yet
            return Ok(None);
        }

        #[derive(Deserialize)]
        struct TxStatus {
            confirmed: bool,
            block_height: Option<u64>,
        }

        let status: TxStatus = response
            .json()
            .await
            .map_err(|e| BitcoinError::ParseResponse(e.to_string()))?;

        if status.confirmed {
            Ok(status.block_height)
        } else {
            Ok(None)
        }
    }

    /// Get the current blockchain height.
    pub async fn get_block_height(&self) -> Result<u64, BitcoinError> {
        let url = format!("{}/blocks/tip/height", self.api_base);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| BitcoinError::ApiRequest(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(BitcoinError::ApiError { status, body });
        }

        let height_str = response
            .text()
            .await
            .map_err(|e| BitcoinError::ParseResponse(e.to_string()))?;

        height_str
            .trim()
            .parse()
            .map_err(|e| BitcoinError::ParseResponse(format!("Invalid height: {}", e)))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_parsing() {
        assert_eq!(BitcoinNetwork::parse("mainnet"), BitcoinNetwork::Mainnet);
        assert_eq!(BitcoinNetwork::parse("testnet"), BitcoinNetwork::Testnet);
        assert_eq!(BitcoinNetwork::parse("regtest"), BitcoinNetwork::Regtest);
        assert_eq!(BitcoinNetwork::parse("unknown"), BitcoinNetwork::Testnet); // Default
    }

    #[test]
    fn test_network_urls() {
        assert_eq!(
            BitcoinNetwork::Mainnet.api_url(),
            Some("https://blockstream.info/api")
        );
        assert_eq!(
            BitcoinNetwork::Testnet.api_url(),
            Some("https://blockstream.info/testnet/api")
        );
        assert_eq!(BitcoinNetwork::Regtest.api_url(), None);
    }

    #[test]
    fn test_bitcoin_network_conversion() {
        assert_eq!(
            BitcoinNetwork::Mainnet.to_bitcoin_network(),
            bitcoin::Network::Bitcoin
        );
        assert_eq!(
            BitcoinNetwork::Testnet.to_bitcoin_network(),
            bitcoin::Network::Testnet
        );
        assert_eq!(
            BitcoinNetwork::Regtest.to_bitcoin_network(),
            bitcoin::Network::Regtest
        );
    }
}
