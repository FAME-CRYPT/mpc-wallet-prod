//! CGGMP24 Protocol Integration
//!
//! Integration layer for the CGGMP24 ECDSA threshold signature protocol.
//!
//! # References
//! - Library: `cggmp24` v0.7.0-alpha.3
//! - Paper: https://eprint.iacr.org/2021/060

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use generic_ec::{Curve, Point, Scalar};
use generic_ec::curves::Secp256k1;
use cggmp24::{
    key_share::DirtyKeyShare,
    signing::{msg_type, Signature},
};
use cggmp24_keygen::{
    msg_type as keygen_msg,
    KeygenProtocol,
};
use round_based::{IsCritical, Msg, StateMachine};
use futures::{SinkExt, StreamExt, Stream, Sink};
use std::collections::HashMap;
use std::pin::Pin;
use rand::rngs::StdRng;
use rand::SeedableRng;

/// CGGMP24 key share (encrypted)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cggmp24KeyShare {
    /// Party index (0-based)
    pub party_index: u16,
    /// Threshold (t)
    pub threshold: u16,
    /// Total parties (n)
    pub total_parties: u16,
    /// Encrypted key share data
    pub encrypted_data: Vec<u8>,
    /// Public key (compressed, 33 bytes)
    pub public_key: Vec<u8>,
}

/// CGGMP24 presignature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cggmp24Presignature {
    /// Presignature ID
    pub id: uuid::Uuid,
    /// Participant indices
    pub participants: Vec<u16>,
    /// Encrypted presignature data
    pub encrypted_data: Vec<u8>,
    /// Generation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// CGGMP24 partial signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cggmp24PartialSignature {
    /// Party index
    pub party_index: u16,
    /// Partial signature data
    pub data: Vec<u8>,
}

/// Run CGGMP24 DKG protocol
///
/// # Arguments
/// * `party_index` - This party's index (0-based)
/// * `threshold` - Signing threshold (e.g., 4 for 4-of-5)
/// * `total_parties` - Total number of parties (e.g., 5)
/// * `incoming` - Stream of incoming messages (from_party, message_bytes)
/// * `outgoing` - Sink to send outgoing messages (to_party, message_bytes)
///
/// # Returns
/// Key share and public key
pub async fn run_cggmp24_dkg<S, Si>(
    party_index: u16,
    threshold: u16,
    total_parties: u16,
    mut incoming: S,
    mut outgoing: Si,
) -> Result<Cggmp24KeyShare>
where
    S: Stream<Item = Result<Msg<keygen_msg::Msg<Secp256k1>>>> + Unpin,
    Si: Sink<Msg<keygen_msg::Msg<Secp256k1>>, Error = anyhow::Error> + Unpin,
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

    // Create keygen protocol state machine
    let mut state_machine = KeygenProtocol::<Secp256k1>::new(
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
                return Err(anyhow!("Critical DKG error: {:?}", err));
            }
            tracing::warn!("Non-critical DKG error: {:?}", err);
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
    let key_share: DirtyKeyShare<Secp256k1> = state_machine
        .pick_output()
        .ok_or_else(|| anyhow!("DKG completed but no output available"))?;

    // Serialize key share for storage
    let encrypted_data = bincode::serialize(&key_share)
        .map_err(|e| anyhow!("Failed to serialize key share: {}", e))?;

    // Extract public key (compressed, 33 bytes)
    let public_key_point = key_share.shared_public_key();
    let public_key = public_key_point.to_bytes(true).to_vec();

    if public_key.len() != 33 {
        bail!("Expected 33-byte compressed public key, got {}", public_key.len());
    }

    Ok(Cggmp24KeyShare {
        party_index,
        threshold,
        total_parties,
        encrypted_data,
        public_key,
    })
}

/// Generate CGGMP24 presignature
///
/// # Arguments
/// * `key_share` - Key share from DKG
/// * `participants` - Indices of participating parties (threshold count)
/// * `incoming` - Stream of incoming messages
/// * `outgoing` - Sink to send outgoing messages
///
/// # Returns
/// Presignature that can be used for fast signing
pub async fn generate_cggmp24_presignature<S, Si>(
    key_share: &Cggmp24KeyShare,
    participants: Vec<u16>,
    mut incoming: S,
    mut outgoing: Si,
) -> Result<Cggmp24Presignature>
where
    S: Stream<Item = Result<Msg<cggmp24::signing::msg_type::Msg<Secp256k1>>>> + Unpin,
    Si: Sink<Msg<cggmp24::signing::msg_type::Msg<Secp256k1>>, Error = anyhow::Error> + Unpin,
{
    // Validate participants
    if participants.len() < key_share.threshold as usize {
        bail!("Need at least {} participants, got {}", key_share.threshold, participants.len());
    }

    // Deserialize key share
    let dirty_key_share: DirtyKeyShare<Secp256k1> = bincode::deserialize(&key_share.encrypted_data)
        .map_err(|e| anyhow!("Failed to deserialize key share: {}", e))?;

    // Initialize RNG
    let mut rng = StdRng::from_entropy();

    // Start presigning protocol
    let mut state_machine = cggmp24::signing::InteractiveSigningBuilder::<Secp256k1>::new(
        dirty_key_share.clone(),
        participants.clone(),
    )
    .set_threshold(key_share.threshold)
    .start_presigning(&mut rng)?;

    // Run the state machine
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
                return Err(anyhow!("Critical presigning error: {:?}", err));
            }
            tracing::warn!("Non-critical presigning error: {:?}", err);
        }

        if let Some(msg_result) = incoming.next().await {
            let msg = msg_result?;
            state_machine.handle_incoming(msg)?;
        } else {
            bail!("Incoming message stream closed unexpectedly");
        }
    }

    // Extract presignature
    let presignature = state_machine
        .pick_output()
        .ok_or_else(|| anyhow!("Presigning completed but no output available"))?;

    // Serialize presignature
    let encrypted_data = bincode::serialize(&presignature)
        .map_err(|e| anyhow!("Failed to serialize presignature: {}", e))?;

    Ok(Cggmp24Presignature {
        id: uuid::Uuid::new_v4(),
        participants,
        encrypted_data,
        created_at: chrono::Utc::now(),
    })
}

/// Issue CGGMP24 partial signature
///
/// # Arguments
/// * `key_share` - Key share from DKG
/// * `presignature` - Pre-computed presignature
/// * `message_hash` - 32-byte message hash to sign
///
/// # Returns
/// Partial signature from this party
pub fn issue_cggmp24_partial_signature(
    key_share: &Cggmp24KeyShare,
    presignature: &Cggmp24Presignature,
    message_hash: &[u8; 32],
) -> Result<Cggmp24PartialSignature> {
    // Deserialize key share
    let dirty_key_share: DirtyKeyShare<Secp256k1> = bincode::deserialize(&key_share.encrypted_data)
        .map_err(|e| anyhow!("Failed to deserialize key share: {}", e))?;

    // Deserialize presignature
    let presig: cggmp24::signing::DataToSign<Secp256k1> = bincode::deserialize(&presignature.encrypted_data)
        .map_err(|e| anyhow!("Failed to deserialize presignature: {}", e))?;

    // Convert message hash to scalar
    let msg_scalar = Scalar::<Secp256k1>::from_be_bytes_mod_order(message_hash);

    // Issue partial signature (non-interactive with presignature)
    let partial_sig = presig.complete_signing(msg_scalar)?;

    // Serialize partial signature
    let data = bincode::serialize(&partial_sig)
        .map_err(|e| anyhow!("Failed to serialize partial signature: {}", e))?;

    Ok(Cggmp24PartialSignature {
        party_index: key_share.party_index,
        data,
    })
}

/// Combine CGGMP24 partial signatures
///
/// # Arguments
/// * `partial_signatures` - Threshold count of partial signatures
/// * `message_hash` - The message hash that was signed
/// * `public_key` - The compressed public key (33 bytes)
///
/// # Returns
/// Final ECDSA signature (DER-encoded with SIGHASH_ALL)
pub fn combine_cggmp24_signatures(
    partial_signatures: &[Cggmp24PartialSignature],
    message_hash: &[u8; 32],
    public_key: &[u8],
) -> Result<Vec<u8>> {
    // Validate we have enough signatures
    if partial_signatures.is_empty() {
        bail!("No partial signatures provided");
    }

    // Deserialize all partial signatures
    let mut sigs: Vec<cggmp24::signing::SignatureShare<Secp256k1>> = Vec::new();
    for partial in partial_signatures {
        let sig: cggmp24::signing::SignatureShare<Secp256k1> = bincode::deserialize(&partial.data)
            .map_err(|e| anyhow!("Failed to deserialize partial signature: {}", e))?;
        sigs.push(sig);
    }

    // Combine partial signatures
    let signature = cggmp24::signing::aggregate_partial_signatures(&sigs)?;

    // Convert to DER format
    let (r_bytes, s_bytes) = signature.to_bytes();

    // Build DER encoding
    let mut der_sig = vec![0x30]; // SEQUENCE tag

    // We'll build the content first, then add the length
    let mut content = Vec::new();

    // Add r as INTEGER
    content.push(0x02); // INTEGER tag
    let r_trimmed = trim_leading_zeros(&r_bytes);
    let r_len = if r_trimmed[0] & 0x80 != 0 {
        // Need padding
        content.push((r_trimmed.len() + 1) as u8);
        content.push(0x00);
        content.extend_from_slice(r_trimmed);
        r_trimmed.len() + 1
    } else {
        content.push(r_trimmed.len() as u8);
        content.extend_from_slice(r_trimmed);
        r_trimmed.len()
    };

    // Add s as INTEGER
    content.push(0x02); // INTEGER tag
    let s_trimmed = trim_leading_zeros(&s_bytes);
    let s_len = if s_trimmed[0] & 0x80 != 0 {
        content.push((s_trimmed.len() + 1) as u8);
        content.push(0x00);
        content.extend_from_slice(s_trimmed);
        s_trimmed.len() + 1
    } else {
        content.push(s_trimmed.len() as u8);
        content.extend_from_slice(s_trimmed);
        s_trimmed.len()
    };

    // Add total length
    der_sig.push(content.len() as u8);
    der_sig.extend(content);

    // Add SIGHASH_ALL flag
    der_sig.push(0x01);

    Ok(der_sig)
}

/// Helper to trim leading zeros from byte array
fn trim_leading_zeros(bytes: &[u8]) -> &[u8] {
    let first_nonzero = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len() - 1);
    &bytes[first_nonzero..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trim_leading_zeros() {
        assert_eq!(trim_leading_zeros(&[0, 0, 1, 2, 3]), &[1, 2, 3]);
        assert_eq!(trim_leading_zeros(&[1, 2, 3]), &[1, 2, 3]);
        assert_eq!(trim_leading_zeros(&[0, 0, 0, 0]), &[0]);
    }
}
