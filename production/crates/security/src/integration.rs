//! Integration layer between rustls and quinn for QUIC connections.
//!
//! This module provides helper functions to create quinn configurations
//! from rustls configurations, bridging the gap between the TLS and QUIC layers.

use quinn::{ClientConfig, ServerConfig};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};

use crate::cert_manager::CertificateManager;
use crate::tls_config::{ClientConfigBuilder, ServerConfigBuilder};

/// Default idle timeout for QUIC connections (60 seconds).
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// Default keep-alive interval (15 seconds).
const DEFAULT_KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(15);

/// Helper for creating quinn configurations with mTLS.
pub struct QuinnConfigBuilder {
    cert_manager: CertificateManager,
    idle_timeout: Duration,
    keep_alive_interval: Duration,
    skip_hostname_verification: bool,
}

impl QuinnConfigBuilder {
    /// Create a new quinn config builder.
    pub fn new(cert_manager: CertificateManager) -> Self {
        Self {
            cert_manager,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            keep_alive_interval: DEFAULT_KEEP_ALIVE_INTERVAL,
            skip_hostname_verification: false,
        }
    }

    /// Set the idle timeout for QUIC connections.
    pub fn with_idle_timeout(mut self, timeout: Duration) -> Self {
        self.idle_timeout = timeout;
        self
    }

    /// Set the keep-alive interval for QUIC connections.
    pub fn with_keep_alive_interval(mut self, interval: Duration) -> Self {
        self.keep_alive_interval = interval;
        self
    }

    /// Skip hostname verification in client connections.
    ///
    /// WARNING: This reduces security and should only be used in controlled environments.
    pub fn skip_hostname_verification(mut self) -> Self {
        self.skip_hostname_verification = true;
        self
    }

    /// Build a quinn client configuration.
    ///
    /// This creates a QUIC client config with:
    /// - mTLS authentication
    /// - Configurable idle timeout and keep-alive
    /// - h3 ALPN protocol support
    ///
    /// # Errors
    /// Returns an error if the configuration cannot be created.
    pub fn build_client_config(&self) -> anyhow::Result<ClientConfig> {
        info!("Building quinn client configuration with mTLS");

        // Create rustls client config
        let mut rustls_builder = ClientConfigBuilder::new(self.cert_manager.clone());
        if self.skip_hostname_verification {
            rustls_builder = rustls_builder.skip_hostname_verification();
        }

        let rustls_config = rustls_builder.build_with_alpn(vec![b"h3".to_vec()])?;

        // Convert to quinn config
        let mut config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(rustls_config)
                .map_err(|e| anyhow::anyhow!("Failed to create QUIC client config: {}", e))?,
        ));

        // Configure transport
        let mut transport = quinn::TransportConfig::default();
        transport.max_idle_timeout(Some(
            self.idle_timeout
                .try_into()
                .map_err(|e| anyhow::anyhow!("Invalid idle timeout: {}", e))?,
        ));
        transport.keep_alive_interval(Some(self.keep_alive_interval));
        config.transport_config(Arc::new(transport));

        debug!(
            "Quinn client config created (idle_timeout={:?}, keep_alive={:?})",
            self.idle_timeout, self.keep_alive_interval
        );

        Ok(config)
    }

    /// Build a quinn server configuration.
    ///
    /// This creates a QUIC server config with:
    /// - mTLS authentication (requires client certificates)
    /// - Configurable idle timeout
    /// - h3 ALPN protocol support
    ///
    /// # Errors
    /// Returns an error if the configuration cannot be created.
    pub fn build_server_config(&self) -> anyhow::Result<ServerConfig> {
        info!("Building quinn server configuration with mTLS");

        // Create rustls server config
        let rustls_config = ServerConfigBuilder::new(self.cert_manager.clone())
            .build_with_alpn(vec![b"h3".to_vec()])?;

        // Convert to quinn config
        let mut config = ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(rustls_config)
                .map_err(|e| anyhow::anyhow!("Failed to create QUIC server config: {}", e))?,
        ));

        // Configure transport
        let mut transport = quinn::TransportConfig::default();
        transport.max_idle_timeout(Some(
            self.idle_timeout
                .try_into()
                .map_err(|e| anyhow::anyhow!("Invalid idle timeout: {}", e))?,
        ));
        config.transport_config(Arc::new(transport));

        debug!("Quinn server config created (idle_timeout={:?})", self.idle_timeout);

        Ok(config)
    }
}

/// Convenience function to create a quinn client config from certificate paths.
///
/// # Arguments
/// * `ca_cert_path` - Path to CA certificate
/// * `client_cert_path` - Path to client certificate
/// * `client_key_path` - Path to client private key
/// * `skip_hostname_verification` - Whether to skip hostname verification
///
/// # Errors
/// Returns an error if the configuration cannot be created.
pub fn create_quinn_client_config(
    ca_cert_path: &str,
    client_cert_path: &str,
    client_key_path: &str,
    skip_hostname_verification: bool,
) -> anyhow::Result<ClientConfig> {
    let cert_manager = CertificateManager::new(ca_cert_path, client_cert_path, client_key_path);

    let mut builder = QuinnConfigBuilder::new(cert_manager);
    if skip_hostname_verification {
        builder = builder.skip_hostname_verification();
    }

    builder.build_client_config()
}

/// Convenience function to create a quinn server config from certificate paths.
///
/// # Arguments
/// * `ca_cert_path` - Path to CA certificate
/// * `server_cert_path` - Path to server certificate
/// * `server_key_path` - Path to server private key
///
/// # Errors
/// Returns an error if the configuration cannot be created.
pub fn create_quinn_server_config(
    ca_cert_path: &str,
    server_cert_path: &str,
    server_key_path: &str,
) -> anyhow::Result<ServerConfig> {
    let cert_manager = CertificateManager::new(ca_cert_path, server_cert_path, server_key_path);

    QuinnConfigBuilder::new(cert_manager).build_server_config()
}

/// Create both client and server configs from the same certificate paths.
///
/// This is useful when a node acts as both client and server in a mesh network.
///
/// # Arguments
/// * `ca_cert_path` - Path to CA certificate
/// * `node_cert_path` - Path to node certificate
/// * `node_key_path` - Path to node private key
/// * `skip_hostname_verification` - Whether to skip hostname verification in client
///
/// # Returns
/// A tuple of (ClientConfig, ServerConfig)
///
/// # Errors
/// Returns an error if either configuration cannot be created.
pub fn create_mesh_configs(
    ca_cert_path: &str,
    node_cert_path: &str,
    node_key_path: &str,
    skip_hostname_verification: bool,
) -> anyhow::Result<(ClientConfig, ServerConfig)> {
    info!("Creating mesh network configs for node");

    let cert_manager = CertificateManager::new(ca_cert_path, node_cert_path, node_key_path);

    let mut builder = QuinnConfigBuilder::new(cert_manager);
    if skip_hostname_verification {
        builder = builder.skip_hostname_verification();
    }

    let client_config = builder.build_client_config()?;
    let server_config = builder.build_server_config()?;

    info!("Mesh network configs created successfully");
    Ok((client_config, server_config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_timeouts() {
        assert_eq!(DEFAULT_IDLE_TIMEOUT.as_secs(), 60);
        assert_eq!(DEFAULT_KEEP_ALIVE_INTERVAL.as_secs(), 15);
    }

    #[test]
    fn test_builder_configuration() {
        // Test that builder methods work
        let cert_manager = CertificateManager::new("ca.pem", "cert.pem", "key.pem");
        let builder = QuinnConfigBuilder::new(cert_manager)
            .with_idle_timeout(Duration::from_secs(120))
            .with_keep_alive_interval(Duration::from_secs(30))
            .skip_hostname_verification();

        assert_eq!(builder.idle_timeout.as_secs(), 120);
        assert_eq!(builder.keep_alive_interval.as_secs(), 30);
        assert!(builder.skip_hostname_verification);
    }
}
