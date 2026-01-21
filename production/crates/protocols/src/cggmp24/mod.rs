//! CGGMP24 Threshold ECDSA Protocol.
//!
//! This module implements the CGGMP24 protocol for threshold ECDSA signatures.
//! It includes:
//! - Pregenerated primes management
//! - Auxiliary info generation
//! - Distributed key generation
//! - Threshold signing
//! - Presignature generation and pooling for fast signing

pub mod aux_info;
pub mod keygen;
pub mod presig_pool;
pub mod presignature;
pub mod primes;
pub mod runner;
pub mod signing;
pub mod signing_fast;

pub use aux_info::*;
pub use keygen::*;
pub use presig_pool::*;
pub use presignature::*;
pub use primes::*;
pub use runner::*;
pub use signing::*;
pub use signing_fast::*;
