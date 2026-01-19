//! CGGMP24 Threshold ECDSA Protocol.
//!
//! This module implements the CGGMP24 protocol for threshold ECDSA signatures.
//! It includes:
//! - Pregenerated primes management
//! - Auxiliary info generation
//! - Distributed key generation
//! - Threshold signing

pub mod aux_info;
pub mod keygen;
pub mod primes;
pub mod runner;
pub mod signing;

pub use aux_info::*;
pub use keygen::*;
pub use primes::*;
pub use runner::*;
pub use signing::*;
