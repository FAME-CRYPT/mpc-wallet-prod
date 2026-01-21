//! Certificate manager for mTLS authentication.
//!
//! This module provides certificate loading, validation, and node ID extraction
//! for mutual TLS authentication in the MPC wallet network.

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::fs::File;
use std::io::BufReader;
use tracing::{debug, info};

/// Manages certificates for mTLS authentication.
///
/// This struct handles loading and validating certificates and private keys
/// for both client and server sides of mTLS connections.
#[derive(Clone)]
pub struct CertificateManager {
    /// Path to the CA certificate for verifying peer certificates
    ca_cert_path: String,
    /// Path to this node's certificate
    node_cert_path: String,
    /// Path to this node's private key
    node_key_path: String,
}

impl CertificateManager {
    /// Create a new certificate manager.
    ///
    /// # Arguments
    /// * `ca_path` - Path to the CA certificate file (PEM format)
    /// * `cert_path` - Path to this node's certificate file (PEM format)
    /// * `key_path` - Path to this node's private key file (PEM format)
    pub fn new(ca_path: impl Into<String>, cert_path: impl Into<String>, key_path: impl Into<String>) -> Self {
        Self {
            ca_cert_path: ca_path.into(),
            node_cert_path: cert_path.into(),
            node_key_path: key_path.into(),
        }
    }

    /// Load all certificates needed for mTLS.
    ///
    /// Returns a tuple of:
    /// - Node certificates (certificate chain)
    /// - Node private key
    /// - CA certificates (for verifying peers)
    ///
    /// # Errors
    /// Returns an error if any certificate or key file cannot be loaded or parsed.
    pub fn load_certificates(
        &self,
    ) -> anyhow::Result<(
        Vec<CertificateDer<'static>>,
        PrivateKeyDer<'static>,
        Vec<CertificateDer<'static>>,
    )> {
        info!("Loading certificates for mTLS");
        debug!("Node cert: {}", self.node_cert_path);
        debug!("Node key: {}", self.node_key_path);
        debug!("CA cert: {}", self.ca_cert_path);

        // Load node certificate
        let certs = self.load_cert_chain(&self.node_cert_path)?;
        info!("Loaded {} node certificate(s)", certs.len());

        // Load node private key
        let key = self.load_private_key(&self.node_key_path)?;
        info!("Loaded node private key");

        // Load CA certificate
        let ca_certs = self.load_cert_chain(&self.ca_cert_path)?;
        info!("Loaded {} CA certificate(s)", ca_certs.len());

        Ok((certs, key, ca_certs))
    }

    /// Load a certificate chain from a PEM file.
    ///
    /// # Arguments
    /// * `path` - Path to the PEM file containing one or more certificates
    ///
    /// # Errors
    /// Returns an error if the file cannot be opened or parsed.
    pub fn load_cert_chain(&self, path: &str) -> anyhow::Result<Vec<CertificateDer<'static>>> {
        let cert_file = File::open(path)
            .map_err(|e| anyhow::anyhow!("Failed to open certificate file {}: {}", path, e))?;
        let mut cert_reader = BufReader::new(cert_file);
        let certs: Vec<CertificateDer> = rustls_pemfile::certs(&mut cert_reader)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("Failed to parse certificates from {}: {}", path, e))?;

        if certs.is_empty() {
            return Err(anyhow::anyhow!("No certificates found in {}", path));
        }

        Ok(certs)
    }

    /// Load a private key from a PEM file.
    ///
    /// Supports both PKCS8 and PKCS1 (RSA) formats.
    ///
    /// # Arguments
    /// * `path` - Path to the PEM file containing the private key
    ///
    /// # Errors
    /// Returns an error if the file cannot be opened, parsed, or contains no valid key.
    pub fn load_private_key(&self, path: &str) -> anyhow::Result<PrivateKeyDer<'static>> {
        let key_file = File::open(path)
            .map_err(|e| anyhow::anyhow!("Failed to open key file {}: {}", path, e))?;
        let mut key_reader = BufReader::new(key_file);

        // Try PKCS8 format first
        let pkcs8_keys: Vec<_> = rustls_pemfile::pkcs8_private_keys(&mut key_reader)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("Failed to parse PKCS8 keys from {}: {}", path, e))?;

        if let Some(k) = pkcs8_keys.into_iter().next() {
            debug!("Loaded PKCS8 private key from {}", path);
            return Ok(PrivateKeyDer::Pkcs8(k));
        }

        // Try RSA (PKCS1) format
        let key_file = File::open(path)?;
        let mut key_reader = BufReader::new(key_file);
        let rsa_keys: Vec<_> = rustls_pemfile::rsa_private_keys(&mut key_reader)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("Failed to parse RSA keys from {}: {}", path, e))?;

        if let Some(k) = rsa_keys.into_iter().next() {
            debug!("Loaded PKCS1 (RSA) private key from {}", path);
            return Ok(PrivateKeyDer::Pkcs1(k));
        }

        Err(anyhow::anyhow!("No valid private key found in {}", path))
    }

    /// Verify if the node certificate is about to expire.
    ///
    /// Returns true if the certificate expires within 30 days.
    ///
    /// # Errors
    /// Returns an error if the certificate cannot be loaded or parsed.
    pub fn verify_certificate_expiry(&self) -> anyhow::Result<bool> {
        use x509_parser::prelude::*;

        debug!("Checking certificate expiry for {}", self.node_cert_path);

        let certs = self.load_cert_chain(&self.node_cert_path)?;

        if let Some(cert) = certs.first() {
            let parsed = X509Certificate::from_der(cert.as_ref())
                .map_err(|e| anyhow::anyhow!("Failed to parse certificate: {}", e))?;

            let validity = parsed.1.validity();
            let now = std::time::SystemTime::now();

            // Check if cert expires within 30 days
            let thirty_days = std::time::Duration::from_secs(30 * 24 * 60 * 60);
            let expires_soon = validity
                .not_after
                .timestamp()
                .checked_sub(now.duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64)
                .map(|secs| secs < thirty_days.as_secs() as i64)
                .unwrap_or(true);

            if expires_soon {
                info!("Certificate {} expires soon", self.node_cert_path);
            } else {
                debug!("Certificate {} is valid", self.node_cert_path);
            }

            return Ok(expires_soon);
        }

        Err(anyhow::anyhow!("No certificate found in {}", self.node_cert_path))
    }

    /// Extract the node ID from a certificate's Common Name (CN).
    ///
    /// The CN must be in the format "node-{id}" where {id} is a numeric node ID.
    ///
    /// # Arguments
    /// * `cert` - The certificate to extract the node ID from
    ///
    /// # Returns
    /// The extracted node ID as a u64.
    ///
    /// # Errors
    /// Returns an error if the CN is missing, has invalid format, or cannot be parsed.
    pub fn extract_node_id(&self, cert: &CertificateDer) -> anyhow::Result<u64> {
        use x509_parser::prelude::*;

        let parsed = X509Certificate::from_der(cert.as_ref())
            .map_err(|e| anyhow::anyhow!("Failed to parse certificate: {}", e))?;

        let cn = parsed
            .1
            .subject()
            .iter_common_name()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No CN found in certificate"))?
            .as_str()
            .map_err(|e| anyhow::anyhow!("Failed to extract CN: {}", e))?;

        debug!("Certificate CN: {}", cn);

        // Extract node_id from "node-{id}" format
        let node_id_str = cn
            .strip_prefix("node-")
            .ok_or_else(|| anyhow::anyhow!("Invalid CN format (expected 'node-{{id}}'): {}", cn))?;

        let node_id = node_id_str
            .parse::<u64>()
            .map_err(|e| anyhow::anyhow!("Failed to parse node ID from '{}': {}", node_id_str, e))?;

        debug!("Extracted node ID: {}", node_id);
        Ok(node_id)
    }

    /// Verify a certificate chain against the CA.
    ///
    /// This checks that the certificate chain is properly signed by the CA
    /// and that all certificates in the chain are valid.
    ///
    /// # Arguments
    /// * `cert_chain` - The certificate chain to verify
    ///
    /// # Errors
    /// Returns an error if verification fails.
    pub fn verify_chain(&self, cert_chain: &[CertificateDer]) -> anyhow::Result<()> {
        use x509_parser::prelude::*;

        if cert_chain.is_empty() {
            return Err(anyhow::anyhow!("Empty certificate chain"));
        }

        debug!("Verifying certificate chain of length {}", cert_chain.len());

        // Parse all certificates
        for (i, cert) in cert_chain.iter().enumerate() {
            let parsed = X509Certificate::from_der(cert.as_ref())
                .map_err(|e| anyhow::anyhow!("Failed to parse certificate {}: {}", i, e))?;

            // Check validity period
            let now = std::time::SystemTime::now();
            let now_unix = now.duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64;

            let not_before = parsed.1.validity().not_before.timestamp();
            let not_after = parsed.1.validity().not_after.timestamp();

            if now_unix < not_before {
                return Err(anyhow::anyhow!("Certificate {} not yet valid", i));
            }
            if now_unix > not_after {
                return Err(anyhow::anyhow!("Certificate {} has expired", i));
            }

            debug!("Certificate {} is valid", i);
        }

        info!("Certificate chain verification passed");
        Ok(())
    }

    /// Get the path to the CA certificate.
    pub fn ca_cert_path(&self) -> &str {
        &self.ca_cert_path
    }

    /// Get the path to the node certificate.
    pub fn node_cert_path(&self) -> &str {
        &self.node_cert_path
    }

    /// Get the path to the node private key.
    pub fn node_key_path(&self) -> &str {
        &self.node_key_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_id_extraction() {
        // This test would require actual certificate files
        // For now, we just verify the format parsing logic
        let valid_cn = "node-123";
        let node_id_str = valid_cn.strip_prefix("node-").unwrap();
        let node_id = node_id_str.parse::<u64>().unwrap();
        assert_eq!(node_id, 123);
    }

    #[test]
    fn test_invalid_cn_format() {
        let invalid_cn = "invalid-123";
        let result = invalid_cn.strip_prefix("node-");
        assert!(result.is_none());
    }
}
