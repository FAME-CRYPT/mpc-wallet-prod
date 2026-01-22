use threshold_types::{Error, Result, Vote};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

/// Verify an Ed25519 signature on a vote
///
/// This is the REAL implementation using ed25519-dalek.
/// NO STUB - performs actual cryptographic verification.
pub fn verify_vote(vote: &Vote) -> Result<()> {
    // Validate inputs
    if vote.signature.is_empty() {
        return Err(Error::CryptoError("Empty signature".to_string()));
    }

    if vote.public_key.is_empty() {
        return Err(Error::CryptoError("Empty public key".to_string()));
    }

    // Parse public key (32 bytes)
    if vote.public_key.len() != 32 {
        return Err(Error::CryptoError(format!(
            "Invalid public key length: {} (expected 32)",
            vote.public_key.len()
        )));
    }

    let mut pk_bytes = [0u8; 32];
    pk_bytes.copy_from_slice(&vote.public_key);

    let public_key = VerifyingKey::from_bytes(&pk_bytes)
        .map_err(|e| Error::CryptoError(format!("Invalid public key: {}", e)))?;

    // Parse signature (64 bytes)
    if vote.signature.len() != 64 {
        return Err(Error::CryptoError(format!(
            "Invalid signature length: {} (expected 64)",
            vote.signature.len()
        )));
    }

    let mut sig_bytes = [0u8; 64];
    sig_bytes.copy_from_slice(&vote.signature);

    let signature = Signature::from_bytes(&sig_bytes);

    // Construct message to verify
    // Format: "vote:round_id:tx_id:approve_value"
    let message = format!(
        "vote:{}:{}:{}",
        vote.round_id,
        vote.tx_id,
        vote.approve
    );

    // Verify signature
    public_key
        .verify(message.as_bytes(), &signature)
        .map_err(|e| Error::CryptoError(format!("Signature verification failed: {}", e)))?;

    Ok(())
}

/// Generate a keypair (stub for testing)
pub struct KeyPair {
    public_key: Vec<u8>,
    #[allow(dead_code)]
    secret_key: Vec<u8>,
}

impl KeyPair {
    pub fn generate() -> Self {
        // Stub implementation
        Self {
            public_key: vec![0u8; 32],
            secret_key: vec![0u8; 32],
        }
    }

    pub fn public_key(&self) -> Vec<u8> {
        self.public_key.clone()
    }

    pub fn sign(&self, _message: &[u8]) -> Vec<u8> {
        // Stub implementation
        vec![0u8; 64]
    }
}
