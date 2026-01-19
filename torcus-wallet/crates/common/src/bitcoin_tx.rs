//! Bitcoin transaction building and utilities.
//!
//! Supports:
//! - Fetching UTXOs from blockchain API
//! - Building unsigned transactions
//! - Fee estimation

use serde::{Deserialize, Serialize};

use crate::MpcWalletError;

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
// Transaction Request/Response Types
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
}

impl BitcoinNetwork {
    /// Get the Blockstream API base URL.
    pub fn api_url(&self) -> &'static str {
        match self {
            BitcoinNetwork::Mainnet => "https://blockstream.info/api",
            BitcoinNetwork::Testnet => "https://blockstream.info/testnet/api",
        }
    }

    /// Get the block explorer URL.
    pub fn explorer_url(&self) -> &'static str {
        match self {
            BitcoinNetwork::Mainnet => "https://blockstream.info",
            BitcoinNetwork::Testnet => "https://blockstream.info/testnet",
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

/// Async client for blockchain API.
pub struct BlockchainClient {
    network: BitcoinNetwork,
    client: reqwest::Client,
}

impl BlockchainClient {
    pub fn new(network: BitcoinNetwork) -> Self {
        Self {
            network,
            client: reqwest::Client::new(),
        }
    }

    /// Get address info (balance, tx count, etc).
    pub async fn get_address_info(&self, address: &str) -> Result<AddressInfo, MpcWalletError> {
        let url = format!("{}/address/{}", self.network.api_url(), address);

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
        let url = format!("{}/address/{}/utxo", self.network.api_url(), address);

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
        let url = format!("{}/tx", self.network.api_url());

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
        let url = format!("{}/fee-estimates", self.network.api_url());

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
        format!("{}/tx/{}", self.network.explorer_url(), txid)
    }

    /// Get address URL for block explorer.
    pub fn address_url(&self, address: &str) -> String {
        format!("{}/address/{}", self.network.explorer_url(), address)
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
}

impl FeeEstimates {
    /// Get recommended fee rate for normal transactions.
    pub fn recommended(&self) -> u64 {
        if self.medium > 0.0 {
            self.medium.ceil() as u64
        } else if self.fast > 0.0 {
            self.fast.ceil() as u64
        } else {
            // Default minimum for testnet
            1
        }
    }
}

// ============================================================================
// Transaction Building
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxInput {
    pub txid: String,
    pub vout: u32,
    pub value: u64,
    /// Script pubkey of the input (hex).
    pub script_pubkey: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxOutput {
    pub address: String,
    pub value: u64,
    pub is_change: bool,
}

/// Build an unsigned transaction.
pub fn build_unsigned_transaction(
    utxos: &[Utxo],
    to_address: &str,
    amount_sats: u64,
    fee_rate_sat_vb: u64,
    change_address: &str,
    sender_script_pubkey: &[u8],
) -> Result<UnsignedTransaction, MpcWalletError> {
    use bitcoin::consensus::encode::serialize_hex;
    use bitcoin::hashes::Hash;
    use bitcoin::sighash::SighashCache;
    use bitcoin::transaction::Version;
    use bitcoin::{
        Address, Amount, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Txid, Witness,
    };
    use std::str::FromStr;

    // Sort UTXOs by value (largest first) for efficient selection
    let mut sorted_utxos = utxos.to_vec();
    sorted_utxos.sort_by(|a, b| b.value.cmp(&a.value));

    // Estimate transaction size for fee calculation
    // P2WPKH: ~110 vB for 1 input, ~40 vB per additional input, ~31 vB per output
    let base_size: u64 = 10; // version + locktime + overhead
    let input_size: u64 = 68; // P2WPKH input (witness data)
    let output_size: u64 = 31; // P2WPKH output

    // Select UTXOs (simple greedy algorithm)
    let mut selected_utxos: Vec<Utxo> = Vec::new();
    let mut total_input: u64 = 0;

    // Calculate minimum needed (amount + estimated fee for 1 input, 2 outputs)
    let min_fee = fee_rate_sat_vb * (base_size + input_size + 2 * output_size);
    let mut target = amount_sats + min_fee;

    for utxo in &sorted_utxos {
        if total_input >= target {
            break;
        }
        selected_utxos.push(utxo.clone());
        total_input += utxo.value;

        // Recalculate target with more inputs
        let estimated_size =
            base_size + (selected_utxos.len() as u64 * input_size) + (2 * output_size);
        let estimated_fee = fee_rate_sat_vb * estimated_size;
        target = amount_sats + estimated_fee;
    }

    if total_input < target {
        return Err(MpcWalletError::Protocol(format!(
            "Insufficient funds: have {} sats, need {} sats (including fee)",
            total_input, target
        )));
    }

    // Calculate actual fee
    let num_outputs = if total_input
        > amount_sats + (fee_rate_sat_vb * (base_size + input_size + 2 * output_size))
    {
        2 // Need change output
    } else {
        1 // No change
    };

    let tx_size =
        base_size + (selected_utxos.len() as u64 * input_size) + (num_outputs * output_size);
    let fee_sats = fee_rate_sat_vb * tx_size;
    let change_sats = total_input.saturating_sub(amount_sats + fee_sats);

    // Parse destination address
    let dest_address = Address::from_str(to_address)
        .map_err(|e| MpcWalletError::Protocol(format!("Invalid destination address: {}", e)))?
        .assume_checked();

    // Build transaction inputs
    let mut tx_inputs = Vec::new();
    let mut input_data = Vec::new();

    for utxo in &selected_utxos {
        let txid = Txid::from_str(&utxo.txid)
            .map_err(|e| MpcWalletError::Protocol(format!("Invalid txid: {}", e)))?;

        tx_inputs.push(TxIn {
            previous_output: OutPoint {
                txid,
                vout: utxo.vout,
            },
            script_sig: ScriptBuf::new(), // Empty for SegWit
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: Witness::default(), // Will be filled during signing
        });

        input_data.push(TxInput {
            txid: utxo.txid.clone(),
            vout: utxo.vout,
            value: utxo.value,
            script_pubkey: hex::encode(sender_script_pubkey),
        });
    }

    // Build transaction outputs
    let mut tx_outputs = Vec::new();
    let mut output_data = Vec::new();

    // Destination output
    tx_outputs.push(TxOut {
        value: Amount::from_sat(amount_sats),
        script_pubkey: dest_address.script_pubkey(),
    });
    output_data.push(TxOutput {
        address: to_address.to_string(),
        value: amount_sats,
        is_change: false,
    });

    // Change output (if needed)
    if change_sats > 546 {
        // Dust limit
        let change_addr = Address::from_str(change_address)
            .map_err(|e| MpcWalletError::Protocol(format!("Invalid change address: {}", e)))?
            .assume_checked();

        tx_outputs.push(TxOut {
            value: Amount::from_sat(change_sats),
            script_pubkey: change_addr.script_pubkey(),
        });
        output_data.push(TxOutput {
            address: change_address.to_string(),
            value: change_sats,
            is_change: true,
        });
    }

    // Create unsigned transaction
    let unsigned_tx = Transaction {
        version: Version::TWO,
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: tx_inputs,
        output: tx_outputs,
    };

    // Compute sighashes for each input
    let mut sighashes = Vec::new();

    let mut sighash_cache = SighashCache::new(&unsigned_tx);

    for (i, utxo) in selected_utxos.iter().enumerate() {
        let sighash = sighash_cache
            .p2wpkh_signature_hash(
                i,
                &ScriptBuf::from_bytes(sender_script_pubkey.to_vec()),
                Amount::from_sat(utxo.value),
                bitcoin::sighash::EcdsaSighashType::All,
            )
            .map_err(|e| MpcWalletError::Protocol(format!("Sighash error: {}", e)))?;

        sighashes.push(hex::encode(sighash.to_byte_array()));
    }

    Ok(UnsignedTransaction {
        inputs: input_data,
        outputs: output_data,
        total_input_sats: total_input,
        send_amount_sats: amount_sats,
        fee_sats,
        change_sats,
        unsigned_tx_hex: serialize_hex(&unsigned_tx),
        sighashes,
    })
}

/// Build an unsigned Taproot (P2TR) transaction.
///
/// This function builds a transaction spending from Taproot inputs and computes
/// BIP-341 sighashes for FROST/Schnorr signing.
pub fn build_unsigned_taproot_transaction(
    utxos: &[Utxo],
    to_address: &str,
    amount_sats: u64,
    fee_rate_sat_vb: u64,
    change_address: &str,
    sender_script_pubkey: &[u8],
) -> Result<UnsignedTransaction, MpcWalletError> {
    use bitcoin::consensus::encode::serialize_hex;
    use bitcoin::hashes::Hash;
    use bitcoin::sighash::{Prevouts, SighashCache, TapSighashType};
    use bitcoin::transaction::Version;
    use bitcoin::{
        Address, Amount, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Txid, Witness,
    };
    use std::str::FromStr;

    tracing::info!("Building Taproot transaction");
    tracing::debug!("UTXOs: {:?}", utxos);
    tracing::debug!("To: {}, Amount: {} sats", to_address, amount_sats);

    // Sort UTXOs by value (largest first) for efficient selection
    let mut sorted_utxos = utxos.to_vec();
    sorted_utxos.sort_by(|a, b| b.value.cmp(&a.value));

    // Estimate transaction size for fee calculation
    // P2TR: ~57.5 vB per input (key path spend), ~43 vB per output
    let base_size: u64 = 10; // version + locktime + overhead
    let input_size: u64 = 58; // P2TR key-path input (witness: 64-byte sig)
    let output_size: u64 = 43; // P2TR output

    // Select UTXOs (simple greedy algorithm)
    let mut selected_utxos: Vec<Utxo> = Vec::new();
    let mut total_input: u64 = 0;

    // Calculate minimum needed (amount + estimated fee for 1 input, 2 outputs)
    let min_fee = fee_rate_sat_vb * (base_size + input_size + 2 * output_size);
    let mut target = amount_sats + min_fee;

    for utxo in &sorted_utxos {
        if total_input >= target {
            break;
        }
        selected_utxos.push(utxo.clone());
        total_input += utxo.value;

        // Recalculate target with more inputs
        let estimated_size =
            base_size + (selected_utxos.len() as u64 * input_size) + (2 * output_size);
        let estimated_fee = fee_rate_sat_vb * estimated_size;
        target = amount_sats + estimated_fee;
    }

    if total_input < target {
        return Err(MpcWalletError::Protocol(format!(
            "Insufficient funds: have {} sats, need {} sats (including fee)",
            total_input, target
        )));
    }

    // Calculate actual fee
    let num_outputs = if total_input
        > amount_sats + (fee_rate_sat_vb * (base_size + input_size + 2 * output_size))
    {
        2 // Need change output
    } else {
        1 // No change
    };

    let tx_size =
        base_size + (selected_utxos.len() as u64 * input_size) + (num_outputs * output_size);
    let fee_sats = fee_rate_sat_vb * tx_size;
    let change_sats = total_input.saturating_sub(amount_sats + fee_sats);

    // Parse destination address
    let dest_address = Address::from_str(to_address)
        .map_err(|e| MpcWalletError::Protocol(format!("Invalid destination address: {}", e)))?
        .assume_checked();

    // Build transaction inputs
    let mut tx_inputs = Vec::new();
    let mut input_data = Vec::new();
    let mut prevouts = Vec::new(); // Needed for Taproot sighash

    let script_pubkey = ScriptBuf::from_bytes(sender_script_pubkey.to_vec());

    for utxo in &selected_utxos {
        let txid = Txid::from_str(&utxo.txid)
            .map_err(|e| MpcWalletError::Protocol(format!("Invalid txid: {}", e)))?;

        tx_inputs.push(TxIn {
            previous_output: OutPoint {
                txid,
                vout: utxo.vout,
            },
            script_sig: ScriptBuf::new(), // Empty for Taproot
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: Witness::default(), // Will be filled during signing
        });

        input_data.push(TxInput {
            txid: utxo.txid.clone(),
            vout: utxo.vout,
            value: utxo.value,
            script_pubkey: hex::encode(sender_script_pubkey),
        });

        // Collect prevouts for Taproot sighash (requires all prevouts)
        prevouts.push(TxOut {
            value: Amount::from_sat(utxo.value),
            script_pubkey: script_pubkey.clone(),
        });
    }

    // Build transaction outputs
    let mut tx_outputs = Vec::new();
    let mut output_data = Vec::new();

    // Destination output
    tx_outputs.push(TxOut {
        value: Amount::from_sat(amount_sats),
        script_pubkey: dest_address.script_pubkey(),
    });
    output_data.push(TxOutput {
        address: to_address.to_string(),
        value: amount_sats,
        is_change: false,
    });

    // Change output (if needed)
    if change_sats > 546 {
        // Dust limit
        let change_addr = Address::from_str(change_address)
            .map_err(|e| MpcWalletError::Protocol(format!("Invalid change address: {}", e)))?
            .assume_checked();

        tx_outputs.push(TxOut {
            value: Amount::from_sat(change_sats),
            script_pubkey: change_addr.script_pubkey(),
        });
        output_data.push(TxOutput {
            address: change_address.to_string(),
            value: change_sats,
            is_change: true,
        });
    }

    // Create unsigned transaction
    let unsigned_tx = Transaction {
        version: Version::TWO,
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: tx_inputs,
        output: tx_outputs,
    };

    // Compute Taproot sighashes for each input (BIP-341)
    let mut sighashes = Vec::new();
    let mut sighash_cache = SighashCache::new(&unsigned_tx);
    let prevouts_ref = Prevouts::All(&prevouts);

    for i in 0..selected_utxos.len() {
        let sighash = sighash_cache
            .taproot_key_spend_signature_hash(
                i,
                &prevouts_ref,
                TapSighashType::Default, // SIGHASH_DEFAULT (0x00) for Taproot
            )
            .map_err(|e| MpcWalletError::Protocol(format!("Taproot sighash error: {}", e)))?;

        tracing::debug!(
            "Input {} sighash: {}",
            i,
            hex::encode(sighash.to_byte_array())
        );
        sighashes.push(hex::encode(sighash.to_byte_array()));
    }

    tracing::info!(
        "Built unsigned Taproot tx with {} inputs, {} outputs, fee: {} sats",
        selected_utxos.len(),
        output_data.len(),
        fee_sats
    );

    Ok(UnsignedTransaction {
        inputs: input_data,
        outputs: output_data,
        total_input_sats: total_input,
        send_amount_sats: amount_sats,
        fee_sats,
        change_sats,
        unsigned_tx_hex: serialize_hex(&unsigned_tx),
        sighashes,
    })
}

/// Finalize a Taproot transaction by adding Schnorr signature witnesses.
///
/// For key-path spending, the witness is just the 64-byte Schnorr signature.
pub fn finalize_taproot_transaction(
    unsigned_tx_hex: &str,
    signatures: &[Vec<u8>], // 64-byte Schnorr signatures for each input
) -> Result<String, MpcWalletError> {
    use bitcoin::consensus::{deserialize, encode::serialize_hex};
    use bitcoin::Transaction;
    use bitcoin::Witness;

    // Deserialize the unsigned transaction
    let tx_bytes = hex::decode(unsigned_tx_hex)
        .map_err(|e| MpcWalletError::Protocol(format!("Invalid tx hex: {}", e)))?;

    let mut tx: Transaction = deserialize(&tx_bytes)
        .map_err(|e| MpcWalletError::Protocol(format!("Failed to deserialize tx: {}", e)))?;

    // Verify we have the right number of signatures
    if signatures.len() != tx.input.len() {
        return Err(MpcWalletError::Protocol(format!(
            "Expected {} signatures, got {}",
            tx.input.len(),
            signatures.len()
        )));
    }

    // Add Schnorr signature witnesses
    for (i, sig) in signatures.iter().enumerate() {
        if sig.len() != 64 {
            return Err(MpcWalletError::Protocol(format!(
                "Invalid signature length for input {}: expected 64, got {}",
                i,
                sig.len()
            )));
        }

        // For Taproot key-path spend with SIGHASH_DEFAULT, witness is just the 64-byte signature
        // (no sighash type byte appended when using SIGHASH_DEFAULT)
        let mut witness = Witness::new();
        witness.push(sig);
        tx.input[i].witness = witness;
    }

    Ok(serialize_hex(&tx))
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
