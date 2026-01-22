//! Protocol Integration Layer
//!
//! This module provides integration stubs for actual cryptographic protocols:
//! - CGGMP24: Using the `cggmp24` crate for ECDSA threshold signatures
//! - FROST: Using the `givre` crate for Schnorr threshold signatures
//!
//! # Implementation Status
//!
//! All methods are currently stubs that return errors with TODO messages.
//! To complete the implementation:
//!
//! 1. Add dependencies to Cargo.toml:
//!    ```toml
//!    [dependencies]
//!    cggmp24 = { version = "0.7.0-alpha.3", features = ["rust-gmp-kzen"] }
//!    givre = { version = "0.2", features = ["full-signing", "serde", "ciphersuite-bitcoin"] }
//!    ```
//!
//! 2. Implement each function using the respective library
//! 3. Reference the threshold-signing and torcus-wallet codebases

pub mod cggmp24_integration;
pub mod frost_integration;

pub use cggmp24_integration::*;
pub use frost_integration::*;
