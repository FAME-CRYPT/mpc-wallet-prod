//! Bitcoin transaction building.
//!
//! Supports:
//! - SegWit (P2WPKH) transactions with ECDSA signatures
//! - Taproot (P2TR) transactions with Schnorr signatures

use common::MpcWalletError;
use serde::{Deserialize, Serialize};

use crate::bitcoin::client::Utxo;

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

// ============================================================================
// SegWit Transaction Building
// ============================================================================

/// Build an unsigned SegWit (P2WPKH) transaction.
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

// ============================================================================
// Taproot Transaction Building
// ============================================================================

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
