//! Node certificate management.
//!
//! This module handles fetching and caching TLS certificates for P2P
//! communication. Certificates are issued by the coordinator's CA and
//! stored locally for reuse.
//!
//! ## Certificate Flow
//!
//! ```text
//! 1. Check local cache (/data/node-cert-party-N.json)
//! 2. If not found or expired, request from coordinator:
//!    - GET /certs/ca (get CA public cert)
//!    - POST /certs/node/:party_index (get signed node cert)
//!      - Requires cert_token from registration for authentication
//! 3. Cache the result locally
//! 4. Return NodeCertificate for QUIC transport use
//! ```
//!
//! ## Security
//!
//! Certificate issuance requires the node to be registered first. The node
//! must provide its `cert_token` (received during registration) when requesting
//! a certificate. This token is a secret that is never exposed in public APIs,
//! preventing unauthorized parties from minting certificates.

#![allow(dead_code)] // Functions will be used in Phase 2 P2P initialization

use std::path::PathBuf;
use std::time::Duration;

use protocols::p2p::certs::{load_node_cert, save_node_cert, NodeCertificate, StoredNodeCert};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Default path for node certificates.
fn node_cert_path(party_index: u16) -> PathBuf {
    PathBuf::from(format!("/data/node-cert-party-{}.json", party_index))
}

/// Response from coordinator's GET /certs/ca endpoint.
#[derive(Debug, Clone, Deserialize)]
struct CaCertResponse {
    cert_pem: String,
    available: bool,
}

/// Request body for POST /certs/node/:party_index.
#[derive(Debug, Clone, Serialize)]
struct IssueNodeCertRequest {
    /// The secret cert_token received during registration (required for authentication).
    cert_token: String,
    /// Hostnames/IPs for the certificate's SAN field.
    hostnames: Vec<String>,
}

/// Response from coordinator's POST /certs/node/:party_index endpoint.
#[derive(Debug, Clone, Deserialize)]
struct IssueNodeCertResponse {
    party_index: u16,
    cert_pem: String,
    key_pem: String,
    ca_cert_pem: String,
    message: String,
}

/// Error type for certificate operations.
#[derive(Debug)]
pub enum CertError {
    /// Failed to communicate with coordinator.
    Network(String),
    /// Coordinator returned an error.
    Coordinator(String),
    /// Local file operation failed.
    Io(String),
    /// Certificate parsing failed.
    Parse(String),
    /// CA not available on coordinator.
    CaUnavailable,
}

impl std::fmt::Display for CertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(e) => write!(f, "Network error: {}", e),
            Self::Coordinator(e) => write!(f, "Coordinator error: {}", e),
            Self::Io(e) => write!(f, "I/O error: {}", e),
            Self::Parse(e) => write!(f, "Parse error: {}", e),
            Self::CaUnavailable => write!(f, "CA certificate not available on coordinator"),
        }
    }
}

impl std::error::Error for CertError {}

/// Try to load a valid certificate from disk cache.
///
/// Returns Some(certificate) if a valid cached certificate exists, None otherwise.
/// This does not require cert_token since it only reads from local cache.
pub fn try_load_cached_certificate(party_index: u16) -> Option<NodeCertificate> {
    let cert_path = node_cert_path(party_index);

    if !cert_path.exists() {
        debug!("No cached certificate at {:?}", cert_path);
        return None;
    }

    debug!("Found existing certificate at {:?}", cert_path);
    match load_node_cert(&cert_path) {
        Ok(stored) => {
            // Check if the certificate is still valid (simple freshness check)
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            // Certificate valid for 365 days, refresh if > 300 days old
            let max_age = 300 * 24 * 60 * 60; // 300 days in seconds
            if now - stored.created_at < max_age {
                info!(
                    "Loaded valid certificate for party {} from cache",
                    party_index
                );
                match NodeCertificate::from_stored(&stored) {
                    Ok(cert) => return Some(cert),
                    Err(e) => {
                        warn!("Failed to parse cached certificate: {}", e);
                    }
                }
            } else {
                info!("Cached certificate is old (>300 days), needs refresh");
            }
        }
        Err(e) => {
            warn!("Failed to load certificate from {:?}: {}", cert_path, e);
        }
    }

    None
}

/// Ensure the node has a valid certificate.
///
/// This function:
/// 1. Tries to load an existing certificate from disk
/// 2. If not found or expired, fetches a new one from the coordinator
/// 3. Caches the certificate locally
/// 4. Returns the NodeCertificate for QUIC transport use
///
/// # Arguments
///
/// * `cert_token` - Secret token received during registration (for authentication).
///   Can be None if we only want to use cached certificate.
/// * `party_index` - This node's party index
/// * `coordinator_url` - Base URL of the coordinator (e.g., "http://mpc-coordinator:3000")
/// * `hostnames` - Hostnames/IPs to include in the certificate's SAN field
///
/// # Returns
///
/// The node's certificate on success, or an error if unavailable.
///
/// # Security
///
/// The `cert_token` is a secret used to authenticate the certificate request.
/// It is only returned once during registration and never exposed in public APIs.
/// Only nodes with valid tokens can obtain certificates, preventing identity spoofing.
/// If cert_token is None and no valid cached cert exists, returns an error.
pub async fn ensure_node_certificate(
    cert_token: Option<&str>,
    party_index: u16,
    coordinator_url: &str,
    hostnames: Vec<String>,
) -> Result<NodeCertificate, CertError> {
    // Try to load from disk first (no cert_token needed)
    if let Some(cert) = try_load_cached_certificate(party_index) {
        return Ok(cert);
    }

    // No valid cached cert - need to fetch from coordinator
    let cert_token = cert_token.ok_or_else(|| {
        CertError::Coordinator(
            "No cached certificate and no cert_token available. \
             Node must be registered to fetch a new certificate."
                .to_string(),
        )
    })?;

    // Fetch from coordinator
    info!(
        "Fetching certificate for party {} from coordinator",
        party_index
    );
    let cert =
        fetch_certificate_from_coordinator(cert_token, party_index, coordinator_url, hostnames)
            .await?;

    // Cache locally
    let cert_path = node_cert_path(party_index);
    let stored = cert.to_stored();
    if let Err(e) = save_node_cert(&cert_path, &stored) {
        warn!("Failed to cache certificate to {:?}: {}", cert_path, e);
        // Continue anyway - we have the cert in memory
    } else {
        info!("Cached certificate to {:?}", cert_path);
    }

    Ok(cert)
}

/// Fetch a new certificate from the coordinator.
async fn fetch_certificate_from_coordinator(
    cert_token: &str,
    party_index: u16,
    coordinator_url: &str,
    hostnames: Vec<String>,
) -> Result<NodeCertificate, CertError> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| CertError::Network(format!("Failed to create HTTP client: {}", e)))?;

    // First, check if CA is available
    let ca_url = format!("{}/certs/ca", coordinator_url);
    debug!("Checking CA availability at {}", ca_url);

    let ca_response = client
        .get(&ca_url)
        .send()
        .await
        .map_err(|e| CertError::Network(format!("Failed to reach coordinator: {}", e)))?;

    if !ca_response.status().is_success() {
        let status = ca_response.status();
        let body = ca_response.text().await.unwrap_or_default();
        return Err(CertError::Coordinator(format!(
            "CA endpoint returned {}: {}",
            status, body
        )));
    }

    let ca_data: CaCertResponse = ca_response
        .json()
        .await
        .map_err(|e| CertError::Parse(format!("Failed to parse CA response: {}", e)))?;

    if !ca_data.available {
        return Err(CertError::CaUnavailable);
    }

    // Request node certificate
    let cert_url = format!("{}/certs/node/{}", coordinator_url, party_index);
    debug!("Requesting node certificate from {}", cert_url);

    let request_body = IssueNodeCertRequest {
        cert_token: cert_token.to_string(),
        hostnames,
    };

    let cert_response = client
        .post(&cert_url)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| CertError::Network(format!("Failed to request certificate: {}", e)))?;

    if !cert_response.status().is_success() {
        let status = cert_response.status();
        let body = cert_response.text().await.unwrap_or_default();
        return Err(CertError::Coordinator(format!(
            "Certificate endpoint returned {}: {}",
            status, body
        )));
    }

    let cert_data: IssueNodeCertResponse = cert_response
        .json()
        .await
        .map_err(|e| CertError::Parse(format!("Failed to parse certificate response: {}", e)))?;

    info!(
        "Received certificate from coordinator: {}",
        cert_data.message
    );

    // Convert to StoredNodeCert and then to NodeCertificate
    let stored = StoredNodeCert {
        party_index: cert_data.party_index,
        cert_pem: cert_data.cert_pem,
        key_pem: cert_data.key_pem,
        ca_cert_pem: cert_data.ca_cert_pem,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };

    NodeCertificate::from_stored(&stored).map_err(|e| CertError::Parse(e.to_string()))
}

/// Check if a valid certificate exists locally.
///
/// Returns true if a certificate exists and is not too old.
pub fn has_valid_certificate(party_index: u16) -> bool {
    let cert_path = node_cert_path(party_index);
    if !cert_path.exists() {
        return false;
    }

    match load_node_cert(&cert_path) {
        Ok(stored) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let max_age = 300 * 24 * 60 * 60; // 300 days
            now - stored.created_at < max_age
        }
        Err(_) => false,
    }
}

/// Get the default hostnames for a node.
///
/// These are used for the certificate's SAN field.
pub fn default_hostnames(party_index: u16) -> Vec<String> {
    vec![
        format!("mpc-node-{}", party_index + 1), // Docker service name (1-indexed)
        "localhost".to_string(),
        "127.0.0.1".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_hostnames() {
        let hostnames = default_hostnames(0);
        assert_eq!(hostnames.len(), 3);
        assert_eq!(hostnames[0], "mpc-node-1");
        assert_eq!(hostnames[1], "localhost");
    }

    #[test]
    fn test_node_cert_path() {
        let path = node_cert_path(2);
        assert!(path.to_string_lossy().contains("party-2"));
    }

    #[test]
    fn test_try_load_cached_certificate_missing() {
        // For a party index that definitely has no cached cert
        // (we use a very high number unlikely to exist)
        let result = try_load_cached_certificate(9999);
        assert!(result.is_none());
    }

    #[test]
    fn test_ensure_node_certificate_no_token_no_cache() {
        // When there's no cert_token and no cached certificate,
        // the function should return an error
        use tokio::runtime::Runtime;

        let rt = Runtime::new().unwrap();
        let result = rt.block_on(async {
            ensure_node_certificate(
                None,                     // No cert_token
                9998,                     // Party unlikely to have cache
                "http://localhost:99999", // Invalid URL (won't be reached)
                vec!["localhost".to_string()],
            )
            .await
        });

        // Should fail because there's no cached cert and no token to fetch one
        assert!(result.is_err());
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("Expected error, got Ok"),
        };
        assert!(
            err.to_string().contains("No cached certificate")
                || err.to_string().contains("cert_token"),
            "Expected error about missing cached cert or token, got: {}",
            err
        );
    }
}
