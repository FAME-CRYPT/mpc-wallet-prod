use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use threshold_types::{Result, Vote, VotingError};

pub struct KeyPair {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

impl KeyPair {
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        Self {
            signing_key,
            verifying_key,
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 32 {
            return Err(VotingError::ConfigError(
                "Invalid key length: expected 32 bytes".to_string(),
            ));
        }

        let signing_key = SigningKey::from_bytes(
            bytes
                .try_into()
                .map_err(|_| VotingError::ConfigError("Failed to parse signing key".to_string()))?,
        );
        let verifying_key = signing_key.verifying_key();

        Ok(Self {
            signing_key,
            verifying_key,
        })
    }

    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        let signature = self.signing_key.sign(message);
        signature.to_bytes().to_vec()
    }

    pub fn public_key(&self) -> Vec<u8> {
        self.verifying_key.to_bytes().to_vec()
    }

    pub fn private_key(&self) -> Vec<u8> {
        self.signing_key.to_bytes().to_vec()
    }
}

pub fn verify_signature(public_key: &[u8], message: &[u8], signature: &[u8]) -> Result<()> {
    if public_key.len() != 32 {
        return Err(VotingError::InvalidSignature);
    }

    if signature.len() != 64 {
        return Err(VotingError::InvalidSignature);
    }

    let verifying_key = VerifyingKey::from_bytes(
        public_key
            .try_into()
            .map_err(|_| VotingError::InvalidSignature)?,
    )
    .map_err(|_| VotingError::InvalidSignature)?;

    let signature = Signature::from_bytes(
        signature
            .try_into()
            .map_err(|_| VotingError::InvalidSignature)?,
    );

    verifying_key
        .verify(message, &signature)
        .map_err(|_| VotingError::InvalidSignature)
}

pub fn verify_vote(vote: &Vote) -> Result<()> {
    let message = vote.message_to_sign();
    verify_signature(&vote.public_key, &message, &vote.signature)
}

pub fn hash_data(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use threshold_types::{NodeId, PeerId, TransactionId};

    #[test]
    fn test_key_generation() {
        let keypair = KeyPair::generate();
        assert_eq!(keypair.public_key().len(), 32);
        assert_eq!(keypair.private_key().len(), 32);
    }

    #[test]
    fn test_sign_and_verify() {
        let keypair = KeyPair::generate();
        let message = b"test message";
        let signature = keypair.sign(message);

        assert!(verify_signature(&keypair.public_key(), message, &signature).is_ok());
    }

    #[test]
    fn test_invalid_signature() {
        let keypair = KeyPair::generate();
        let message = b"test message";
        let mut signature = keypair.sign(message);

        signature[0] ^= 1;

        assert!(verify_signature(&keypair.public_key(), message, &signature).is_err());
    }

    #[test]
    fn test_vote_verification() {
        let keypair = KeyPair::generate();
        let tx_id = TransactionId::from("tx_001");
        let node_id = NodeId::from("node_1");
        let peer_id = PeerId::from("peer_1");
        let value = 42u64;

        let message = format!("{}||{}", tx_id, value);
        let signature = keypair.sign(message.as_bytes());

        let vote = Vote::new(
            tx_id,
            node_id,
            peer_id,
            value,
            signature,
            keypair.public_key(),
        );

        assert!(verify_vote(&vote).is_ok());
    }

    #[test]
    fn test_hash_data() {
        let data = b"test data";
        let hash1 = hash_data(data);
        let hash2 = hash_data(data);

        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 32);
    }
}
