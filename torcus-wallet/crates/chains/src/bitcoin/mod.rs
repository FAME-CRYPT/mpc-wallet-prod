//! Bitcoin blockchain support.
//!
//! Includes:
//! - SegWit (P2WPKH) address derivation
//! - Taproot (P2TR) address derivation
//! - Transaction building and signing
//! - Blockchain API client (Esplora for testnet/mainnet)
//! - Bitcoin Core RPC client (for regtest)

pub mod address;
pub mod client;
pub mod hd;
pub mod rpc;
pub mod transaction;

pub use address::*;
pub use client::*;
pub use hd::*;
pub use rpc::*;
pub use transaction::*;
