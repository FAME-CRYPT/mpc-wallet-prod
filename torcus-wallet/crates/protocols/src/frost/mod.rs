//! FROST Threshold Schnorr Protocol.
//!
//! This module implements the FROST protocol for threshold Schnorr signatures.
//! It includes:
//! - Distributed key generation
//! - Threshold signing (BIP-340 compatible for Taproot)

pub mod keygen;
pub mod signing;

// Explicit re-exports to avoid ambiguity
pub use keygen::{run_frost_keygen, FrostKeygenResult};
pub use signing::{run_frost_signing, FrostKeyShare, FrostSigningResult, SchnorrSignature};
