//! Grant system for MPC signing authorization.
//!
//! Grants are signed authorization artifacts that nodes verify before
//! participating in signing operations. This ensures the coordinator
//! is non-authoritative - it can relay messages but cannot authorize
//! signing without a valid grant.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Default grant validity duration in seconds (5 minutes).
pub const DEFAULT_GRANT_VALIDITY_SECS: u64 = 300;

/// Domain separator for grant signatures to prevent cross-protocol confusion.
/// Format: "torcus-wallet:signing-grant:v1"
const GRANT_DOMAIN_TAG: &[u8] = b"torcus-wallet:signing-grant:v1";

/// A signing grant authorizes nodes to participate in a signing session.
///
/// The grant is signed by the grant issuer (coordinator for now, policy
/// engine in the future). Nodes verify the signature before participating.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigningGrant {
    /// Unique identifier for this grant.
    pub grant_id: Uuid,

    /// The wallet to sign with.
    pub wallet_id: String,

    /// The message hash to sign (32 bytes).
    #[serde(with = "hex_array")]
    pub message_hash: [u8; 32],

    /// Threshold required for signing.
    pub threshold: u16,

    /// Party indices that should participate.
    pub participants: Vec<u16>,

    /// Unique nonce to prevent replay attacks.
    pub nonce: u64,

    /// Unix timestamp when the grant was issued.
    pub issued_at: u64,

    /// Unix timestamp when the grant expires.
    pub expires_at: u64,

    /// Ed25519 signature over the grant data.
    #[serde(with = "hex_array_64")]
    pub signature: [u8; 64],
}

/// Data that gets signed (everything except the signature itself).
#[derive(Debug, Clone, Serialize)]
struct SigningGrantData {
    pub grant_id: Uuid,
    pub wallet_id: String,
    #[serde(with = "hex_array")]
    pub message_hash: [u8; 32],
    pub threshold: u16,
    pub participants: Vec<u16>,
    pub nonce: u64,
    pub issued_at: u64,
    pub expires_at: u64,
}

impl SigningGrant {
    /// Create a new signing grant and sign it with the provided key.
    ///
    /// Note: Participants are sorted before signing to ensure canonical ordering.
    /// This means the signature commits to a specific set of participants, not
    /// a specific ordering, which matches the order-insensitive validation on nodes.
    pub fn new(
        wallet_id: String,
        message_hash: [u8; 32],
        threshold: u16,
        mut participants: Vec<u16>,
        signing_key: &SigningKey,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Canonicalize participant order before signing
        participants.sort();

        let mut grant = Self {
            grant_id: Uuid::new_v4(),
            wallet_id,
            message_hash,
            threshold,
            participants,
            nonce: rand::random(),
            issued_at: now,
            expires_at: now + DEFAULT_GRANT_VALIDITY_SECS,
            signature: [0u8; 64],
        };

        grant.sign(signing_key);
        grant
    }

    /// Create a grant with custom expiry.
    ///
    /// Note: Participants are sorted before signing to ensure canonical ordering,
    /// consistent with `new()`.
    pub fn with_expiry(
        wallet_id: String,
        message_hash: [u8; 32],
        threshold: u16,
        mut participants: Vec<u16>,
        validity_secs: u64,
        signing_key: &SigningKey,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Canonicalize participant order before signing (same as new())
        participants.sort();

        let mut grant = Self {
            grant_id: Uuid::new_v4(),
            wallet_id,
            message_hash,
            threshold,
            participants,
            nonce: rand::random(),
            issued_at: now,
            expires_at: now + validity_secs,
            signature: [0u8; 64],
        };

        grant.sign(signing_key);
        grant
    }

    /// Compute the canonical bytes to sign/verify.
    ///
    /// Includes a domain tag to prevent signature confusion with other protocols.
    fn signable_bytes(&self) -> Vec<u8> {
        let data = SigningGrantData {
            grant_id: self.grant_id,
            wallet_id: self.wallet_id.clone(),
            message_hash: self.message_hash,
            threshold: self.threshold,
            participants: self.participants.clone(),
            nonce: self.nonce,
            issued_at: self.issued_at,
            expires_at: self.expires_at,
        };

        // Use JSON for deterministic serialization
        let json = serde_json::to_string(&data).expect("grant data serialization failed");

        // Hash with domain tag to prevent cross-protocol signature confusion
        let mut hasher = Sha256::new();
        hasher.update(GRANT_DOMAIN_TAG);
        hasher.update(b":");
        hasher.update(json.as_bytes());
        hasher.finalize().to_vec()
    }

    /// Sign the grant with the provided key.
    fn sign(&mut self, signing_key: &SigningKey) {
        let bytes = self.signable_bytes();
        let signature = signing_key.sign(&bytes);
        self.signature = signature.to_bytes();
    }

    /// Verify the grant signature.
    pub fn verify(&self, verifying_key: &VerifyingKey) -> Result<(), GrantError> {
        let bytes = self.signable_bytes();
        let signature = Signature::from_bytes(&self.signature);

        verifying_key
            .verify(&bytes, &signature)
            .map_err(|_| GrantError::InvalidSignature)
    }

    /// Check if the grant has expired.
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        now > self.expires_at
    }

    /// Validate the grant (signature + expiry + participants).
    pub fn validate(
        &self,
        verifying_key: &VerifyingKey,
        party_index: u16,
    ) -> Result<(), GrantError> {
        // Check signature first
        self.verify(verifying_key)?;

        // Check expiry
        if self.is_expired() {
            return Err(GrantError::Expired);
        }

        // Check for duplicate participants
        let mut seen = std::collections::HashSet::new();
        for &p in &self.participants {
            if !seen.insert(p) {
                return Err(GrantError::DuplicateParticipant(p));
            }
        }

        // Check if this party is a participant
        if !self.participants.contains(&party_index) {
            return Err(GrantError::NotParticipant(party_index));
        }

        // Check threshold is achievable
        if self.participants.len() < self.threshold as usize {
            return Err(GrantError::InsufficientParticipants {
                required: self.threshold,
                provided: self.participants.len() as u16,
            });
        }

        Ok(())
    }

    /// Derive a deterministic session ID from the grant.
    ///
    /// This ensures the same grant always produces the same session ID,
    /// enabling idempotency and replay detection.
    pub fn session_id(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.grant_id.as_bytes());
        hasher.update(self.nonce.to_le_bytes());
        let hash = hasher.finalize();
        format!("grant-{}", hex::encode(&hash[..16]))
    }
}

/// Errors that can occur during grant validation.
#[derive(Debug, Clone, thiserror::Error)]
pub enum GrantError {
    #[error("invalid grant signature")]
    InvalidSignature,

    #[error("grant has expired")]
    Expired,

    #[error("party {0} is not a participant in this grant")]
    NotParticipant(u16),

    #[error("duplicate participant: party {0} appears multiple times")]
    DuplicateParticipant(u16),

    #[error("insufficient participants: need {required}, got {provided}")]
    InsufficientParticipants { required: u16, provided: u16 },
}

/// Helper module for serializing [u8; 32] as hex.
mod hex_array {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("expected 32 bytes"))
    }
}

/// Helper module for serializing [u8; 64] as hex.
mod hex_array_64 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; 64], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 64], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("expected 64 bytes"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[test]
    fn test_grant_creation_and_verification() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        let grant = SigningGrant::new(
            "test-wallet".to_string(),
            [0u8; 32],
            3,
            vec![0, 1, 2],
            &signing_key,
        );

        // Should verify with correct key
        assert!(grant.verify(&verifying_key).is_ok());

        // Should not be expired
        assert!(!grant.is_expired());

        // Should validate for participant
        assert!(grant.validate(&verifying_key, 0).is_ok());
        assert!(grant.validate(&verifying_key, 1).is_ok());
        assert!(grant.validate(&verifying_key, 2).is_ok());

        // Should fail for non-participant
        assert!(matches!(
            grant.validate(&verifying_key, 3),
            Err(GrantError::NotParticipant(3))
        ));
    }

    #[test]
    fn test_grant_invalid_signature() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let other_key = SigningKey::generate(&mut OsRng);
        let other_verifying_key = other_key.verifying_key();

        let grant = SigningGrant::new(
            "test-wallet".to_string(),
            [0u8; 32],
            3,
            vec![0, 1, 2],
            &signing_key,
        );

        // Should fail with wrong key
        assert!(matches!(
            grant.verify(&other_verifying_key),
            Err(GrantError::InvalidSignature)
        ));
    }

    #[test]
    fn test_grant_session_id_deterministic() {
        let signing_key = SigningKey::generate(&mut OsRng);

        let grant = SigningGrant::new(
            "test-wallet".to_string(),
            [0u8; 32],
            3,
            vec![0, 1, 2],
            &signing_key,
        );

        let session_id1 = grant.session_id();
        let session_id2 = grant.session_id();

        assert_eq!(session_id1, session_id2);
        assert!(session_id1.starts_with("grant-"));
    }

    #[test]
    fn test_grant_serialization() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        let grant = SigningGrant::new(
            "test-wallet".to_string(),
            [0xab; 32],
            3,
            vec![0, 1, 2],
            &signing_key,
        );

        // Serialize to JSON
        let json = serde_json::to_string(&grant).unwrap();

        // Deserialize back
        let restored: SigningGrant = serde_json::from_str(&json).unwrap();

        // Should still verify
        assert!(restored.verify(&verifying_key).is_ok());
        assert_eq!(grant.grant_id, restored.grant_id);
        assert_eq!(grant.wallet_id, restored.wallet_id);
        assert_eq!(grant.message_hash, restored.message_hash);
    }
}
