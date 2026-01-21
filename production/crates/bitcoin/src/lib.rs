//! Bitcoin integration for MPC wallet.
//!
//! This crate provides:
//! - Bitcoin blockchain API client (Esplora-compatible)
//! - Transaction building with UTXO selection
//! - OP_RETURN metadata embedding support (up to 80 bytes)
//! - Fee estimation and calculation
//! - Both SegWit (P2WPKH/ECDSA) and Taproot (P2TR/Schnorr) support
//!
//! # Integration Flow
//!
//! 1. **UTXO Fetching**: Use `BitcoinClient::get_utxos()` to fetch available UTXOs
//! 2. **Transaction Building**: Use `TransactionBuilder` to create unsigned transactions
//!    - Add payment outputs with `add_output()`
//!    - Optionally embed metadata with `add_op_return()`
//!    - Build for SegWit with `build_p2wpkh()` or Taproot with `build_p2tr()`
//! 3. **MPC Signing**: Pass sighashes to MPC signing protocol (CGGMP24 or FROST)
//! 4. **Transaction Finalization**: Combine signatures with unsigned transaction
//!    - For SegWit: `finalize_p2wpkh_transaction()`
//!    - For Taproot: `finalize_taproot_transaction()`
//! 5. **Broadcasting**: Use `BitcoinClient::broadcast_tx()` to broadcast to network
//! 6. **Confirmation**: Use `BitcoinClient::get_tx_confirmation()` to check status
//!
//! # Example: Building a transaction with OP_RETURN
//!
//! ```rust,no_run
//! use threshold_bitcoin::{BitcoinClient, BitcoinNetwork, TransactionBuilder};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create client
//! let client = BitcoinClient::new(BitcoinNetwork::Testnet)?;
//!
//! // Fetch UTXOs
//! let utxos = client.get_utxos("tb1q...").await?;
//!
//! // Build transaction with metadata
//! let metadata = b"Hello, Bitcoin!".to_vec();
//! let unsigned_tx = TransactionBuilder::new(
//!     utxos,
//!     "tb1qchange...".to_string(), // change address
//!     vec![0; 22], // sender script pubkey
//!     5, // fee rate (sat/vB)
//! )
//! .add_output("tb1qrecipient...".to_string(), 10_000)
//! .add_op_return(metadata)?
//! .build_p2wpkh()?;
//!
//! // ... MPC signing happens here ...
//!
//! // Broadcast
//! // let signed_tx_hex = finalize_p2wpkh_transaction(...);
//! // let txid = client.broadcast_tx(&signed_tx_hex).await?;
//! # Ok(())
//! # }
//! ```

pub mod client;
pub mod tx_builder;
pub mod types;

// Re-export main types for convenience
pub use client::{BitcoinClient, BitcoinError, BitcoinNetwork};
pub use tx_builder::{
    finalize_p2wpkh_transaction, finalize_taproot_transaction, TransactionBuilder,
    TxBuilderError, DUST_LIMIT, MAX_OP_RETURN_SIZE,
};
pub use types::{
    AddressInfo, BalanceResponse, BroadcastResult, ChainStats, FeeEstimates, MempoolStats,
    SendBitcoinRequest, SendBitcoinResponse, TxInput, TxOutput, UnsignedTransaction, Utxo,
    UtxoStatus,
};

/// Library version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_exports() {
        // Ensure all main types are exported
        let _ = BitcoinNetwork::Testnet;
        let _ = MAX_OP_RETURN_SIZE;
        let _ = DUST_LIMIT;
    }
}
