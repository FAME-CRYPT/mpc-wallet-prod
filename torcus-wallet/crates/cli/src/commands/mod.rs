//! CLI command implementations.
//!
//! This module organizes commands by domain:
//! - `wallet`: Wallet CRUD and HD address derivation
//! - `bitcoin`: Balance, faucet, and legacy send
//! - `cggmp24`: CGGMP24 threshold ECDSA commands
//! - `taproot`: Taproot/FROST threshold Schnorr commands

pub mod bitcoin;
pub mod cggmp24;
pub mod taproot;
pub mod wallet;

// Re-export all command functions
pub use bitcoin::*;
pub use cggmp24::*;
pub use taproot::*;
pub use wallet::*;
