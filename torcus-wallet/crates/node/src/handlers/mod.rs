//! HTTP request handlers for the MPC node.
//!
//! This module is organized into submodules:
//! - `health`: Health check and status endpoints
//! - `cggmp24`: CGGMP24 threshold ECDSA endpoints
//! - `frost`: FROST threshold Schnorr endpoints
//! - `p2p`: P2P session coordination endpoints (Phase 4)

pub mod cggmp24;
mod frost;
mod health;
pub mod p2p;

// Re-export all handlers
pub use cggmp24::*;
pub use frost::*;
pub use health::*;
pub use p2p::*;
