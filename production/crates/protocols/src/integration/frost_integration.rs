//! FROST Protocol Integration
//!
//! Integration layer for the FROST Schnorr threshold signature protocol using Givre.
//!
//! # References
//! - Library: `givre` v0.2 (https://github.com/LFDT-Lockness/givre)
//! - Paper: https://eprint.iacr.org/2020/852 (FROST)

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use givre::{
    ciphersuite::Secp256k1,
    KeyShare, PublicKey, Signature,
};
use round_based::{IsCritical, Msg, StateMachine};
use futures::{SinkExt, StreamExt, Stream, Sink};
use rand::rngs::StdRng;
use rand::SeedableRng;

/// FROST key share (encrypted)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrostKeyShare {
    /// Party index (0-based)
    pub party_index: u16,
    /// Threshold (t)
    pub threshold: u16,
    /// Total parties (n)
    pub total_parties: u16,
    /// Encrypted key share data
    pub encrypted_data: Vec<u8>,
    /// X-only public key (32 bytes, BIP-340)
    pub x_only_public_key: Vec<u8>,
}

/// FROST partial signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrostPartialSignature {
    /// Party index
    pub party_index: u16,
    /// Partial signature data
    pub data: Vec<u8>,
}

/// FROST signing nonce (round 1 output)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrostNonce {
    /// Party index
    pub party_index: u16,
    /// Nonce commitment data
    pub commitment: Vec<u8>,
}

/// Run FROST DKG protocol using Givre
///
/// # Arguments
/// * `party_index` - This party's index (0-based)
/// * `threshold` - Signing threshold (e.g., 4 for 4-of-5)
/// * `total_parties` - Total number of parties (e.g., 5)
/// * `incoming` - Stream of incoming messages
/// * `outgoing` - Sink to send outgoing messages
///
/// # Returns
/// Key share and x-only public key (BIP-340 format)
pub async fn run_frost_dkg<S, Si>(
    party_index: u16,
    threshold: u16,
    total_parties: u16,
    mut incoming: S,
    mut outgoing: Si,
) -> Result<FrostKeyShare>
where
    S: Stream<Item = Result<Msg<givre::keygen::msg_type::Msg<Secp256k1>>>> + Unpin,
    Si: Sink<Msg<givre::keygen::msg_type::Msg<Secp256k1>>, Error = anyhow::Error> + Unpin,
{
    // Validate parameters
    if party_index >= total_parties {
        bail!("party_index {} must be < total_parties {}", party_index, total_parties);
    }
    if threshold > total_parties {
        bail!("threshold {} must be <= total_parties {}", threshold, total_parties);
    }

    // Initialize RNG from secure source
    let mut rng = StdRng::from_entropy();

    // Create FROST DKG protocol state machine
    let mut state_machine = givre::keygen::KeygenProtocol::<Secp256k1>::new(
        party_index,
        threshold,
        total_parties,
        &mut rng,
    )?;

    // Run the state machine until completion
    loop {
        // Check if protocol is finished
        if state_machine.is_finished() {
            break;
        }

        // Check if we want to send messages
        if state_machine.wants_to_proceed() {
            // Get messages to send
            let messages = state_machine.message_queue();

            // Send all queued messages
            for msg in messages {
                outgoing.send(msg).await?;
            }
        }

        // Check if there are errors
        if let Some(err) = state_machine.pick_error() {
            if err.is_critical() {
                return Err(anyhow!("Critical FROST DKG error: {:?}", err));
            }
            tracing::warn!("Non-critical FROST DKG error: {:?}", err);
        }

        // Wait for incoming message
        if let Some(msg_result) = incoming.next().await {
            let msg = msg_result?;
            state_machine.handle_incoming(msg)?;
        } else {
            bail!("Incoming message stream closed unexpectedly");
        }
    }

    // Extract the result
    let key_share: KeyShare<Secp256k1> = state_machine
        .pick_output()
        .ok_or_else(|| anyhow!("FROST DKG completed but no output available"))?;

    // Serialize key share for storage
    let encrypted_data = bincode::serialize(&key_share)
        .map_err(|e| anyhow!("Failed to serialize key share: {}", e))?;

    // Extract x-only public key (32 bytes) for Taproot/BIP-340
    let public_key_point = key_share.shared_public_key();
    let x_only_public_key = public_key_point.x_only().to_vec();

    if x_only_public_key.len() != 32 {
        bail!("Expected 32-byte x-only public key, got {}", x_only_public_key.len());
    }

    Ok(FrostKeyShare {
        party_index,
        threshold,
        total_parties,
        encrypted_data,
        x_only_public_key,
    })
}

/// Generate FROST partial signature (round 1: nonce generation)
///
/// # Arguments
/// * `key_share` - Key share from DKG
/// * `message` - 32-byte message hash to sign
/// * `incoming` - Stream of incoming messages
/// * `outgoing` - Sink to send outgoing messages
///
/// # Returns
/// Nonce commitment to be broadcast
pub async fn frost_signing_round1<S, Si>(
    key_share: &FrostKeyShare,
    message: &[u8; 32],
    mut incoming: S,
    mut outgoing: Si,
) -> Result<(FrostNonce, Vec<u8>)> // Returns (nonce for broadcasting, secret state)
where
    S: Stream<Item = Result<Msg<givre::signing::msg_type::Msg<Secp256k1>>>> + Unpin,
    Si: Sink<Msg<givre::signing::msg_type::Msg<Secp256k1>>, Error = anyhow::Error> + Unpin,
{
    // Deserialize key share
    let frost_key_share: KeyShare<Secp256k1> = bincode::deserialize(&key_share.encrypted_data)
        .map_err(|e| anyhow!("Failed to deserialize key share: {}", e))?;

    // Initialize RNG
    let mut rng = StdRng::from_entropy();

    // Start signing protocol - this generates nonce commitments
    let mut state_machine = givre::signing::InteractiveSigningBuilder::<Secp256k1>::new(
        frost_key_share.clone(),
        message,
    )
    .set_threshold(key_share.threshold)
    .start_signing(&mut rng)?;

    // Run the state machine for round 1 (nonce generation)
    loop {
        if state_machine.is_finished() {
            break;
        }

        if state_machine.wants_to_proceed() {
            let messages = state_machine.message_queue();
            for msg in messages {
                outgoing.send(msg).await?;
            }
        }

        if let Some(err) = state_machine.pick_error() {
            if err.is_critical() {
                return Err(anyhow!("Critical FROST signing error: {:?}", err));
            }
            tracing::warn!("Non-critical FROST signing error: {:?}", err);
        }

        if let Some(msg_result) = incoming.next().await {
            let msg = msg_result?;
            state_machine.handle_incoming(msg)?;
        } else {
            bail!("Incoming message stream closed unexpectedly");
        }
    }

    // Extract nonce commitment and secret state
    let round1_output = state_machine
        .pick_output()
        .ok_or_else(|| anyhow!("FROST signing round 1 completed but no output available"))?;

    // Serialize nonce commitment and secret state
    let commitment = bincode::serialize(&round1_output.nonce_commitment)
        .map_err(|e| anyhow!("Failed to serialize nonce commitment: {}", e))?;

    let secret_state = bincode::serialize(&round1_output.secret_state)
        .map_err(|e| anyhow!("Failed to serialize secret state: {}", e))?;

    Ok((
        FrostNonce {
            party_index: key_share.party_index,
            commitment,
        },
        secret_state,
    ))
}

/// Generate FROST partial signature (round 2: signature share generation)
///
/// # Arguments
/// * `key_share` - Key share from DKG
/// * `round1_secret` - Secret from round 1
/// * `nonce_commitments` - Nonce commitments from all signers
/// * `message` - 32-byte message hash to sign
///
/// # Returns
/// Partial signature from this party
pub fn frost_signing_round2(
    key_share: &FrostKeyShare,
    round1_secret: &[u8],
    nonce_commitments: &[FrostNonce],
    message: &[u8; 32],
) -> Result<FrostPartialSignature> {
    // Validate nonce commitments
    if nonce_commitments.len() < key_share.threshold as usize {
        bail!("Need at least {} nonce commitments, got {}", key_share.threshold, nonce_commitments.len());
    }

    // Deserialize key share
    let frost_key_share: KeyShare<Secp256k1> = bincode::deserialize(&key_share.encrypted_data)
        .map_err(|e| anyhow!("Failed to deserialize key share: {}", e))?;

    // Deserialize round1 secret state
    let secret_state: givre::signing::Round1SecretState<Secp256k1> = bincode::deserialize(round1_secret)
        .map_err(|e| anyhow!("Failed to deserialize round1 secret: {}", e))?;

    // Deserialize nonce commitments from other parties
    let mut commitments = Vec::new();
    for nonce in nonce_commitments {
        let commitment: givre::signing::NonceCommitment<Secp256k1> = bincode::deserialize(&nonce.commitment)
            .map_err(|e| anyhow!("Failed to deserialize nonce commitment: {}", e))?;
        commitments.push(commitment);
    }

    // Generate partial signature
    let partial_sig = givre::signing::round2(
        &frost_key_share,
        secret_state,
        &commitments,
        message,
    )?;

    // Serialize partial signature
    let data = bincode::serialize(&partial_sig)
        .map_err(|e| anyhow!("Failed to serialize partial signature: {}", e))?;

    Ok(FrostPartialSignature {
        party_index: key_share.party_index,
        data,
    })
}

/// Combine FROST partial signatures
///
/// # Arguments
/// * `partial_signatures` - Threshold count of partial signatures
/// * `nonce_commitments` - Nonce commitments from round 1
/// * `message` - The message hash that was signed
/// * `public_key` - The x-only public key (32 bytes)
///
/// # Returns
/// Final Schnorr signature (64 bytes, BIP-340 format)
pub fn combine_frost_signatures(
    partial_signatures: &[FrostPartialSignature],
    nonce_commitments: &[FrostNonce],
    message: &[u8; 32],
    public_key: &[u8],
) -> Result<Vec<u8>> {
    // Validate we have enough signatures
    if partial_signatures.is_empty() {
        bail!("No partial signatures provided");
    }

    if nonce_commitments.is_empty() {
        bail!("No nonce commitments provided");
    }

    // Deserialize all partial signatures
    let mut sigs: Vec<givre::signing::SignatureShare<Secp256k1>> = Vec::new();
    for partial in partial_signatures {
        let sig: givre::signing::SignatureShare<Secp256k1> = bincode::deserialize(&partial.data)
            .map_err(|e| anyhow!("Failed to deserialize partial signature: {}", e))?;
        sigs.push(sig);
    }

    // Deserialize nonce commitments
    let mut commitments = Vec::new();
    for nonce in nonce_commitments {
        let commitment: givre::signing::NonceCommitment<Secp256k1> = bincode::deserialize(&nonce.commitment)
            .map_err(|e| anyhow!("Failed to deserialize nonce commitment: {}", e))?;
        commitments.push(commitment);
    }

    // Deserialize public key
    let pubkey: PublicKey<Secp256k1> = PublicKey::from_x_only(public_key)
        .map_err(|e| anyhow!("Failed to parse public key: {}", e))?;

    // Aggregate partial signatures into final Schnorr signature
    let signature: Signature<Secp256k1> = givre::signing::aggregate(
        &pubkey,
        &sigs,
        &commitments,
        message,
    )?;

    // Convert to BIP-340 format (64 bytes: r || s)
    let sig_bytes = signature.to_bytes();

    if sig_bytes.len() != 64 {
        bail!("Expected 64-byte BIP-340 signature, got {}", sig_bytes.len());
    }

    Ok(sig_bytes.to_vec())
}

/// Verify FROST/Schnorr signature (BIP-340)
///
/// # Arguments
/// * `signature` - 64-byte Schnorr signature
/// * `message` - 32-byte message that was signed
/// * `x_only_pubkey` - 32-byte x-only public key
///
/// # Returns
/// true if signature is valid
pub fn verify_frost_signature(
    signature: &[u8],
    message: &[u8; 32],
    x_only_pubkey: &[u8],
) -> Result<bool> {
    // Validate inputs
    if signature.len() != 64 {
        bail!("Invalid Schnorr signature length: expected 64, got {}", signature.len());
    }

    if x_only_pubkey.len() != 32 {
        bail!("Invalid x-only pubkey length: expected 32, got {}", x_only_pubkey.len());
    }

    // Parse public key
    let pubkey = PublicKey::<Secp256k1>::from_x_only(x_only_pubkey)
        .map_err(|e| anyhow!("Failed to parse x-only public key: {}", e))?;

    // Parse signature
    let sig = Signature::<Secp256k1>::from_bytes(signature)
        .map_err(|e| anyhow!("Failed to parse signature: {}", e))?;

    // Verify BIP-340 Schnorr signature
    Ok(givre::signing::verify(&pubkey, &sig, message))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_frost_signature_validation() {
        // Test input validation
        let invalid_sig = vec![0u8; 32]; // Wrong length
        let valid_msg = [0u8; 32];
        let valid_pubkey = [0u8; 32];

        let result = verify_frost_signature(&invalid_sig, &valid_msg, &valid_pubkey);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid Schnorr signature length"));
    }
}
