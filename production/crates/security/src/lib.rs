//! Security crate for mTLS certificate management.
//!
//! This crate provides certificate management, TLS configuration, and
//! QUIC integration for mutual TLS authentication in the MPC wallet network.
//!
//! # Features
//! - Certificate loading and validation (PEM format)
//! - Node ID extraction from certificate CN (format: "node-{id}")
//! - rustls ClientConfig and ServerConfig builders with mTLS
//! - quinn integration for QUIC with mTLS
//! - TLS 1.3 enforcement
//! - Certificate expiry checking
//!
//! # Example
//! ```no_run
//! use threshold_security::{CertificateManager, create_mesh_configs};
//!
//! # fn main() -> anyhow::Result<()> {
//! // Create configs for a mesh node
//! let (client_config, server_config) = create_mesh_configs(
//!     "/path/to/ca.pem",
//!     "/path/to/node.pem",
//!     "/path/to/node.key",
//!     true, // skip hostname verification for mesh network
//! )?;
//!
//! // Use these configs with quinn endpoints
//! # Ok(())
//! # }
//! ```

mod cert_manager;
mod tls_config;
mod integration;

// Re-export main types and functions
pub use cert_manager::CertificateManager;
pub use tls_config::{ClientConfigBuilder, ServerConfigBuilder};
pub use integration::{
    QuinnConfigBuilder,
    create_quinn_client_config,
    create_quinn_server_config,
    create_mesh_configs,
};
