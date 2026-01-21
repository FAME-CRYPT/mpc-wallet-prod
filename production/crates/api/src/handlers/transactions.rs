//! Transaction business logic handlers

use chrono::Utc;
use threshold_bitcoin::{BitcoinClient, TransactionBuilder, Utxo, UtxoStatus};
use threshold_storage::PostgresStorage;
use threshold_types::{Transaction, TransactionState, TxId};
use tracing::{error, info, warn};

use crate::error::ApiError;

/// Create a new Bitcoin transaction with optional OP_RETURN metadata
pub async fn create_transaction(
    postgres: &PostgresStorage,
    bitcoin: &BitcoinClient,
    recipient: &str,
    amount_sats: u64,
    metadata: Option<&str>,
) -> Result<Transaction, ApiError> {
    info!(
        "Creating transaction: recipient={} amount={} metadata={:?}",
        recipient, amount_sats, metadata
    );

    // Get fee estimates from Bitcoin network
    let fee_estimates = bitcoin
        .get_fee_estimates()
        .await
        .unwrap_or_default();

    // Use recommended fee rate (medium priority)
    let fee_rate = if fee_estimates.medium > 0.0 {
        fee_estimates.medium.ceil() as u64
    } else {
        5 // Default to 5 sat/vB if API fails
    };

    info!("Using fee rate: {} sat/vB", fee_rate);

    // For demo/testing: Create a realistic unsigned Bitcoin transaction
    // In production with a real wallet, we would call bitcoin.get_utxos(wallet_address).await
    // Since we don't have a funded wallet, we create a demo transaction with realistic structure

    // Demo UTXO (simulating a previous transaction output)
    // In production, this would come from: bitcoin.get_utxos(wallet_address).await
    let demo_utxo = Utxo {
        txid: "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
        vout: 0,
        value: amount_sats + 10_000, // Enough to cover amount + fees
        status: UtxoStatus {
            confirmed: true,
            block_height: Some(2_500_000),
        },
    };

    warn!(
        "Using demo UTXO for development. In production, fetch real UTXOs from wallet address."
    );

    // Demo change address (in production, this would be a real address from the MPC wallet)
    let change_address = "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx".to_string(); // Demo testnet address

    // Demo script pubkey for P2WPKH
    // For P2WPKH, we need the actual P2WPKH script pubkey from the address
    // Format: OP_0 <20-byte-pubkey-hash>
    // tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx decodes to:
    // Version 0 + pubkey hash 751e76e8199196d454941c45d1b3a323f1433bd6
    let sender_script_pubkey = hex::decode("0014751e76e8199196d454941c45d1b3a323f1433bd6")
        .expect("Valid P2WPKH script pubkey");

    // Build unsigned Bitcoin transaction using TransactionBuilder
    let mut builder = TransactionBuilder::new(
        vec![demo_utxo],
        change_address,
        sender_script_pubkey,
        fee_rate,
    );

    // Add recipient output
    builder = builder.add_output(recipient.to_string(), amount_sats);

    // Add OP_RETURN metadata if provided
    if let Some(meta) = metadata {
        let metadata_bytes = meta.as_bytes().to_vec();
        builder = builder
            .add_op_return(metadata_bytes)
            .map_err(|e| ApiError::BadRequest(format!("Invalid metadata: {}", e)))?;
    }

    // Build unsigned transaction (P2WPKH/SegWit)
    let unsigned_transaction = builder
        .build_p2wpkh()
        .map_err(|e| {
            error!("Failed to build Bitcoin transaction: {}", e);
            ApiError::InternalError(format!("Failed to build transaction: {}", e))
        })?;

    info!(
        "Built unsigned transaction: total_input={} sats, send_amount={} sats, fee={} sats, change={} sats",
        unsigned_transaction.total_input_sats,
        unsigned_transaction.send_amount_sats,
        unsigned_transaction.fee_sats,
        unsigned_transaction.change_sats
    );

    // Decode the unsigned transaction hex to get the actual txid
    let tx_bytes = hex::decode(&unsigned_transaction.unsigned_tx_hex)
        .map_err(|e| ApiError::InternalError(format!("Failed to decode transaction hex: {}", e)))?;

    // Compute txid using Bitcoin's double SHA256
    use sha2::{Digest, Sha256};
    let hash1 = Sha256::digest(&tx_bytes);
    let hash2 = Sha256::digest(hash1);

    // Reverse bytes for txid (Bitcoin uses little-endian)
    let mut txid_bytes = hash2.to_vec();
    txid_bytes.reverse();
    let txid_hex = hex::encode(&txid_bytes);

    let txid = TxId::from(txid_hex.clone());

    info!("Generated transaction ID: {}", txid_hex);

    // Create transaction record with REAL unsigned Bitcoin transaction
    let tx = Transaction {
        id: 0, // Will be set by database
        txid: txid.clone(),
        state: TransactionState::Pending,
        unsigned_tx: tx_bytes, // Real Bitcoin transaction bytes
        signed_tx: None,
        recipient: recipient.to_string(),
        amount_sats,
        fee_sats: unsigned_transaction.fee_sats,
        metadata: metadata.map(|s| s.to_string()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    // Store transaction in database
    match postgres.create_transaction(&tx).await {
        Ok(id) => {
            info!("Transaction created successfully: id={} txid={}", id, txid);
            Ok(Transaction { id, ..tx })
        }
        Err(e) => {
            error!("Failed to store transaction in PostgreSQL: {:?}", e);
            Err(ApiError::from(e))
        }
    }
}

/// List all transactions from the database
pub async fn list_transactions(
    _postgres: &PostgresStorage,
) -> Result<Vec<Transaction>, ApiError> {
    // In production, this would support pagination and filtering
    // For now, we'll return a simple implementation

    info!("Listing all transactions");

    // Since PostgresStorage doesn't have a list_all method yet,
    // we'll return an empty list for now
    // In production, you would implement a proper query
    Ok(vec![])
}
