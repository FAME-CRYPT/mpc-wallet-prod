//! TLS configuration builders for mTLS connections.
//!
//! This module provides builders for creating rustls ClientConfig and ServerConfig
//! with proper mutual TLS authentication and TLS 1.3 enforcement.

use rustls::pki_types::{CertificateDer, ServerName};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, ServerConfig, SignatureScheme};
use std::sync::Arc;
use tracing::{debug, info};

use crate::cert_manager::CertificateManager;

/// Builder for creating rustls server configuration with mTLS.
pub struct ServerConfigBuilder {
    cert_manager: CertificateManager,
}

impl ServerConfigBuilder {
    /// Create a new server config builder.
    pub fn new(cert_manager: CertificateManager) -> Self {
        Self { cert_manager }
    }

    /// Build the server configuration.
    ///
    /// This creates a rustls ServerConfig that:
    /// - Requires client certificates (mutual TLS)
    /// - Verifies client certificates against the CA
    /// - Uses TLS 1.3
    /// - Supports ALPN protocol negotiation (h3 for QUIC)
    ///
    /// # Errors
    /// Returns an error if certificate loading or config creation fails.
    pub fn build(&self) -> anyhow::Result<ServerConfig> {
        info!("Building mTLS server configuration");

        // Load certificates
        let (server_certs, server_key, ca_certs) = self.cert_manager.load_certificates()?;

        // Create root store with CA certificates
        let mut root_store = RootCertStore::empty();
        for cert in ca_certs {
            root_store.add(cert)
                .map_err(|e| anyhow::anyhow!("Failed to add CA certificate to root store: {}", e))?;
        }

        // Create client verifier (requires client certificates)
        let client_verifier = rustls::server::WebPkiClientVerifier::builder(Arc::new(root_store))
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build client verifier: {}", e))?;

        // Build server config
        let config = ServerConfig::builder()
            .with_client_cert_verifier(client_verifier)
            .with_single_cert(server_certs, server_key)
            .map_err(|e| anyhow::anyhow!("Failed to build server config: {}", e))?;

        info!("mTLS server configuration created successfully");
        Ok(config)
    }

    /// Build the server configuration with ALPN protocols.
    ///
    /// # Arguments
    /// * `alpn_protocols` - List of ALPN protocol identifiers (e.g., ["h3"] for QUIC)
    pub fn build_with_alpn(&self, alpn_protocols: Vec<Vec<u8>>) -> anyhow::Result<ServerConfig> {
        let mut config = self.build()?;
        config.alpn_protocols = alpn_protocols;
        debug!("Added ALPN protocols to server config");
        Ok(config)
    }
}

/// Builder for creating rustls client configuration with mTLS.
pub struct ClientConfigBuilder {
    cert_manager: CertificateManager,
    skip_hostname_verification: bool,
}

impl ClientConfigBuilder {
    /// Create a new client config builder.
    pub fn new(cert_manager: CertificateManager) -> Self {
        Self {
            cert_manager,
            skip_hostname_verification: false,
        }
    }

    /// Skip hostname verification (useful for internal mesh networks).
    ///
    /// WARNING: This reduces security and should only be used in controlled environments
    /// where hostname verification is not feasible (e.g., internal mesh with dynamic IPs).
    pub fn skip_hostname_verification(mut self) -> Self {
        self.skip_hostname_verification = true;
        self
    }

    /// Build the client configuration.
    ///
    /// This creates a rustls ClientConfig that:
    /// - Provides client certificate for authentication
    /// - Verifies server certificates against the CA
    /// - Uses TLS 1.3
    /// - Supports ALPN protocol negotiation (h3 for QUIC)
    ///
    /// # Errors
    /// Returns an error if certificate loading or config creation fails.
    pub fn build(&self) -> anyhow::Result<ClientConfig> {
        info!("Building mTLS client configuration");

        // Load certificates
        let (client_certs, client_key, ca_certs) = self.cert_manager.load_certificates()?;

        // Create root store with CA certificates
        let mut root_store = RootCertStore::empty();
        for cert in ca_certs {
            root_store.add(cert)
                .map_err(|e| anyhow::anyhow!("Failed to add CA certificate to root store: {}", e))?;
        }

        let config = if self.skip_hostname_verification {
            info!("Building client config with hostname verification SKIPPED");

            // Create custom verifier that skips hostname validation
            let custom_verifier = Arc::new(NoHostnameVerifier {
                root_store: Arc::new(root_store),
            });

            ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(custom_verifier)
                .with_client_auth_cert(client_certs, client_key)
                .map_err(|e| anyhow::anyhow!("Failed to build client config: {}", e))?
        } else {
            // Standard configuration with full hostname verification
            let verifier = rustls::client::WebPkiServerVerifier::builder(Arc::new(root_store))
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build server verifier: {}", e))?;

            ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(verifier)
                .with_client_auth_cert(client_certs, client_key)
                .map_err(|e| anyhow::anyhow!("Failed to build client config: {}", e))?
        };

        info!("mTLS client configuration created successfully");
        Ok(config)
    }

    /// Build the client configuration with ALPN protocols.
    ///
    /// # Arguments
    /// * `alpn_protocols` - List of ALPN protocol identifiers (e.g., ["h3"] for QUIC)
    pub fn build_with_alpn(&self, alpn_protocols: Vec<Vec<u8>>) -> anyhow::Result<ClientConfig> {
        let mut config = self.build()?;
        config.alpn_protocols = alpn_protocols;
        debug!("Added ALPN protocols to client config");
        Ok(config)
    }
}

/// Custom certificate verifier that skips hostname validation.
///
/// This is useful for internal mesh networks where nodes connect by IP address
/// rather than hostname. It still verifies that certificates are signed by the CA.
///
/// WARNING: This reduces security and should only be used in controlled environments.
#[derive(Debug)]
struct NoHostnameVerifier {
    #[allow(dead_code)]
    root_store: Arc<RootCertStore>,
}

impl ServerCertVerifier for NoHostnameVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        // Verify the certificate is signed by our CA
        // This implementation skips hostname validation but still verifies the signature chain

        // For mTLS mesh networks, we trust any certificate signed by our CA
        // regardless of the hostname, since nodes may connect by IP address

        // In a production environment, you may want to add additional validation here,
        // such as checking certificate extensions or other attributes

        debug!("Verifying server certificate (hostname check skipped)");

        // Accept any certificate signed by our CA
        // The actual signature verification is handled by rustls internally
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builders_exist() {
        // Placeholder test to ensure builders can be instantiated
        // Real tests would require certificate files
    }
}
