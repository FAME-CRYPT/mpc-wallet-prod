//! Certificate management for QUIC TLS.
//!
//! This module provides CA-signed certificate infrastructure for secure
//! P2P communication between MPC nodes.
//!
//! ## Certificate Hierarchy
//!
//! ```text
//! Root CA (self-signed)
//!     │
//!     ├── Node 0 Certificate (signed by CA)
//!     ├── Node 1 Certificate (signed by CA)
//!     ├── Node 2 Certificate (signed by CA)
//!     └── Node 3 Certificate (signed by CA)
//! ```
//!
//! ## Security Model
//!
//! - The CA private key should be kept secure (ideally in HSM or secure enclave)
//! - Each node has its own certificate signed by the CA
//! - Nodes verify peer certificates against the CA during TLS handshake
//! - Certificate includes party index in the subject for identification

use std::sync::Arc;
use std::time::Duration;

use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    KeyPair, KeyUsagePurpose, SanType,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use super::error::P2pError;

/// Default certificate validity period (1 year).
const DEFAULT_VALIDITY_DAYS: u32 = 365;

/// CA certificate common name.
const CA_COMMON_NAME: &str = "Torcus MPC Wallet CA";

/// Organization name for certificates.
const ORG_NAME: &str = "Torcus";

/// Stored CA certificate and private key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCaCert {
    /// PEM-encoded CA certificate.
    pub cert_pem: String,
    /// PEM-encoded CA private key (PKCS#8).
    pub key_pem: String,
    /// Unix timestamp when the CA was created.
    pub created_at: u64,
}

/// Stored node certificate and private key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredNodeCert {
    /// Party index this certificate is for.
    pub party_index: u16,
    /// PEM-encoded node certificate.
    pub cert_pem: String,
    /// PEM-encoded node private key (PKCS#8).
    pub key_pem: String,
    /// PEM-encoded CA certificate (for verification).
    pub ca_cert_pem: String,
    /// Unix timestamp when the certificate was created.
    pub created_at: u64,
}

/// CA certificate for signing node certificates.
pub struct CaCertificate {
    /// The CA certificate.
    cert: rcgen::Certificate,
    /// The CA key pair.
    key_pair: KeyPair,
    /// DER-encoded certificate.
    cert_der: Vec<u8>,
}

impl CaCertificate {
    /// Generate a new CA certificate.
    pub fn generate() -> Result<Self, P2pError> {
        info!("Generating new CA certificate");

        let mut params = CertificateParams::default();

        // Set distinguished name
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, CA_COMMON_NAME);
        dn.push(DnType::OrganizationName, ORG_NAME);
        params.distinguished_name = dn;

        // CA constraints
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];

        // Validity period
        let now = std::time::SystemTime::now();
        params.not_before = now.into();
        params.not_after =
            (now + Duration::from_secs(DEFAULT_VALIDITY_DAYS as u64 * 24 * 60 * 60)).into();

        // Generate key pair
        let key_pair = KeyPair::generate()
            .map_err(|e| P2pError::CodecError(format!("Failed to generate CA key pair: {}", e)))?;

        // Create certificate
        let cert = params
            .self_signed(&key_pair)
            .map_err(|e| P2pError::CodecError(format!("Failed to create CA certificate: {}", e)))?;

        let cert_der = cert.der().to_vec();

        info!("CA certificate generated successfully");

        Ok(Self {
            cert,
            key_pair,
            cert_der,
        })
    }

    /// Load CA from stored PEM data.
    ///
    /// Note: This regenerates the CA certificate from the stored key pair.
    /// The certificate content will be equivalent but the actual bytes may differ.
    pub fn from_pem(_cert_pem: &str, key_pem: &str) -> Result<Self, P2pError> {
        debug!("Loading CA certificate from PEM");

        let key_pair = KeyPair::from_pem(key_pem)
            .map_err(|e| P2pError::CodecError(format!("Failed to parse CA private key: {}", e)))?;

        // Regenerate the CA certificate with the same key
        let mut params = CertificateParams::default();
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, CA_COMMON_NAME);
        dn.push(DnType::OrganizationName, ORG_NAME);
        params.distinguished_name = dn;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];

        let now = std::time::SystemTime::now();
        params.not_before = now.into();
        params.not_after =
            (now + Duration::from_secs(DEFAULT_VALIDITY_DAYS as u64 * 24 * 60 * 60)).into();

        let cert = params.self_signed(&key_pair).map_err(|e| {
            P2pError::CodecError(format!("Failed to recreate CA certificate: {}", e))
        })?;

        let cert_der = cert.der().to_vec();

        Ok(Self {
            cert,
            key_pair,
            cert_der,
        })
    }

    /// Export to storable format.
    pub fn to_stored(&self) -> StoredCaCert {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        StoredCaCert {
            cert_pem: self.cert.pem(),
            key_pem: self.key_pair.serialize_pem(),
            created_at: now,
        }
    }

    /// Get the CA certificate in DER format.
    pub fn cert_der(&self) -> &[u8] {
        &self.cert_der
    }

    /// Get the CA certificate in PEM format.
    pub fn cert_pem(&self) -> String {
        self.cert.pem()
    }

    /// Sign a node certificate.
    pub fn sign_node_cert(
        &self,
        party_index: u16,
        hostnames: &[String],
    ) -> Result<NodeCertificate, P2pError> {
        info!("Signing certificate for node {} (party index)", party_index);

        let mut params = CertificateParams::default();

        // Set distinguished name with party index
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, format!("MPC Node {}", party_index));
        dn.push(DnType::OrganizationName, ORG_NAME);
        // Store party index in organizational unit for identification
        dn.push(
            DnType::OrganizationalUnitName,
            format!("party-{}", party_index),
        );
        params.distinguished_name = dn;

        // Not a CA
        params.is_ca = IsCa::NoCa;
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];
        params.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ServerAuth,
            ExtendedKeyUsagePurpose::ClientAuth,
        ];

        // Add hostnames as SANs
        for hostname in hostnames {
            if let Ok(ip) = hostname.parse::<std::net::IpAddr>() {
                params.subject_alt_names.push(SanType::IpAddress(ip));
            } else {
                // Use Ia5String::try_from for DNS names
                let dns_name = rcgen::Ia5String::try_from(hostname.clone()).map_err(|e| {
                    P2pError::CodecError(format!("Invalid DNS name {}: {:?}", hostname, e))
                })?;
                params.subject_alt_names.push(SanType::DnsName(dns_name));
            }
        }

        // Validity period
        let now = std::time::SystemTime::now();
        params.not_before = now.into();
        params.not_after =
            (now + Duration::from_secs(DEFAULT_VALIDITY_DAYS as u64 * 24 * 60 * 60)).into();

        // Generate key pair for node
        let key_pair = KeyPair::generate().map_err(|e| {
            P2pError::CodecError(format!("Failed to generate node key pair: {}", e))
        })?;

        // Sign with CA
        let cert = params
            .signed_by(&key_pair, &self.cert, &self.key_pair)
            .map_err(|e| P2pError::CodecError(format!("Failed to sign node certificate: {}", e)))?;

        let cert_der = cert.der().to_vec();
        let key_der = key_pair.serialize_der();

        info!("Node certificate signed for party {}", party_index);

        Ok(NodeCertificate {
            party_index,
            cert_pem: cert.pem(),
            key_pem: key_pair.serialize_pem(),
            cert_der,
            key_der,
            ca_cert_pem: self.cert.pem(),
            ca_cert_der: self.cert_der.clone(),
        })
    }
}

/// Node certificate signed by the CA.
pub struct NodeCertificate {
    /// Party index this certificate is for.
    pub party_index: u16,
    /// PEM-encoded certificate.
    pub cert_pem: String,
    /// PEM-encoded private key.
    pub key_pem: String,
    /// DER-encoded certificate.
    cert_der: Vec<u8>,
    /// DER-encoded private key.
    key_der: Vec<u8>,
    /// PEM-encoded CA certificate.
    pub ca_cert_pem: String,
    /// DER-encoded CA certificate.
    ca_cert_der: Vec<u8>,
}

impl NodeCertificate {
    /// Load from stored format.
    pub fn from_stored(stored: &StoredNodeCert) -> Result<Self, P2pError> {
        debug!("Loading node certificate for party {}", stored.party_index);

        // Parse PEM to get DER
        let cert_der = pem_to_der(&stored.cert_pem, "CERTIFICATE")?;
        let key_der = pem_to_der(&stored.key_pem, "PRIVATE KEY")?;
        let ca_cert_der = pem_to_der(&stored.ca_cert_pem, "CERTIFICATE")?;

        Ok(Self {
            party_index: stored.party_index,
            cert_pem: stored.cert_pem.clone(),
            key_pem: stored.key_pem.clone(),
            cert_der,
            key_der,
            ca_cert_pem: stored.ca_cert_pem.clone(),
            ca_cert_der,
        })
    }

    /// Export to storable format.
    pub fn to_stored(&self) -> StoredNodeCert {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        StoredNodeCert {
            party_index: self.party_index,
            cert_pem: self.cert_pem.clone(),
            key_pem: self.key_pem.clone(),
            ca_cert_pem: self.ca_cert_pem.clone(),
            created_at: now,
        }
    }

    /// Get certificate chain for TLS (node cert + CA cert).
    pub fn cert_chain(&self) -> Vec<CertificateDer<'static>> {
        vec![
            CertificateDer::from(self.cert_der.clone()),
            CertificateDer::from(self.ca_cert_der.clone()),
        ]
    }

    /// Get private key for TLS.
    pub fn private_key(&self) -> PrivateKeyDer<'static> {
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key_der.clone()))
    }

    /// Get CA certificate for verification.
    pub fn ca_cert_der(&self) -> CertificateDer<'static> {
        CertificateDer::from(self.ca_cert_der.clone())
    }

    /// Build rustls server config for this node.
    pub fn server_config(&self) -> Result<rustls::ServerConfig, P2pError> {
        let cert_chain = self.cert_chain();
        let private_key = self.private_key();

        // Create root cert store with our CA
        let mut root_store = rustls::RootCertStore::empty();
        root_store
            .add(self.ca_cert_der())
            .map_err(|e| P2pError::CodecError(format!("Failed to add CA to root store: {}", e)))?;

        // Build client verifier
        let client_verifier = rustls::server::WebPkiClientVerifier::builder(Arc::new(root_store))
            .build()
            .map_err(|e| P2pError::CodecError(format!("Failed to build client verifier: {}", e)))?;

        // Build server config with client auth
        let config = rustls::ServerConfig::builder()
            .with_client_cert_verifier(client_verifier)
            .with_single_cert(cert_chain, private_key)
            .map_err(|e| P2pError::CodecError(format!("Failed to build server config: {}", e)))?;

        Ok(config)
    }

    /// Build rustls client config for connecting to peers.
    pub fn client_config(&self) -> Result<rustls::ClientConfig, P2pError> {
        let cert_chain = self.cert_chain();
        let private_key = self.private_key();

        // Create root cert store with our CA
        let mut root_store = rustls::RootCertStore::empty();
        root_store
            .add(self.ca_cert_der())
            .map_err(|e| P2pError::CodecError(format!("Failed to add CA to root store: {}", e)))?;

        // Build client config with our cert for mutual TLS
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_client_auth_cert(cert_chain, private_key)
            .map_err(|e| P2pError::CodecError(format!("Failed to build client config: {}", e)))?;

        Ok(config)
    }
}

/// Parse PEM to DER bytes.
fn pem_to_der(pem: &str, expected_label: &str) -> Result<Vec<u8>, P2pError> {
    // Simple PEM parser
    let lines: Vec<&str> = pem.lines().collect();
    let begin_marker = format!("-----BEGIN {}-----", expected_label);
    let end_marker = format!("-----END {}-----", expected_label);

    let start = lines
        .iter()
        .position(|l| *l == begin_marker)
        .ok_or_else(|| {
            P2pError::CodecError(format!("Missing PEM begin marker for {}", expected_label))
        })?;

    let end = lines.iter().position(|l| *l == end_marker).ok_or_else(|| {
        P2pError::CodecError(format!("Missing PEM end marker for {}", expected_label))
    })?;

    let base64_data: String = lines[start + 1..end].join("");

    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(&base64_data)
        .map_err(|e| P2pError::CodecError(format!("Failed to decode PEM base64: {}", e)))
}

/// Default paths for certificate storage.
pub mod paths {
    use std::path::PathBuf;

    /// Get the CA certificate path.
    pub fn ca_cert_path() -> PathBuf {
        PathBuf::from("/data/ca-cert.json")
    }

    /// Get the node certificate path for a party.
    pub fn node_cert_path(party_index: u16) -> PathBuf {
        PathBuf::from(format!("/data/node-cert-party-{}.json", party_index))
    }
}

/// Save CA certificate to file.
pub fn save_ca_cert(path: &std::path::Path, ca: &StoredCaCert) -> Result<(), P2pError> {
    let json = serde_json::to_string_pretty(ca)
        .map_err(|e| P2pError::CodecError(format!("Failed to serialize CA cert: {}", e)))?;
    std::fs::write(path, json)
        .map_err(|e| P2pError::CodecError(format!("Failed to write CA cert: {}", e)))?;
    info!("CA certificate saved to {:?}", path);
    Ok(())
}

/// Load CA certificate from file.
pub fn load_ca_cert(path: &std::path::Path) -> Result<StoredCaCert, P2pError> {
    let json = std::fs::read_to_string(path)
        .map_err(|e| P2pError::CodecError(format!("Failed to read CA cert: {}", e)))?;
    let stored: StoredCaCert = serde_json::from_str(&json)
        .map_err(|e| P2pError::CodecError(format!("Failed to parse CA cert: {}", e)))?;
    debug!("CA certificate loaded from {:?}", path);
    Ok(stored)
}

/// Save node certificate to file.
pub fn save_node_cert(path: &std::path::Path, cert: &StoredNodeCert) -> Result<(), P2pError> {
    let json = serde_json::to_string_pretty(cert)
        .map_err(|e| P2pError::CodecError(format!("Failed to serialize node cert: {}", e)))?;
    std::fs::write(path, json)
        .map_err(|e| P2pError::CodecError(format!("Failed to write node cert: {}", e)))?;
    info!("Node certificate saved to {:?}", path);
    Ok(())
}

/// Load node certificate from file.
pub fn load_node_cert(path: &std::path::Path) -> Result<StoredNodeCert, P2pError> {
    let json = std::fs::read_to_string(path)
        .map_err(|e| P2pError::CodecError(format!("Failed to read node cert: {}", e)))?;
    let stored: StoredNodeCert = serde_json::from_str(&json)
        .map_err(|e| P2pError::CodecError(format!("Failed to parse node cert: {}", e)))?;
    debug!("Node certificate loaded from {:?}", path);
    Ok(stored)
}

/// Extract party index from a DER-encoded certificate.
///
/// The party index is stored in the OrganizationalUnitName field
/// as "party-{index}" (e.g., "party-0", "party-1").
///
/// # Arguments
/// * `cert_der` - DER-encoded X.509 certificate
///
/// # Returns
/// * `Some(party_index)` if successfully extracted
/// * `None` if the certificate doesn't contain a valid party index
pub fn extract_party_index_from_cert(cert_der: &[u8]) -> Option<u16> {
    use x509_parser::prelude::*;

    let (_, cert) = X509Certificate::from_der(cert_der).ok()?;

    // Look for OrganizationalUnitName in the subject
    for rdn in cert.subject().iter() {
        for attr in rdn.iter() {
            if attr.attr_type() == &oid_registry::OID_X509_ORGANIZATIONAL_UNIT {
                if let Ok(ou) = attr.as_str() {
                    // Parse "party-{index}" format
                    if let Some(idx_str) = ou.strip_prefix("party-") {
                        if let Ok(party_index) = idx_str.parse::<u16>() {
                            return Some(party_index);
                        }
                    }
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ca_generation() {
        let ca = CaCertificate::generate().unwrap();
        assert!(!ca.cert_der().is_empty());
        assert!(ca.cert_pem().contains("BEGIN CERTIFICATE"));
    }

    #[test]
    fn test_ca_roundtrip() {
        let ca = CaCertificate::generate().unwrap();
        let stored = ca.to_stored();

        // Note: from_pem regenerates the certificate, so bytes won't be identical
        // but the key material should be the same
        let ca2 = CaCertificate::from_pem(&stored.cert_pem, &stored.key_pem).unwrap();
        assert!(!ca2.cert_der().is_empty());
    }

    #[test]
    fn test_node_cert_signing() {
        let ca = CaCertificate::generate().unwrap();
        let hostnames = vec!["localhost".to_string(), "127.0.0.1".to_string()];

        let node_cert = ca.sign_node_cert(0, &hostnames).unwrap();
        assert_eq!(node_cert.party_index, 0);
        assert!(node_cert.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(node_cert.key_pem.contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn test_node_cert_roundtrip() {
        let ca = CaCertificate::generate().unwrap();
        let node_cert = ca.sign_node_cert(1, &["mpc-node-2".to_string()]).unwrap();

        let stored = node_cert.to_stored();
        let loaded = NodeCertificate::from_stored(&stored).unwrap();

        assert_eq!(loaded.party_index, 1);
    }

    #[test]
    fn test_tls_configs() {
        let ca = CaCertificate::generate().unwrap();
        let node_cert = ca.sign_node_cert(0, &["localhost".to_string()]).unwrap();

        // Should be able to build both configs
        let _server_config = node_cert.server_config().unwrap();
        let _client_config = node_cert.client_config().unwrap();
    }

    #[test]
    fn test_extract_party_index_from_cert() {
        let ca = CaCertificate::generate().unwrap();

        // Test various party indices
        for party_index in [0, 1, 5, 100] {
            let node_cert = ca
                .sign_node_cert(party_index, &["localhost".to_string()])
                .unwrap();

            // Extract party index from the DER-encoded certificate
            let extracted = extract_party_index_from_cert(&node_cert.cert_der);
            assert_eq!(
                extracted,
                Some(party_index),
                "Failed to extract party index {} from cert",
                party_index
            );
        }
    }

    #[test]
    fn test_extract_party_index_from_invalid_cert() {
        // Test with invalid DER data
        let invalid_der = b"not a valid certificate";
        assert_eq!(extract_party_index_from_cert(invalid_der), None);

        // Test with empty data
        assert_eq!(extract_party_index_from_cert(&[]), None);
    }
}
