//! Bitcoin Core RPC client for regtest.
//!
//! Provides the same interface as BlockchainClient but uses Bitcoin Core RPC
//! instead of Esplora API. This enables regtest support with block mining.

use common::MpcWalletError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::client::{AddressInfo, ChainStats, FeeEstimates, MempoolStats, Utxo, UtxoStatus};

/// Bitcoin Core RPC client configuration.
#[derive(Debug, Clone)]
pub struct RpcConfig {
    pub url: String,
    pub user: String,
    pub password: String,
}

impl RpcConfig {
    /// Create config from environment variables.
    pub fn from_env() -> Result<Self, MpcWalletError> {
        let url = std::env::var("BITCOIN_RPC_URL")
            .unwrap_or_else(|_| "http://localhost:18443".to_string());
        let user = std::env::var("BITCOIN_RPC_USER").unwrap_or_else(|_| "bitcoin".to_string());
        let password =
            std::env::var("BITCOIN_RPC_PASSWORD").unwrap_or_else(|_| "bitcoin".to_string());

        Ok(Self {
            url,
            user,
            password,
        })
    }
}

/// Bitcoin Core RPC client for regtest.
pub struct BitcoinRpcClient {
    config: RpcConfig,
    client: reqwest::Client,
}

impl BitcoinRpcClient {
    pub fn new(config: RpcConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    /// Create from environment variables.
    pub fn from_env() -> Result<Self, MpcWalletError> {
        Ok(Self::new(RpcConfig::from_env()?))
    }

    /// Make an RPC call.
    async fn call<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        params: Vec<Value>,
    ) -> Result<T, MpcWalletError> {
        let body = json!({
            "jsonrpc": "1.0",
            "id": "mpc-wallet",
            "method": method,
            "params": params,
        });

        let response = self
            .client
            .post(&self.config.url)
            .basic_auth(&self.config.user, Some(&self.config.password))
            .json(&body)
            .send()
            .await
            .map_err(|e| MpcWalletError::NodeCommunication(format!("RPC request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(MpcWalletError::NodeCommunication(format!(
                "RPC error {}: {}",
                status, body
            )));
        }

        let result: RpcResponse<T> = response.json().await.map_err(|e| {
            MpcWalletError::Serialization(format!("Failed to parse RPC response: {}", e))
        })?;

        if let Some(error) = result.error {
            return Err(MpcWalletError::NodeCommunication(format!(
                "RPC error {}: {}",
                error.code, error.message
            )));
        }

        result.result.ok_or_else(|| {
            MpcWalletError::NodeCommunication("RPC returned null result".to_string())
        })
    }

    /// Get address info (balance, tx count, etc).
    /// Note: Bitcoin Core doesn't track arbitrary addresses by default.
    /// We use scantxoutset for balance and listunspent requires the address to be in the wallet.
    pub async fn get_address_info(&self, address: &str) -> Result<AddressInfo, MpcWalletError> {
        // Use scantxoutset to get balance for any address
        let scan_result: ScanTxOutSetResult = self
            .call(
                "scantxoutset",
                vec![json!("start"), json!([format!("addr({})", address)])],
            )
            .await?;

        Ok(AddressInfo {
            address: address.to_string(),
            chain_stats: ChainStats {
                funded_txo_count: scan_result.unspents.len() as u64,
                funded_txo_sum: scan_result.total_amount_sats(),
                spent_txo_count: 0, // Can't determine from scantxoutset
                spent_txo_sum: 0,
                tx_count: scan_result.unspents.len() as u64,
            },
            mempool_stats: MempoolStats::default(), // Mempool not tracked this way
        })
    }

    /// Get UTXOs for an address.
    pub async fn get_utxos(&self, address: &str) -> Result<Vec<Utxo>, MpcWalletError> {
        // Use scantxoutset to find UTXOs for any address
        let scan_result: ScanTxOutSetResult = self
            .call(
                "scantxoutset",
                vec![json!("start"), json!([format!("addr({})", address)])],
            )
            .await?;

        Ok(scan_result
            .unspents
            .into_iter()
            .map(|u| Utxo {
                txid: u.txid,
                vout: u.vout,
                value: (u.amount * 100_000_000.0) as u64,
                status: UtxoStatus {
                    confirmed: true, // scantxoutset only returns confirmed
                    block_height: Some(u.height),
                },
            })
            .collect())
    }

    /// Broadcast a signed transaction.
    pub async fn broadcast_tx(&self, tx_hex: &str) -> Result<String, MpcWalletError> {
        self.call("sendrawtransaction", vec![json!(tx_hex)]).await
    }

    /// Get the minimum relay fee from Bitcoin Core (sat/vB).
    async fn get_min_relay_fee(&self) -> f64 {
        // Try getnetworkinfo to get relayfee (in BTC/kB)
        let result: Result<Value, _> = self.call("getnetworkinfo", vec![]).await;
        if let Ok(info) = result {
            if let Some(relay_fee_btc_kb) = info["relayfee"].as_f64() {
                // Convert BTC/kB to sat/vB: BTC/kB * 100_000_000 / 1000 = sat/vB * 100
                // So: BTC/kB * 100_000 = sat/vB
                return relay_fee_btc_kb * 100_000.0;
            }
        }
        // Default fallback
        1.0
    }

    /// Get current fee estimates (sat/vB).
    /// For regtest, we return low fixed fees since there's no real fee market.
    pub async fn get_fee_estimates(&self) -> Result<FeeEstimates, MpcWalletError> {
        // Get minimum relay fee from the node
        let min_relay_fee = self.get_min_relay_fee().await;

        // Try to get estimate, but regtest often returns errors
        let result: Result<EstimateSmartFeeResult, _> =
            self.call("estimatesmartfee", vec![json!(6)]).await;

        match result {
            Ok(estimate) if estimate.feerate.is_some() => {
                let btc_per_kb = estimate.feerate.unwrap();
                let sat_per_vb = btc_per_kb * 100_000.0; // BTC/kB to sat/vB
                Ok(FeeEstimates {
                    fastest: sat_per_vb * 2.0,
                    fast: sat_per_vb * 1.5,
                    medium: sat_per_vb,
                    slow: sat_per_vb / 2.0,
                    min_relay_fee,
                })
            }
            _ => {
                // Default for regtest (no fee pressure)
                // Use min_relay_fee as the baseline
                Ok(FeeEstimates {
                    fastest: min_relay_fee * 2.0,
                    fast: min_relay_fee * 1.5,
                    medium: min_relay_fee,
                    slow: min_relay_fee,
                    min_relay_fee,
                })
            }
        }
    }

    /// Mine blocks to an address (regtest only).
    pub async fn generate_to_address(
        &self,
        num_blocks: u32,
        address: &str,
    ) -> Result<Vec<String>, MpcWalletError> {
        self.call("generatetoaddress", vec![json!(num_blocks), json!(address)])
            .await
    }

    /// Get blockchain info.
    pub async fn get_blockchain_info(&self) -> Result<BlockchainInfo, MpcWalletError> {
        self.call("getblockchaininfo", vec![]).await
    }

    /// Create a new wallet in Bitcoin Core (needed for some operations).
    pub async fn create_wallet(&self, name: &str) -> Result<CreateWalletResult, MpcWalletError> {
        self.call(
            "createwallet",
            vec![
                json!(name),
                json!(false), // disable_private_keys
                json!(true),  // blank
                json!(""),    // passphrase
                json!(false), // avoid_reuse
                json!(true),  // descriptors
                json!(false), // load_on_startup
            ],
        )
        .await
    }

    /// Load an existing wallet.
    pub async fn load_wallet(&self, name: &str) -> Result<LoadWalletResult, MpcWalletError> {
        self.call("loadwallet", vec![json!(name)]).await
    }

    /// Import an address to the wallet for tracking.
    pub async fn import_address(&self, address: &str, label: &str) -> Result<(), MpcWalletError> {
        // For descriptor wallets, use importdescriptors
        let descriptor = format!("addr({})", address);

        // Get checksum for descriptor
        let checksum_result: String = self
            .call("getdescriptorinfo", vec![json!(descriptor)])
            .await
            .map(|info: DescriptorInfo| info.descriptor)?;

        let import_request = json!([{
            "desc": checksum_result,
            "label": label,
            "timestamp": "now",
            "watchonly": true
        }]);

        let _result: Vec<ImportResult> =
            self.call("importdescriptors", vec![import_request]).await?;

        Ok(())
    }

    /// Get transaction URL (for regtest, just return txid).
    pub fn tx_url(&self, txid: &str) -> String {
        format!("txid:{}", txid)
    }

    /// Get address URL (for regtest, just return address).
    pub fn address_url(&self, address: &str) -> String {
        address.to_string()
    }
}

// ============================================================================
// RPC Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
    error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
struct RpcError {
    code: i32,
    message: String,
}

#[derive(Debug, Deserialize)]
struct ScanTxOutSetResult {
    #[serde(default)]
    unspents: Vec<ScanUnspent>,
    #[serde(default)]
    total_amount: f64,
}

impl ScanTxOutSetResult {
    fn total_amount_sats(&self) -> u64 {
        (self.total_amount * 100_000_000.0) as u64
    }
}

#[derive(Debug, Deserialize)]
struct ScanUnspent {
    txid: String,
    vout: u32,
    amount: f64,
    height: u64,
}

#[derive(Debug, Deserialize)]
struct EstimateSmartFeeResult {
    feerate: Option<f64>,
    #[allow(dead_code)]
    errors: Option<Vec<String>>,
}

/// Blockchain info from getblockchaininfo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockchainInfo {
    pub chain: String,
    pub blocks: u64,
    pub headers: u64,
    pub bestblockhash: String,
    #[serde(default)]
    pub initialblockdownload: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateWalletResult {
    pub name: String,
    #[serde(default)]
    pub warning: String,
}

#[derive(Debug, Deserialize)]
pub struct LoadWalletResult {
    pub name: String,
    #[serde(default)]
    pub warning: String,
}

#[derive(Debug, Deserialize)]
struct DescriptorInfo {
    descriptor: String,
}

#[derive(Debug, Deserialize)]
struct ImportResult {
    #[allow(dead_code)]
    success: bool,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_config_defaults() {
        // Clear env vars for test
        std::env::remove_var("BITCOIN_RPC_URL");
        std::env::remove_var("BITCOIN_RPC_USER");
        std::env::remove_var("BITCOIN_RPC_PASSWORD");

        let config = RpcConfig::from_env().unwrap();
        assert_eq!(config.url, "http://localhost:18443");
        assert_eq!(config.user, "bitcoin");
        assert_eq!(config.password, "bitcoin");
    }
}
