//! Unified Bitcoin client that supports both Esplora (testnet/mainnet) and RPC (regtest).
//!
//! Configured via BITCOIN_NETWORK environment variable:
//! - "regtest" -> Uses Bitcoin Core RPC
//! - "testnet" (default) -> Uses Blockstream Esplora API
//! - "mainnet" -> Uses Blockstream Esplora API

use chains::bitcoin::{
    AddressInfo, BitcoinNetwork, BitcoinRpcClient, BlockchainClient, FeeEstimates, RpcConfig, Utxo,
};
use common::MpcWalletError;
use tracing::info;

/// Get the configured Bitcoin network from environment.
pub fn get_network() -> BitcoinNetwork {
    let network_str = std::env::var("BITCOIN_NETWORK").unwrap_or_else(|_| "testnet".to_string());
    BitcoinNetwork::parse(&network_str)
}

/// Unified Bitcoin backend that works with both Esplora and RPC.
pub enum BitcoinBackend {
    Esplora(BlockchainClient),
    Rpc(BitcoinRpcClient),
}

impl BitcoinBackend {
    /// Create a new backend based on BITCOIN_NETWORK environment variable.
    pub fn from_env() -> Result<Self, MpcWalletError> {
        let network = get_network();

        if network.uses_rpc() {
            info!("Using Bitcoin Core RPC backend (regtest)");
            let config = RpcConfig::from_env()?;
            Ok(BitcoinBackend::Rpc(BitcoinRpcClient::new(config)))
        } else {
            info!("Using Esplora API backend ({:?})", network);
            Ok(BitcoinBackend::Esplora(BlockchainClient::new(network)?))
        }
    }

    /// Get address info.
    pub async fn get_address_info(&self, address: &str) -> Result<AddressInfo, MpcWalletError> {
        match self {
            BitcoinBackend::Esplora(client) => client.get_address_info(address).await,
            BitcoinBackend::Rpc(client) => client.get_address_info(address).await,
        }
    }

    /// Get UTXOs for an address.
    pub async fn get_utxos(&self, address: &str) -> Result<Vec<Utxo>, MpcWalletError> {
        match self {
            BitcoinBackend::Esplora(client) => client.get_utxos(address).await,
            BitcoinBackend::Rpc(client) => client.get_utxos(address).await,
        }
    }

    /// Broadcast a signed transaction.
    pub async fn broadcast_tx(&self, tx_hex: &str) -> Result<String, MpcWalletError> {
        match self {
            BitcoinBackend::Esplora(client) => client.broadcast_tx(tx_hex).await,
            BitcoinBackend::Rpc(client) => client.broadcast_tx(tx_hex).await,
        }
    }

    /// Get fee estimates.
    pub async fn get_fee_estimates(&self) -> Result<FeeEstimates, MpcWalletError> {
        match self {
            BitcoinBackend::Esplora(client) => client.get_fee_estimates().await,
            BitcoinBackend::Rpc(client) => client.get_fee_estimates().await,
        }
    }

    /// Get transaction URL for display.
    pub fn tx_url(&self, txid: &str) -> String {
        match self {
            BitcoinBackend::Esplora(client) => client.tx_url(txid),
            BitcoinBackend::Rpc(client) => client.tx_url(txid),
        }
    }

    /// Get address URL for display.
    pub fn address_url(&self, address: &str) -> String {
        match self {
            BitcoinBackend::Esplora(client) => client.address_url(address),
            BitcoinBackend::Rpc(client) => client.address_url(address),
        }
    }

    /// Mine blocks (regtest only).
    pub async fn mine_blocks(
        &self,
        num_blocks: u32,
        address: &str,
    ) -> Result<Vec<String>, MpcWalletError> {
        match self {
            BitcoinBackend::Rpc(client) => client.generate_to_address(num_blocks, address).await,
            BitcoinBackend::Esplora(_) => Err(MpcWalletError::Configuration(
                "Mining is only available on regtest".to_string(),
            )),
        }
    }

    /// Check if this is a regtest backend.
    pub fn is_regtest(&self) -> bool {
        matches!(self, BitcoinBackend::Rpc(_))
    }

    /// Get the network name for display.
    pub fn network_name(&self) -> &'static str {
        match self {
            BitcoinBackend::Rpc(_) => "regtest",
            BitcoinBackend::Esplora(_) => {
                let network = get_network();
                match network {
                    BitcoinNetwork::Mainnet => "mainnet",
                    BitcoinNetwork::Testnet => "testnet",
                    BitcoinNetwork::Regtest => "regtest",
                }
            }
        }
    }
}
