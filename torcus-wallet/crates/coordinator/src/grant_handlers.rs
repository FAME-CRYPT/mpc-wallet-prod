//! Grant issuance handlers for P2P session coordination.
//!
//! This module provides standalone grant issuance endpoints that
//! separate grant creation from session management. Grants authorize
//! specific signing operations while allowing nodes to coordinate
//! sessions themselves via P2P.
//!
//! ## Architecture (Phase 4)
//!
//! ```text
//! ┌─────────┐     ┌──────────────┐
//! │   CLI   │────>│  Coordinator │  Issue grant
//! └─────────┘     │  (this code) │
//!      │          └──────────────┘
//!      │
//!      │  Grant + sign request
//!      ▼
//! ┌─────────┐
//! │ Node N  │  (initiator determined by grant)
//! └─────────┘
//!      │
//!      │  SessionProposal (P2P)
//!      ▼
//! ┌─────────────────────────────┐
//! │   Other nodes (P2P mesh)    │
//! └─────────────────────────────┘
//! ```
//!
//! ## Security
//!
//! - All grants are Ed25519 signed by the coordinator
//! - Grants include expiration times and nonces for replay protection
//! - Nodes verify grants before participating in sessions

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use rand::RngCore;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info};

use common::{grant::SigningGrant, selection::derive_selection_seed, SelectionPolicy, THRESHOLD};

use crate::handlers::reject_mainnet;
use crate::relay_handlers::select_parties_with_policy;
use crate::state::AppState;

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request to issue a signing grant.
#[derive(Debug, Clone, Deserialize)]
pub struct IssueSigningGrantRequest {
    /// Wallet ID to sign with.
    pub wallet_id: String,
    /// Message hash to sign (hex-encoded, 32 bytes).
    pub message_hash: String,
    /// Optional: specific parties to participate.
    /// If not provided, uses deterministic selection from healthy nodes.
    #[serde(default)]
    pub parties: Option<Vec<u16>>,
}

/// Response containing the issued grant.
#[derive(Debug, Clone, Serialize)]
pub struct IssueSigningGrantResponse {
    /// The signed grant.
    pub grant: SigningGrant,
    /// Session ID derived from the grant.
    pub session_id: String,
    /// Selected participants.
    pub participants: Vec<u16>,
    /// Human-readable message.
    pub message: String,
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse and validate a message hash.
fn parse_message_hash(hash_hex: &str) -> Result<[u8; 32], (StatusCode, String)> {
    let bytes = hex::decode(hash_hex).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid message hash hex: {}", e),
        )
    })?;

    if bytes.len() != 32 {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Message hash must be 32 bytes, got {}", bytes.len()),
        ));
    }

    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

/// Select parties for signing.
///
/// If parties are explicitly provided, validates them.
/// Otherwise, uses deterministic selection from healthy nodes.
async fn select_signing_parties(
    state: &Arc<RwLock<AppState>>,
    wallet_id: &str,
    message_hash: &[u8; 32],
    explicit_parties: Option<Vec<u16>>,
) -> Result<Vec<u16>, (StatusCode, String)> {
    let node_urls = {
        let s = state.read().await;
        s.nodes.clone()
    };

    if let Some(parties) = explicit_parties {
        // Validate explicit parties
        if parties.len() < THRESHOLD as usize {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Need at least {} parties, got {}", THRESHOLD, parties.len()),
            ));
        }

        for party_index in &parties {
            if *party_index as usize >= node_urls.len() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!(
                        "Invalid party index: {} (must be 0-{})",
                        party_index,
                        node_urls.len() - 1
                    ),
                ));
            }
        }

        Ok(parties)
    } else {
        // Deterministic selection from healthy nodes
        debug!(
            "No parties specified, selecting {} nodes deterministically",
            THRESHOLD
        );

        let seed = derive_selection_seed(wallet_id, message_hash);
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_default();

        let result = select_parties_with_policy(
            &client,
            &node_urls,
            THRESHOLD as usize,
            &seed,
            SelectionPolicy::Deterministic,
        )
        .await
        .map_err(|e| (StatusCode::SERVICE_UNAVAILABLE, e))?;

        Ok(result.participants)
    }
}

// ============================================================================
// Handlers
// ============================================================================

/// Issue a signing grant without starting a session.
///
/// POST /grant/signing
///
/// This endpoint issues a grant that authorizes a signing operation.
/// The grant can then be sent to any participating node to initiate
/// a P2P signing session.
///
/// ## Request
///
/// ```json
/// {
///     "wallet_id": "abc123",
///     "message_hash": "deadbeef...",
///     "parties": [0, 1, 2]  // optional
/// }
/// ```
///
/// ## Response
///
/// ```json
/// {
///     "grant": { ... },
///     "session_id": "...",
///     "participants": [0, 1, 2],
///     "message": "Grant issued successfully"
/// }
/// ```
pub async fn issue_signing_grant(
    State(state): State<Arc<RwLock<AppState>>>,
    Json(request): Json<IssueSigningGrantRequest>,
) -> Result<Json<IssueSigningGrantResponse>, (StatusCode, String)> {
    reject_mainnet()?;

    info!("========================================");
    info!("  GRANT ISSUANCE REQUEST (P2P)");
    info!("========================================");
    info!("Wallet ID: {}", request.wallet_id);
    info!("Message hash: {}", request.message_hash);
    info!("Explicit parties: {:?}", request.parties);

    // Validate wallet exists
    let wallet_id_uuid = uuid::Uuid::parse_str(&request.wallet_id)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid wallet ID: {}", e)))?;

    {
        let s = state.read().await;
        s.get_stored_wallet(wallet_id_uuid)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or_else(|| (StatusCode::NOT_FOUND, "Wallet not found".to_string()))?;
    }

    // Parse and validate message hash
    let message_hash_bytes = parse_message_hash(&request.message_hash)?;

    // Select parties
    let parties = select_signing_parties(
        &state,
        &request.wallet_id,
        &message_hash_bytes,
        request.parties,
    )
    .await?;

    // Create the grant
    let grant = {
        let s = state.read().await;
        SigningGrant::new(
            request.wallet_id.clone(),
            message_hash_bytes,
            THRESHOLD,
            parties.clone(),
            s.grant_signing_key(),
        )
    };

    let session_id = grant.session_id();

    info!("Grant issued successfully:");
    info!("  Grant ID: {}", grant.grant_id);
    info!("  Session ID: {}", session_id);
    info!("  Participants: {:?}", parties);
    info!("  Expires at: {}", grant.expires_at);

    Ok(Json(IssueSigningGrantResponse {
        grant,
        session_id,
        participants: parties,
        message: "Grant issued successfully. Send to any participant to initiate P2P signing."
            .to_string(),
    }))
}

/// Issue a keygen grant.
///
/// POST /grant/keygen
///
/// Similar to signing grants, but for key generation operations.
#[derive(Debug, Clone, Deserialize)]
pub struct IssueKeygenGrantRequest {
    /// Wallet ID for the new key.
    pub wallet_id: String,
    /// Optional: specific parties to participate.
    #[serde(default)]
    pub parties: Option<Vec<u16>>,
}

/// Response for keygen grant.
#[derive(Debug, Clone, Serialize)]
pub struct IssueKeygenGrantResponse {
    /// The signed grant.
    pub grant: SigningGrant,
    /// Session ID.
    pub session_id: String,
    /// Selected participants.
    pub participants: Vec<u16>,
    /// Message.
    pub message: String,
}

/// Issue a keygen grant.
pub async fn issue_keygen_grant(
    State(state): State<Arc<RwLock<AppState>>>,
    Json(request): Json<IssueKeygenGrantRequest>,
) -> Result<Json<IssueKeygenGrantResponse>, (StatusCode, String)> {
    reject_mainnet()?;

    info!("========================================");
    info!("  KEYGEN GRANT ISSUANCE REQUEST (P2P)");
    info!("========================================");
    info!("Wallet ID: {}", request.wallet_id);
    info!("Explicit parties: {:?}", request.parties);

    // For keygen, use a random message hash (since there's no transaction to sign)
    let mut keygen_seed = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut keygen_seed);

    // Select parties
    let parties =
        select_signing_parties(&state, &request.wallet_id, &keygen_seed, request.parties).await?;

    // Create the grant (reusing SigningGrant structure for now)
    let grant = {
        let s = state.read().await;
        SigningGrant::new(
            request.wallet_id.clone(),
            keygen_seed,
            THRESHOLD,
            parties.clone(),
            s.grant_signing_key(),
        )
    };

    let session_id = grant.session_id();

    info!("Keygen grant issued successfully:");
    info!("  Grant ID: {}", grant.grant_id);
    info!("  Session ID: {}", session_id);
    info!("  Participants: {:?}", parties);

    Ok(Json(IssueKeygenGrantResponse {
        grant,
        session_id,
        participants: parties,
        message: "Keygen grant issued. Send to any participant to initiate P2P keygen.".to_string(),
    }))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_message_hash_valid() {
        let valid_hex = "a".repeat(64);
        let result = parse_message_hash(&valid_hex);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_message_hash_invalid_hex() {
        let invalid = "not_hex";
        let result = parse_message_hash(invalid);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_message_hash_wrong_length() {
        let short = "aabbccdd";
        let result = parse_message_hash(short);
        assert!(result.is_err());
        let (_, msg) = result.unwrap_err();
        assert!(msg.contains("32 bytes"));
    }
}
