//! Bitcoin transaction building utilities.
//!
//! Supports:
//! - Building unsigned transactions with UTXO selection
//! - Fee calculation and change handling
//! - OP_RETURN output support (up to 80 bytes of metadata)
//! - Both SegWit (P2WPKH) and Taproot (P2TR) transactions

use crate::types::{TxInput, TxOutput, UnsignedTransaction, Utxo};
use bitcoin::consensus::encode::serialize_hex;
use bitcoin::hashes::Hash;
use bitcoin::sighash::{Prevouts, SighashCache};
use bitcoin::transaction::Version;
use bitcoin::{
    absolute, Address, Amount, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Txid,
    Witness,
};
use std::str::FromStr;
use thiserror::Error;

/// Maximum size for OP_RETURN data (Bitcoin consensus rule).
pub const MAX_OP_RETURN_SIZE: usize = 80;

/// Dust limit in satoshis (546 sats for P2WPKH).
pub const DUST_LIMIT: u64 = 546;

/// Errors that can occur during transaction building.
#[derive(Debug, Error)]
pub enum TxBuilderError {
    #[error("Insufficient funds: have {available} sats, need {required} sats (including fee)")]
    InsufficientFunds { available: u64, required: u64 },

    #[error("Invalid destination address: {0}")]
    InvalidAddress(String),

    #[error("Invalid change address: {0}")]
    InvalidChangeAddress(String),

    #[error("Invalid TXID: {0}")]
    InvalidTxid(String),

    #[error("OP_RETURN data too large: {size} bytes (max {MAX_OP_RETURN_SIZE})")]
    OpReturnTooLarge { size: usize },

    #[error("Sighash computation error: {0}")]
    SighashError(String),

    #[error("No UTXOs available")]
    NoUtxos,

    #[error("Invalid signature length for input {input}: expected {expected}, got {actual}")]
    InvalidSignatureLength {
        input: usize,
        expected: usize,
        actual: usize,
    },

    #[error("Failed to deserialize transaction: {0}")]
    DeserializationError(String),
}

/// Builder for creating unsigned Bitcoin transactions.
pub struct TransactionBuilder {
    utxos: Vec<Utxo>,
    outputs: Vec<(String, u64)>, // (address, amount)
    op_return_data: Option<Vec<u8>>,
    fee_rate: u64, // sat/vB
    change_address: String,
    sender_script_pubkey: Vec<u8>,
}

impl TransactionBuilder {
    /// Create a new transaction builder.
    ///
    /// # Arguments
    /// * `utxos` - Available UTXOs to spend from
    /// * `change_address` - Address to send change to
    /// * `sender_script_pubkey` - Script pubkey of the sender's address (for sighash)
    /// * `fee_rate` - Fee rate in sat/vB
    pub fn new(
        utxos: Vec<Utxo>,
        change_address: String,
        sender_script_pubkey: Vec<u8>,
        fee_rate: u64,
    ) -> Self {
        Self {
            utxos,
            outputs: Vec::new(),
            op_return_data: None,
            fee_rate,
            change_address,
            sender_script_pubkey,
        }
    }

    /// Add a payment output.
    pub fn add_output(mut self, address: String, amount: u64) -> Self {
        self.outputs.push((address, amount));
        self
    }

    /// Add OP_RETURN output with metadata.
    ///
    /// # Arguments
    /// * `data` - Data to embed (max 80 bytes)
    ///
    /// # Errors
    /// Returns error if data exceeds 80 bytes.
    pub fn add_op_return(mut self, data: Vec<u8>) -> Result<Self, TxBuilderError> {
        if data.len() > MAX_OP_RETURN_SIZE {
            return Err(TxBuilderError::OpReturnTooLarge { size: data.len() });
        }
        self.op_return_data = Some(data);
        Ok(self)
    }

    /// Build an unsigned SegWit (P2WPKH) transaction.
    pub fn build_p2wpkh(self) -> Result<UnsignedTransaction, TxBuilderError> {
        build_unsigned_transaction(
            &self.utxos,
            &self.outputs,
            self.op_return_data.as_deref(),
            self.fee_rate,
            &self.change_address,
            &self.sender_script_pubkey,
        )
    }

    /// Build an unsigned Taproot (P2TR) transaction.
    pub fn build_p2tr(self) -> Result<UnsignedTransaction, TxBuilderError> {
        build_unsigned_taproot_transaction(
            &self.utxos,
            &self.outputs,
            self.op_return_data.as_deref(),
            self.fee_rate,
            &self.change_address,
            &self.sender_script_pubkey,
        )
    }
}

/// Build an unsigned SegWit (P2WPKH) transaction.
fn build_unsigned_transaction(
    utxos: &[Utxo],
    outputs: &[(String, u64)],
    op_return_data: Option<&[u8]>,
    fee_rate_sat_vb: u64,
    change_address: &str,
    sender_script_pubkey: &[u8],
) -> Result<UnsignedTransaction, TxBuilderError> {
    if utxos.is_empty() {
        return Err(TxBuilderError::NoUtxos);
    }

    // Calculate total output amount
    let total_output: u64 = outputs.iter().map(|(_, amt)| amt).sum();

    // Sort UTXOs by value (largest first) for efficient selection
    let mut sorted_utxos = utxos.to_vec();
    sorted_utxos.sort_by(|a, b| b.value.cmp(&a.value));

    // Estimate transaction size for fee calculation
    // P2WPKH: ~110 vB for 1 input, ~68 vB per additional input, ~31 vB per output
    let base_size: u64 = 10; // version + locktime + overhead
    let input_size: u64 = 68; // P2WPKH input (witness data)
    let output_size: u64 = 31; // P2WPKH output
    let op_return_size: u64 = if let Some(data) = op_return_data {
        43 + data.len() as u64 // OP_RETURN output size
    } else {
        0
    };

    // Select UTXOs (greedy algorithm)
    let mut selected_utxos: Vec<Utxo> = Vec::new();
    let mut total_input: u64 = 0;

    // Calculate minimum needed (amount + estimated fee)
    let mut num_outputs = outputs.len() as u64 + 1; // +1 for potential change
    if op_return_data.is_some() {
        num_outputs += 1;
    }

    let min_fee = fee_rate_sat_vb * (base_size + input_size + num_outputs * output_size + op_return_size);
    let mut target = total_output + min_fee;

    for utxo in &sorted_utxos {
        if total_input >= target {
            break;
        }
        selected_utxos.push(utxo.clone());
        total_input += utxo.value;

        // Recalculate target with more inputs
        let estimated_size = base_size
            + (selected_utxos.len() as u64 * input_size)
            + (num_outputs * output_size)
            + op_return_size;
        let estimated_fee = fee_rate_sat_vb * estimated_size;
        target = total_output + estimated_fee;
    }

    if total_input < target {
        return Err(TxBuilderError::InsufficientFunds {
            available: total_input,
            required: target,
        });
    }

    // Calculate actual fee and change
    let mut actual_num_outputs = outputs.len() as u64;
    if op_return_data.is_some() {
        actual_num_outputs += 1;
    }

    let tx_size = base_size
        + (selected_utxos.len() as u64 * input_size)
        + (actual_num_outputs * output_size)
        + output_size // Potential change output
        + op_return_size;
    let fee_sats = fee_rate_sat_vb * tx_size;
    let change_sats = total_input.saturating_sub(total_output + fee_sats);

    // Build transaction inputs
    let mut tx_inputs = Vec::new();
    let mut input_data = Vec::new();

    for utxo in &selected_utxos {
        let txid =
            Txid::from_str(&utxo.txid).map_err(|e| TxBuilderError::InvalidTxid(e.to_string()))?;

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

    // Payment outputs
    for (address, amount) in outputs {
        let dest_address = Address::from_str(address)
            .map_err(|e| TxBuilderError::InvalidAddress(e.to_string()))?
            .assume_checked();

        tx_outputs.push(TxOut {
            value: Amount::from_sat(*amount),
            script_pubkey: dest_address.script_pubkey(),
        });
        output_data.push(TxOutput {
            address: address.clone(),
            value: *amount,
            is_change: false,
        });
    }

    // OP_RETURN output (if requested)
    if let Some(data) = op_return_data {
        let op_return_script = create_op_return_script(data);
        tx_outputs.push(TxOut {
            value: Amount::ZERO,
            script_pubkey: op_return_script,
        });
        output_data.push(TxOutput {
            address: format!("OP_RETURN({})", hex::encode(data)),
            value: 0,
            is_change: false,
        });
    }

    // Change output (if needed)
    if change_sats > DUST_LIMIT {
        let change_addr = Address::from_str(change_address)
            .map_err(|e| TxBuilderError::InvalidChangeAddress(e.to_string()))?
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
        lock_time: absolute::LockTime::ZERO,
        input: tx_inputs,
        output: tx_outputs,
    };

    // Compute sighashes for each input (P2WPKH)
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
            .map_err(|e| TxBuilderError::SighashError(e.to_string()))?;

        sighashes.push(hex::encode(sighash.to_byte_array()));
    }

    Ok(UnsignedTransaction {
        inputs: input_data,
        outputs: output_data,
        total_input_sats: total_input,
        send_amount_sats: total_output,
        fee_sats,
        change_sats,
        unsigned_tx_hex: serialize_hex(&unsigned_tx),
        sighashes,
    })
}

/// Build an unsigned Taproot (P2TR) transaction.
fn build_unsigned_taproot_transaction(
    utxos: &[Utxo],
    outputs: &[(String, u64)],
    op_return_data: Option<&[u8]>,
    fee_rate_sat_vb: u64,
    change_address: &str,
    sender_script_pubkey: &[u8],
) -> Result<UnsignedTransaction, TxBuilderError> {
    if utxos.is_empty() {
        return Err(TxBuilderError::NoUtxos);
    }

    // Calculate total output amount
    let total_output: u64 = outputs.iter().map(|(_, amt)| amt).sum();

    // Sort UTXOs by value (largest first)
    let mut sorted_utxos = utxos.to_vec();
    sorted_utxos.sort_by(|a, b| b.value.cmp(&a.value));

    // Estimate transaction size for fee calculation
    // P2TR: ~57.5 vB per input (key path spend), ~43 vB per output
    let base_size: u64 = 10;
    let input_size: u64 = 58; // P2TR key-path input
    let output_size: u64 = 43; // P2TR output
    let op_return_size: u64 = if let Some(data) = op_return_data {
        43 + data.len() as u64
    } else {
        0
    };

    // Select UTXOs
    let mut selected_utxos: Vec<Utxo> = Vec::new();
    let mut total_input: u64 = 0;

    let mut num_outputs = outputs.len() as u64 + 1; // +1 for potential change
    if op_return_data.is_some() {
        num_outputs += 1;
    }

    let min_fee = fee_rate_sat_vb * (base_size + input_size + num_outputs * output_size + op_return_size);
    let mut target = total_output + min_fee;

    for utxo in &sorted_utxos {
        if total_input >= target {
            break;
        }
        selected_utxos.push(utxo.clone());
        total_input += utxo.value;

        let estimated_size = base_size
            + (selected_utxos.len() as u64 * input_size)
            + (num_outputs * output_size)
            + op_return_size;
        let estimated_fee = fee_rate_sat_vb * estimated_size;
        target = total_output + estimated_fee;
    }

    if total_input < target {
        return Err(TxBuilderError::InsufficientFunds {
            available: total_input,
            required: target,
        });
    }

    // Calculate actual fee and change
    let mut actual_num_outputs = outputs.len() as u64;
    if op_return_data.is_some() {
        actual_num_outputs += 1;
    }

    let tx_size = base_size
        + (selected_utxos.len() as u64 * input_size)
        + (actual_num_outputs * output_size)
        + output_size // Potential change
        + op_return_size;
    let fee_sats = fee_rate_sat_vb * tx_size;
    let change_sats = total_input.saturating_sub(total_output + fee_sats);

    // Build transaction inputs
    let mut tx_inputs = Vec::new();
    let mut input_data = Vec::new();
    let mut prevouts = Vec::new(); // Needed for Taproot sighash

    let script_pubkey = ScriptBuf::from_bytes(sender_script_pubkey.to_vec());

    for utxo in &selected_utxos {
        let txid =
            Txid::from_str(&utxo.txid).map_err(|e| TxBuilderError::InvalidTxid(e.to_string()))?;

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

        // Collect prevouts for Taproot sighash
        prevouts.push(TxOut {
            value: Amount::from_sat(utxo.value),
            script_pubkey: script_pubkey.clone(),
        });
    }

    // Build transaction outputs
    let mut tx_outputs = Vec::new();
    let mut output_data = Vec::new();

    // Payment outputs
    for (address, amount) in outputs {
        let dest_address = Address::from_str(address)
            .map_err(|e| TxBuilderError::InvalidAddress(e.to_string()))?
            .assume_checked();

        tx_outputs.push(TxOut {
            value: Amount::from_sat(*amount),
            script_pubkey: dest_address.script_pubkey(),
        });
        output_data.push(TxOutput {
            address: address.clone(),
            value: *amount,
            is_change: false,
        });
    }

    // OP_RETURN output (if requested)
    if let Some(data) = op_return_data {
        let op_return_script = create_op_return_script(data);
        tx_outputs.push(TxOut {
            value: Amount::ZERO,
            script_pubkey: op_return_script,
        });
        output_data.push(TxOutput {
            address: format!("OP_RETURN({})", hex::encode(data)),
            value: 0,
            is_change: false,
        });
    }

    // Change output (if needed)
    if change_sats > DUST_LIMIT {
        let change_addr = Address::from_str(change_address)
            .map_err(|e| TxBuilderError::InvalidChangeAddress(e.to_string()))?
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
        lock_time: absolute::LockTime::ZERO,
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
                bitcoin::sighash::TapSighashType::Default,
            )
            .map_err(|e| TxBuilderError::SighashError(e.to_string()))?;

        sighashes.push(hex::encode(sighash.to_byte_array()));
    }

    Ok(UnsignedTransaction {
        inputs: input_data,
        outputs: output_data,
        total_input_sats: total_input,
        send_amount_sats: total_output,
        fee_sats,
        change_sats,
        unsigned_tx_hex: serialize_hex(&unsigned_tx),
        sighashes,
    })
}

/// Create an OP_RETURN script for embedding data.
///
/// OP_RETURN scripts are used to store arbitrary data in the blockchain.
/// They are provably unspendable and cost no UTXO set resources.
fn create_op_return_script(data: &[u8]) -> ScriptBuf {
    use bitcoin::opcodes;
    use bitcoin::script::PushBytesBuf;

    let mut builder = bitcoin::script::Builder::new()
        .push_opcode(opcodes::all::OP_RETURN);

    // Convert data to PushBytesBuf for push_slice
    let push_bytes = PushBytesBuf::try_from(data.to_vec())
        .expect("OP_RETURN data should be validated before this point");

    builder = builder.push_slice(&push_bytes);

    builder.into_script()
}

/// Finalize a SegWit transaction by adding ECDSA signature witnesses.
pub fn finalize_p2wpkh_transaction(
    unsigned_tx_hex: &str,
    signatures: &[Vec<u8>],      // DER-encoded ECDSA signatures
    public_keys: &[Vec<u8>],     // Compressed public keys
) -> Result<String, TxBuilderError> {
    use bitcoin::consensus::deserialize;

    // Deserialize the unsigned transaction
    let tx_bytes = hex::decode(unsigned_tx_hex)
        .map_err(|e| TxBuilderError::DeserializationError(e.to_string()))?;

    let mut tx: Transaction = deserialize(&tx_bytes)
        .map_err(|e| TxBuilderError::DeserializationError(e.to_string()))?;

    // Verify we have the right number of signatures and public keys
    if signatures.len() != tx.input.len() || public_keys.len() != tx.input.len() {
        return Err(TxBuilderError::InvalidSignatureLength {
            input: 0,
            expected: tx.input.len(),
            actual: signatures.len(),
        });
    }

    // Add ECDSA signature witnesses (P2WPKH format: <signature> <pubkey>)
    for (i, (sig, pubkey)) in signatures.iter().zip(public_keys.iter()).enumerate() {
        let mut witness = Witness::new();
        witness.push(sig);
        witness.push(pubkey);
        tx.input[i].witness = witness;
    }

    Ok(serialize_hex(&tx))
}

/// Finalize a Taproot transaction by adding Schnorr signature witnesses.
pub fn finalize_taproot_transaction(
    unsigned_tx_hex: &str,
    signatures: &[Vec<u8>], // 64-byte Schnorr signatures
) -> Result<String, TxBuilderError> {
    use bitcoin::consensus::deserialize;

    // Deserialize the unsigned transaction
    let tx_bytes = hex::decode(unsigned_tx_hex)
        .map_err(|e| TxBuilderError::DeserializationError(e.to_string()))?;

    let mut tx: Transaction = deserialize(&tx_bytes)
        .map_err(|e| TxBuilderError::DeserializationError(e.to_string()))?;

    // Verify we have the right number of signatures
    if signatures.len() != tx.input.len() {
        return Err(TxBuilderError::InvalidSignatureLength {
            input: 0,
            expected: tx.input.len(),
            actual: signatures.len(),
        });
    }

    // Add Schnorr signature witnesses (Taproot key-path: just the 64-byte signature)
    for (i, sig) in signatures.iter().enumerate() {
        if sig.len() != 64 {
            return Err(TxBuilderError::InvalidSignatureLength {
                input: i,
                expected: 64,
                actual: sig.len(),
            });
        }

        // For Taproot key-path spend with SIGHASH_DEFAULT,
        // witness is just the 64-byte signature (no sighash type byte)
        let mut witness = Witness::new();
        witness.push(sig);
        tx.input[i].witness = witness;
    }

    Ok(serialize_hex(&tx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_op_return_max_size() {
        let data = vec![0u8; MAX_OP_RETURN_SIZE];
        let script = create_op_return_script(&data);
        assert!(script.is_op_return());
    }

    #[test]
    fn test_op_return_builder() {
        let utxos = vec![Utxo {
            txid: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            vout: 0,
            value: 100_000,
            status: Default::default(),
        }];

        let builder = TransactionBuilder::new(
            utxos,
            "tb1qtest".to_string(),
            vec![0; 22],
            1,
        );

        let data = b"Hello, Bitcoin!".to_vec();
        let result = builder.add_op_return(data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_op_return_too_large() {
        let utxos = vec![Utxo {
            txid: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            vout: 0,
            value: 100_000,
            status: Default::default(),
        }];

        let builder = TransactionBuilder::new(
            utxos,
            "tb1qtest".to_string(),
            vec![0; 22],
            1,
        );

        let data = vec![0u8; MAX_OP_RETURN_SIZE + 1];
        let result = builder.add_op_return(data);
        assert!(result.is_err());
    }

    #[test]
    fn test_dust_limit() {
        assert_eq!(DUST_LIMIT, 546);
    }
}
