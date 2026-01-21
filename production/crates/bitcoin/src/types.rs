//! Bitcoin type definitions.
//!
//! Common types used across the Bitcoin integration, including:
//! - UTXO and address information
//! - Transaction inputs and outputs
//! - Fee estimates
//! - Request/response types

use serde::{Deserialize, Serialize};

// ============================================================================
// UTXO and Address Types
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

/// Status of a UTXO (confirmed or in mempool).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UtxoStatus {
    pub confirmed: bool,
    #[serde(default)]
    pub block_height: Option<u64>,
}

/// Address balance and transaction information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressInfo {
    pub address: String,
    pub chain_stats: ChainStats,
    pub mempool_stats: MempoolStats,
}

/// On-chain statistics for an address.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChainStats {
    pub funded_txo_count: u64,
    pub funded_txo_sum: u64,
    pub spent_txo_count: u64,
    pub spent_txo_sum: u64,
    pub tx_count: u64,
}

/// Mempool statistics for an address.
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

// ============================================================================
// Transaction Types
// ============================================================================

/// An unsigned transaction ready for signing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsignedTransaction {
    /// Inputs to spend.
    pub inputs: Vec<TxInput>,
    /// Outputs to create.
    pub outputs: Vec<TxOutput>,
    /// Total input value.
    pub total_input_sats: u64,
    /// Total output value (excluding change).
    pub send_amount_sats: u64,
    /// Fee in satoshis.
    pub fee_sats: u64,
    /// Change amount (if any).
    pub change_sats: u64,
    /// Serialized unsigned transaction (hex).
    pub unsigned_tx_hex: String,
    /// Sighashes to sign (one per input).
    pub sighashes: Vec<String>,
}

/// Transaction input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxInput {
    pub txid: String,
    pub vout: u32,
    pub value: u64,
    /// Script pubkey of the input (hex).
    pub script_pubkey: String,
}

/// Transaction output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxOutput {
    pub address: String,
    pub value: u64,
    pub is_change: bool,
}

// ============================================================================
// Fee Estimation
// ============================================================================

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
    /// Optional metadata to embed in OP_RETURN output (max 80 bytes).
    #[serde(default)]
    pub metadata: Option<Vec<u8>>,
}

/// Response from sending Bitcoin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendBitcoinResponse {
    pub txid: String,
    pub amount_sats: u64,
    pub fee_sats: u64,
    pub to_address: String,
    pub has_metadata: bool,
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
    /// Create a balance response from address info.
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

/// Transaction broadcast result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastResult {
    pub txid: String,
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

    #[test]
    fn test_fee_estimates_recommended() {
        let fees = FeeEstimates {
            medium: 5.0,
            fast: 10.0,
            ..Default::default()
        };
        assert_eq!(fees.recommended(), 5);
    }

    #[test]
    fn test_balance_response_from_address_info() {
        let info = AddressInfo {
            address: "tb1qtest".to_string(),
            chain_stats: ChainStats {
                funded_txo_sum: 100_000,
                spent_txo_sum: 30_000,
                ..Default::default()
            },
            mempool_stats: MempoolStats::default(),
        };

        let balance = BalanceResponse::from_address_info("tb1qtest".to_string(), &info);
        assert_eq!(balance.confirmed_sats, 70_000);
        assert_eq!(balance.total_sats, 70_000);
    }

    #[test]
    fn test_send_bitcoin_request_with_metadata() {
        let req = SendBitcoinRequest {
            wallet_id: "test".to_string(),
            to_address: "tb1qtest".to_string(),
            amount_sats: 10_000,
            fee_rate: Some(5),
            metadata: Some(b"Hello Bitcoin!".to_vec()),
        };

        assert_eq!(req.metadata.unwrap().len(), 14);
    }
}
