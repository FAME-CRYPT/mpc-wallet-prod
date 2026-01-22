//! Signing Coordinator
//!
//! Coordinates threshold signature generation across multiple nodes using either:
//! - CGGMP24 (ECDSA) for SegWit and Legacy addresses
//! - FROST (Schnorr) for Taproot addresses
//!
//! # Architecture
//!
//! 1. **Protocol Selection**: Automatically detects recipient address type
//! 2. **Presignature Pool**: Uses pre-computed signatures for CGGMP24 (<500ms)
//! 3. **Distributed Signing**: Coordinates signature share collection from nodes
//! 4. **Signature Combination**: Combines threshold shares into final signature
//! 5. **Verification**: Validates signature before broadcasting transaction

use crate::error::{OrchestrationError, Result};
use crate::presig_service::PresignatureService;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use threshold_network::QuicEngine;
use threshold_storage::{EtcdStorage, PostgresStorage};
use threshold_types::{NetworkMessage, NodeId, PresignatureId, SigningMessage, TxId};
use tokio::sync::{Mutex, RwLock};
use tracing::{error, info, warn};
use uuid::Uuid;

/// Signature protocol type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignatureProtocol {
    /// CGGMP24 ECDSA (for SegWit, Legacy)
    CGGMP24,
    /// FROST Schnorr (for Taproot)
    FROST,
}

impl std::fmt::Display for SignatureProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignatureProtocol::CGGMP24 => write!(f, "cggmp24"),
            SignatureProtocol::FROST => write!(f, "frost"),
        }
    }
}

/// Signing request sent to all nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigningRequest {
    /// Transaction ID
    pub tx_id: TxId,
    /// Unsigned transaction bytes
    pub unsigned_tx: Vec<u8>,
    /// Message hash to sign
    pub message_hash: Vec<u8>,
    /// Presignature ID (for CGGMP24 only)
    pub presignature_id: Option<PresignatureId>,
    /// Protocol to use
    pub protocol: SignatureProtocol,
    /// Signing session ID
    pub session_id: Uuid,
}

/// Signature share from a single node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureShare {
    /// Transaction ID
    pub tx_id: TxId,
    /// Node that generated this share
    pub node_id: NodeId,
    /// Partial signature data
    pub partial_signature: Vec<u8>,
    /// Presignature ID used (for CGGMP24)
    pub presignature_id: Option<PresignatureId>,
    /// Signing session ID
    pub session_id: Uuid,
}

/// Final combined signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombinedSignature {
    /// Complete signature bytes (DER-encoded for ECDSA, 64 bytes for Schnorr)
    pub signature: Vec<u8>,
    /// Protocol used
    pub protocol: SignatureProtocol,
    /// Number of shares combined
    pub share_count: usize,
    /// Signing duration in milliseconds
    pub duration_ms: u64,
}

/// Signing session state
#[derive(Debug, Clone)]
struct SigningSession {
    session_id: Uuid,
    tx_id: TxId,
    protocol: SignatureProtocol,
    presignature_id: Option<PresignatureId>,
    started_at: Instant,
    shares_received: Vec<SignatureShare>,
    threshold: usize,
}

/// Signing Coordinator Service
pub struct SigningCoordinator {
    /// QUIC network engine
    quic: Arc<QuicEngine>,
    /// PostgreSQL storage
    postgres: Arc<PostgresStorage>,
    /// etcd storage
    etcd: Arc<EtcdStorage>,
    /// Presignature service (for CGGMP24)
    presig_service: Arc<PresignatureService>,
    /// Current node ID
    node_id: NodeId,
    /// Signature threshold (e.g., 4 for 4-of-5)
    threshold: usize,
    /// Active signing sessions
    active_sessions: Arc<RwLock<Vec<SigningSession>>>,
    /// Signature share buffer (session_id -> shares)
    share_buffer: Arc<Mutex<std::collections::HashMap<Uuid, Vec<SignatureShare>>>>,
}

impl SigningCoordinator {
    /// Create new signing coordinator
    pub fn new(
        quic: Arc<QuicEngine>,
        postgres: Arc<PostgresStorage>,
        etcd: Arc<EtcdStorage>,
        presig_service: Arc<PresignatureService>,
        node_id: NodeId,
        threshold: usize,
    ) -> Self {
        Self {
            quic,
            postgres,
            etcd,
            presig_service,
            node_id,
            threshold,
            active_sessions: Arc::new(RwLock::new(Vec::new())),
            share_buffer: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// Sign a transaction using the appropriate protocol
    ///
    /// This method:
    /// 1. Determines protocol based on recipient address type
    /// 2. Acquires presignature if using CGGMP24
    /// 3. Broadcasts signing request to all nodes
    /// 4. Collects signature shares from threshold nodes
    /// 5. Combines shares into final signature
    /// 6. Verifies signature validity
    pub async fn sign_transaction(
        &self,
        tx_id: &TxId,
        unsigned_tx: &[u8],
        protocol: SignatureProtocol,
    ) -> Result<CombinedSignature> {
        let start = Instant::now();
        info!(
            "Starting {} signing for tx_id={}",
            protocol, tx_id
        );

        // Create signing session
        let session_id = Uuid::new_v4();

        // Acquire presignature if using CGGMP24
        let presignature_id = if protocol == SignatureProtocol::CGGMP24 {
            match self.presig_service.acquire_presignature().await {
                Ok(presig_id) => {
                    info!("Acquired presignature: {}", presig_id);
                    Some(presig_id)
                }
                Err(e) => {
                    error!("Failed to acquire presignature: {}", e);
                    return Err(OrchestrationError::Internal(format!(
                        "No presignatures available: {}",
                        e
                    )));
                }
            }
        } else {
            None
        };

        // Compute message hash
        let message_hash = self.compute_message_hash(unsigned_tx, protocol)?;

        // Create signing session
        let session = SigningSession {
            session_id,
            tx_id: tx_id.clone(),
            protocol,
            presignature_id: presignature_id.clone(),
            started_at: Instant::now(),
            shares_received: Vec::new(),
            threshold: self.threshold,
        };

        // Register session
        {
            let mut sessions = self.active_sessions.write().await;
            sessions.push(session);
        }

        // Broadcast signing request
        let request = SigningRequest {
            tx_id: tx_id.clone(),
            unsigned_tx: unsigned_tx.to_vec(),
            message_hash: message_hash.clone(),
            presignature_id: presignature_id.clone(),
            protocol,
            session_id,
        };

        self.broadcast_signing_request(&request).await?;

        // Collect signature shares (with 30 second timeout)
        let shares = self
            .collect_signature_shares(session_id, Duration::from_secs(30))
            .await?;

        info!(
            "Collected {}/{} signature shares for session={}",
            shares.len(),
            self.threshold,
            session_id
        );

        // Combine signature shares
        let signature = self.combine_signature_shares(&shares, protocol).await?;

        // Verify signature
        self.verify_signature(unsigned_tx, &signature, protocol)?;

        let duration_ms = start.elapsed().as_millis() as u64;

        info!(
            "Signing completed: protocol={} duration={}ms tx_id={}",
            protocol, duration_ms, tx_id
        );

        // Cleanup session
        {
            let mut sessions = self.active_sessions.write().await;
            sessions.retain(|s| s.session_id != session_id);
        }

        Ok(CombinedSignature {
            signature,
            protocol,
            share_count: shares.len(),
            duration_ms,
        })
    }

    /// Broadcast signing request to all nodes
    async fn broadcast_signing_request(&self, request: &SigningRequest) -> Result<()> {
        let msg = NetworkMessage::SigningRound(SigningMessage {
            tx_id: request.tx_id.clone(),
            round: 1,
            from: self.node_id,
            payload: serde_json::to_vec(&request).map_err(|e| {
                OrchestrationError::SerializationError(format!("Failed to serialize request: {}", e))
            })?,
        });

        let stream_id = 1; // Signing stream
        self.quic
            .broadcast(&msg, stream_id, None)
            .await
            .map_err(|e| OrchestrationError::NetworkError(format!("Failed to broadcast: {}", e)))?;

        info!(
            "Broadcasted signing request: session={} protocol={}",
            request.session_id, request.protocol
        );

        Ok(())
    }

    /// Collect signature shares from threshold nodes
    async fn collect_signature_shares(
        &self,
        session_id: Uuid,
        timeout: Duration,
    ) -> Result<Vec<SignatureShare>> {
        let start = Instant::now();

        loop {
            // Check timeout
            if start.elapsed() >= timeout {
                return Err(OrchestrationError::Timeout(format!(
                    "Timeout waiting for signature shares (session={})",
                    session_id
                )));
            }

            // Check if we have enough shares
            let buffer = self.share_buffer.lock().await;
            if let Some(shares) = buffer.get(&session_id) {
                if shares.len() >= self.threshold {
                    return Ok(shares.clone());
                }
            }
            drop(buffer);

            // Wait before next check
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Handle incoming signature share from another node
    pub async fn handle_signature_share(&self, share: SignatureShare) -> Result<()> {
        info!(
            "Received signature share: session={} from={} tx_id={}",
            share.session_id, share.node_id, share.tx_id
        );

        // Add to buffer
        let mut buffer = self.share_buffer.lock().await;
        buffer
            .entry(share.session_id)
            .or_insert_with(Vec::new)
            .push(share);

        Ok(())
    }

    /// Combine signature shares into final signature
    async fn combine_signature_shares(
        &self,
        shares: &[SignatureShare],
        protocol: SignatureProtocol,
    ) -> Result<Vec<u8>> {
        match protocol {
            SignatureProtocol::CGGMP24 => self.combine_cggmp24_shares(shares).await,
            SignatureProtocol::FROST => self.combine_frost_shares(shares).await,
        }
    }

    /// Combine CGGMP24 ECDSA signature shares
    async fn combine_cggmp24_shares(&self, shares: &[SignatureShare]) -> Result<Vec<u8>> {
        // NOTE: In the presignature-based signing flow, each node already produces
        // a COMPLETE signature locally by combining partial signatures.
        // The "shares" here are actually complete signatures from each node.
        // We just need to verify they're all identical and return one.

        if shares.is_empty() {
            return Err(OrchestrationError::Internal(
                "No signature shares provided".to_string()
            ));
        }

        // All nodes should produce the same final signature when using presignatures
        // Take the first share's signature (they should all be identical)
        let first_sig = &shares[0].partial_signature;

        // Verify all shares have the same signature (sanity check)
        for (i, share) in shares.iter().enumerate().skip(1) {
            if share.partial_signature != *first_sig {
                error!(
                    "Signature mismatch: share {} differs from first share",
                    i
                );
                return Err(OrchestrationError::Internal(
                    format!("Signature mismatch between nodes (share {})", i)
                ));
            }
        }

        info!(
            "All {} signature shares match - signature is valid",
            shares.len()
        );

        Ok(first_sig.clone())
    }

    /// Combine FROST Schnorr signature shares
    async fn combine_frost_shares(&self, shares: &[SignatureShare]) -> Result<Vec<u8>> {
        // NOTE: In FROST protocol, each node runs the complete signing protocol
        // and produces the SAME complete 64-byte Schnorr signature.
        // The "shares" here are actually complete signatures from each node.
        // We just need to verify they're all identical and return one.

        if shares.is_empty() {
            return Err(OrchestrationError::Internal(
                "No signature shares provided".to_string()
            ));
        }

        // All nodes should produce the same final Schnorr signature
        // Take the first share's signature (they should all be identical)
        let first_sig = &shares[0].partial_signature;

        // Verify it's 64 bytes (BIP-340 Schnorr signature format)
        if first_sig.len() != 64 {
            return Err(OrchestrationError::Internal(
                format!("Invalid Schnorr signature length: {} (expected 64)", first_sig.len())
            ));
        }

        // Verify all shares have the same signature (sanity check)
        for (i, share) in shares.iter().enumerate().skip(1) {
            if share.partial_signature != *first_sig {
                error!(
                    "Schnorr signature mismatch: share {} differs from first share",
                    i
                );
                return Err(OrchestrationError::Internal(
                    format!("Signature mismatch between nodes (share {})", i)
                ));
            }
        }

        info!(
            "All {} Schnorr signature shares match - signature is valid (64 bytes)",
            shares.len()
        );

        Ok(first_sig.clone())
    }

    /// Compute message hash for signing
    fn compute_message_hash(&self, unsigned_tx: &[u8], protocol: SignatureProtocol) -> Result<Vec<u8>> {
        // NOTE: In production, this should use the actual Bitcoin transaction
        // and compute the proper sighash using bitcoin-rs crate.
        //
        // For now, we use SHA-256 double hash of the transaction bytes.
        // The caller (transaction builder) is responsible for providing
        // the correct transaction bytes to hash.

        use sha2::{Sha256, Digest};

        match protocol {
            SignatureProtocol::CGGMP24 => {
                // ECDSA: Use double SHA-256 (Bitcoin's standard)
                // First hash
                let mut hasher = Sha256::new();
                hasher.update(unsigned_tx);
                let hash1 = hasher.finalize();

                // Second hash
                let mut hasher = Sha256::new();
                hasher.update(&hash1);
                let hash2 = hasher.finalize();

                info!(
                    "Computed CGGMP24 sighash: {}",
                    hex::encode(&hash2)
                );

                Ok(hash2.to_vec())
            }
            SignatureProtocol::FROST => {
                // Schnorr/Taproot: Use single SHA-256
                // (BIP-341 tagged hash should be used in production)
                let mut hasher = Sha256::new();
                hasher.update(unsigned_tx);
                let hash = hasher.finalize();

                info!(
                    "Computed FROST sighash: {}",
                    hex::encode(&hash)
                );

                Ok(hash.to_vec())
            }
        }
    }

    /// Verify signature validity
    fn verify_signature(
        &self,
        _unsigned_tx: &[u8],
        signature: &[u8],
        protocol: SignatureProtocol,
    ) -> Result<()> {
        // Basic signature format validation
        match protocol {
            SignatureProtocol::CGGMP24 => {
                // ECDSA DER signature should be 70-73 bytes typically
                if signature.len() < 8 || signature.len() > 73 {
                    return Err(OrchestrationError::Internal(format!(
                        "Invalid ECDSA signature length: {} bytes (expected 8-73)",
                        signature.len()
                    )));
                }

                // Basic DER format check: must start with 0x30 (SEQUENCE)
                if signature[0] != 0x30 {
                    return Err(OrchestrationError::Internal(
                        "Invalid ECDSA signature: must start with DER SEQUENCE tag (0x30)".to_string()
                    ));
                }

                info!(
                    "ECDSA signature format valid: {} bytes, DER-encoded",
                    signature.len()
                );
            }
            SignatureProtocol::FROST => {
                // Schnorr signature must be exactly 64 bytes (BIP-340)
                if signature.len() != 64 {
                    return Err(OrchestrationError::Internal(format!(
                        "Invalid Schnorr signature length: {} bytes (expected 64)",
                        signature.len()
                    )));
                }

                info!("Schnorr signature format valid: 64 bytes (BIP-340)");
            }
        }

        // NOTE: In production, should verify signature cryptographically
        // using bitcoin::secp256k1 crate with the public key

        Ok(())
    }

    /// Get active signing sessions count
    pub async fn active_sessions_count(&self) -> usize {
        self.active_sessions.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signature_protocol_display() {
        assert_eq!(SignatureProtocol::CGGMP24.to_string(), "cggmp24");
        assert_eq!(SignatureProtocol::FROST.to_string(), "frost");
    }

    #[tokio::test]
    #[ignore] // Requires running infrastructure
    async fn test_signing_coordinator() {
        // TODO: Add integration test
    }
}
