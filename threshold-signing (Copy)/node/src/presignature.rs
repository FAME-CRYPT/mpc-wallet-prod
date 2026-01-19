// Presignature generation and management for fast threshold signing
//
// Presignatures allow splitting threshold signing into two phases:
// 1. Offline: Generate presignatures in background (slow, ~4 seconds)
// 2. Online: Use presignature to sign message (fast, non-interactive)

use crate::http_transport;
use crate::message_board_client::MessageBoardClient;
use anyhow::Result;
use cggmp24::signing::{PartialSignature, Presignature, PresignaturePublicData};
use cggmp24::KeyShare;
use log::info;
use rand::rngs::OsRng;
use round_based::MpcParty;
use serde::{Deserialize, Serialize};

/// A presignature along with its public data and metadata
#[derive(Clone)]
pub struct StoredPresignature<E: generic_ec::Curve, L: cggmp24::security_level::SecurityLevel> {
    /// The presignature (contains secret data)
    pub presignature: Presignature<E>,
    /// Public data for verification
    pub public_data: PresignaturePublicData<E>,
    /// The participants who generated this presignature
    pub participants: Vec<u16>,
    /// When this presignature was generated
    pub generated_at: std::time::SystemTime,
    /// Phantom data for security level
    _phantom: std::marker::PhantomData<L>,
}

impl<E: generic_ec::Curve, L: cggmp24::security_level::SecurityLevel> StoredPresignature<E, L> {
    pub fn new(
        presignature: Presignature<E>,
        public_data: PresignaturePublicData<E>,
        participants: Vec<u16>,
    ) -> Self {
        Self {
            presignature,
            public_data,
            participants,
            generated_at: std::time::SystemTime::now(),
            _phantom: std::marker::PhantomData,
        }
    }
}

/// Generate a presignature by coordinating with other nodes
///
/// This is the slow part of threshold signing (~4 seconds)
/// Presignatures can be generated in advance, before knowing the message
pub async fn generate_presignature<E, L>(
    request_id: String,
    my_index: u16,
    participants: &[u16],
    key_share: &KeyShare<E, L>,
    client: MessageBoardClient,
) -> Result<StoredPresignature<E, L>>
where
    E: generic_ec::Curve,
    generic_ec::Point<E>: generic_ec::coords::HasAffineX<E>,
    L: cggmp24::security_level::SecurityLevel,
{
    info!(
        "Generating presignature for request {}, participants: {:?}",
        request_id, participants
    );

    // Check we're a participant
    if !participants.contains(&my_index) {
        anyhow::bail!("Not a participant in this presignature generation");
    }

    // Create execution ID from request ID
    let mut eid_bytes = [0u8; 32];
    let request_bytes = request_id.as_bytes();
    let len = std::cmp::min(request_bytes.len(), 32);
    eid_bytes[..len].copy_from_slice(&request_bytes[..len]);
    let eid = cggmp24::ExecutionId::new(&eid_bytes);

    // Create HTTP transport for this presignature generation session
    // Use presignature-specific endpoints to avoid conflicts with regular signing
    let transport =
        http_transport::http_transport_presignature(client, request_id.clone(), my_index);

    // Create MPC party
    let party = MpcParty::connected(transport);

    // Initialize RNG
    let mut rng = OsRng;

    info!("Running presignature generation protocol...");

    // Run presignature generation protocol
    let (presignature, public_data) = cggmp24::signing(eid, my_index, participants, key_share)
        .generate_presignature(&mut rng, party)
        .await
        .map_err(|e| anyhow::anyhow!("Presignature generation failed: {:?}", e))?;

    info!("Presignature generated successfully");

    Ok(StoredPresignature::new(
        presignature,
        public_data,
        participants.to_vec(),
    ))
}

/// Issue a partial signature using a presignature
///
/// This is fast and non-interactive (local computation only)
pub fn issue_partial_signature<E>(
    presignature: Presignature<E>,
    message: cggmp24::signing::DataToSign<E>,
) -> PartialSignature<E>
where
    E: generic_ec::Curve,
    generic_ec::Point<E>: generic_ec::coords::HasAffineX<E>,
{
    presignature.issue_partial_signature(message)
}

/// Combine partial signatures into a full signature
///
/// Requires threshold number of partial signatures
pub fn combine_partial_signatures<E>(
    partial_signatures: &[PartialSignature<E>],
    public_data: &PresignaturePublicData<E>,
    message: cggmp24::signing::DataToSign<E>,
) -> Result<cggmp24::Signature<E>>
where
    E: generic_ec::Curve,
    generic_ec::Point<E>: generic_ec::coords::HasAffineX<E>,
{
    PartialSignature::combine(partial_signatures, public_data, message)
        .ok_or_else(|| anyhow::anyhow!("Failed to combine partial signatures"))
}
