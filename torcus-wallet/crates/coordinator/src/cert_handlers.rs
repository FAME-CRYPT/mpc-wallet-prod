//! Certificate management handlers for P2P mTLS.
//!
//! This module provides API endpoints for certificate distribution
//! to MPC nodes. Nodes fetch certificates from the coordinator to
//! establish secure QUIC connections with mutual TLS authentication.
//!
//! ## Certificate Flow
//!
//! ```text
//! ┌─────────────┐                    ┌──────────────┐
//! │   Node N    │──POST /registry───>│  Coordinator │  (1) Register first
//! │             │<───node_id─────────│              │
//! │             │                    │              │
//! │             │──GET /certs/ca────>│              │  (2) Get CA cert
//! │             │<───CA cert (PEM)───│              │
//! │             │                    │              │
//! │             │─POST /certs/node/N>│              │  (3) Get node cert
//! │             │  (with node_id)    │              │      (requires valid node_id)
//! │             │<──Signed node cert─│              │
//! └─────────────┘                    └──────────────┘
//! ```
//!
//! ## Security
//!
//! - CA certificate is public (nodes use it to verify peers)
//! - Node certificates are signed by the CA and specific to each party
//! - **Node certificates are ONLY issued to registered nodes**
//! - The requesting node must provide its node_id from registration
//! - The node_id must match a registered node with the same party_index
//! - Node private keys are generated fresh and returned only once
//! - Certificates include party index for identification during TLS

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::state::AppState;

// ============================================================================
// Request/Response Types
// ============================================================================

/// Response containing the CA certificate.
#[derive(Debug, Clone, Serialize)]
pub struct GetCaCertResponse {
    /// CA certificate in PEM format.
    pub cert_pem: String,
    /// Whether the CA is available.
    pub available: bool,
}

/// Request to issue a node certificate.
#[derive(Debug, Clone, Deserialize)]
pub struct IssueNodeCertRequest {
    /// The secret cert_token received during registration.
    /// This authenticates the request - only registered nodes can get certificates.
    /// This token is never exposed in public APIs and should be treated as a secret.
    pub cert_token: String,
    /// Hostnames/IPs for the certificate's SAN field.
    /// At minimum should include the node's Docker service name.
    pub hostnames: Vec<String>,
}

/// Response containing the issued node certificate.
#[derive(Debug, Clone, Serialize)]
pub struct IssueNodeCertResponse {
    /// Party index this certificate is for.
    pub party_index: u16,
    /// Node certificate in PEM format.
    pub cert_pem: String,
    /// Node private key in PEM format (PKCS#8).
    /// WARNING: This is returned only once. Store securely.
    pub key_pem: String,
    /// CA certificate in PEM format (for verification).
    pub ca_cert_pem: String,
    /// Human-readable message.
    pub message: String,
}

// ============================================================================
// Handlers
// ============================================================================

/// Get the CA certificate (public).
///
/// GET /certs/ca
///
/// Returns the CA certificate in PEM format. Nodes use this to
/// verify peer certificates during mTLS handshakes.
///
/// ## Response
///
/// ```json
/// {
///     "cert_pem": "-----BEGIN CERTIFICATE-----\n...\n-----END CERTIFICATE-----\n",
///     "available": true
/// }
/// ```
pub async fn get_ca_cert(
    State(state): State<Arc<RwLock<AppState>>>,
) -> Result<Json<GetCaCertResponse>, (StatusCode, String)> {
    debug!("CA certificate requested");

    let s = state.read().await;

    match s.ca_cert_pem() {
        Some(cert_pem) => {
            info!("CA certificate served successfully");
            Ok(Json(GetCaCertResponse {
                cert_pem,
                available: true,
            }))
        }
        None => {
            warn!("CA certificate requested but not available");
            Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "CA certificate not available. P2P mode may be disabled.".to_string(),
            ))
        }
    }
}

/// Issue a signed node certificate.
///
/// POST /certs/node/:party_index
///
/// Signs and returns a certificate for the specified node. The certificate
/// is signed by the coordinator's CA and includes the party index for
/// identification.
///
/// ## Security
///
/// **Authentication required**: The requesting node must provide its `cert_token`
/// received during registration. This token:
/// - Is only returned once during initial registration
/// - Is never exposed in any public API (unlike node_id)
/// - Is verified using a secure hash comparison
///
/// This prevents arbitrary parties from minting identities and impersonating nodes,
/// even if they can query the public registry APIs.
///
/// ## Request
///
/// ```json
/// {
///     "cert_token": "secret-token-from-registration",
///     "hostnames": ["mpc-node-1", "localhost", "127.0.0.1"]
/// }
/// ```
///
/// ## Response
///
/// ```json
/// {
///     "party_index": 0,
///     "cert_pem": "-----BEGIN CERTIFICATE-----\n...",
///     "key_pem": "-----BEGIN PRIVATE KEY-----\n...",
///     "ca_cert_pem": "-----BEGIN CERTIFICATE-----\n...",
///     "message": "Node certificate issued successfully"
/// }
/// ```
pub async fn issue_node_cert(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(party_index): Path<u16>,
    Json(request): Json<IssueNodeCertRequest>,
) -> Result<Json<IssueNodeCertResponse>, (StatusCode, String)> {
    info!("========================================");
    info!("  NODE CERTIFICATE ISSUANCE REQUEST");
    info!("========================================");
    info!("Party index: {}", party_index);
    // Note: We don't log cert_token as it's a secret
    info!("Hostnames: {:?}", request.hostnames);

    // Validate hostnames
    if request.hostnames.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "At least one hostname is required".to_string(),
        ));
    }

    // Validate party index (basic sanity check)
    if party_index > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid party index: {} (must be reasonable value)",
                party_index
            ),
        ));
    }

    let s = state.read().await;

    // SECURITY: Verify the cert_token is valid for this party_index
    // The cert_token is a secret issued during registration and never exposed publicly
    if !s
        .node_registry
        .verify_cert_token(party_index, &request.cert_token)
    {
        warn!(
            "Certificate request rejected: invalid cert_token for party {}",
            party_index
        );
        return Err((
            StatusCode::FORBIDDEN,
            "Invalid cert_token. Ensure you are using the token received during registration."
                .to_string(),
        ));
    }

    // Sign the certificate
    let node_cert = s
        .sign_node_certificate(party_index, &request.hostnames)
        .map_err(|e| (StatusCode::SERVICE_UNAVAILABLE, e))?;

    info!(
        "Node certificate issued for party {} with {} hostnames",
        party_index,
        request.hostnames.len()
    );

    Ok(Json(IssueNodeCertResponse {
        party_index,
        cert_pem: node_cert.cert_pem.clone(),
        key_pem: node_cert.key_pem.clone(),
        ca_cert_pem: node_cert.ca_cert_pem.clone(),
        message: format!(
            "Node certificate issued for party {}. Store the private key securely.",
            party_index
        ),
    }))
}

/// Check CA availability.
///
/// GET /certs/status
///
/// Returns whether certificate issuance is available.
#[derive(Debug, Clone, Serialize)]
pub struct CertStatusResponse {
    /// Whether the CA is available.
    pub ca_available: bool,
    /// Human-readable status.
    pub status: String,
}

pub async fn cert_status(State(state): State<Arc<RwLock<AppState>>>) -> Json<CertStatusResponse> {
    let s = state.read().await;
    let available = s.has_ca_certificate();

    Json(CertStatusResponse {
        ca_available: available,
        status: if available {
            "CA certificate available. P2P certificate issuance enabled.".to_string()
        } else {
            "CA certificate not available. P2P mode disabled.".to_string()
        },
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_issue_node_cert_request_deserialize() {
        let json =
            r#"{"cert_token": "secret-token-123", "hostnames": ["mpc-node-1", "localhost"]}"#;
        let req: IssueNodeCertRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.cert_token, "secret-token-123");
        assert_eq!(req.hostnames.len(), 2);
        assert_eq!(req.hostnames[0], "mpc-node-1");
    }

    #[test]
    fn test_get_ca_cert_response_serialize() {
        let resp = GetCaCertResponse {
            cert_pem: "test".to_string(),
            available: true,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("cert_pem"));
        assert!(json.contains("available"));
    }
}
