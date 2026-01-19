// Threshold signing using CGGMP24
// Nodes coordinate via MessageBoard to sign messages

use crate::http_transport;
use crate::message_board_client::MessageBoardClient;
use anyhow::Result;
use cggmp24::signing::DataToSign;

use cggmp24::KeyShare;
use log::info;
use rand::rngs::OsRng;
use round_based::MpcParty;
use sha2::Sha256;

/// Run threshold signing for a message
///
/// Coordinates with other nodes via MessageBoard to produce a signature
/// Returns the signature as a hex-encoded string
pub async fn sign_message<E, L>(
    request_id: String,
    message: &str,
    my_index: u16,
    participants: &[u16],
    key_share: &KeyShare<E, L>,
    client: MessageBoardClient,
) -> Result<String>
where
    E: generic_ec::Curve,
    generic_ec::Point<E>: generic_ec::coords::HasAffineX<E>,
    L: cggmp24::security_level::SecurityLevel,
{
    info!(
        "Starting signing for request {}, participants: {:?}",
        request_id, participants
    );

    // Check we're a participant
    if !participants.contains(&my_index) {
        anyhow::bail!("Not a participant in this signing session");
    }

    // Create execution ID from request ID
    let mut eid_bytes = [0u8; 32];
    let request_bytes = request_id.as_bytes();
    let len = std::cmp::min(request_bytes.len(), 32);
    eid_bytes[..len].copy_from_slice(&request_bytes[..len]);
    let eid = cggmp24::ExecutionId::new(&eid_bytes);

    // Prepare message to sign (hash it)
    let message_bytes = message.as_bytes();
    let data_to_sign = DataToSign::digest::<Sha256>(message_bytes);

    // Create HTTP transport for this signing session
    let transport = http_transport::http_transport(client, request_id.clone(), my_index);

    // Create MPC party (transport is a tuple of (Stream, Sink))
    let party = MpcParty::connected(transport);

    // Initialize RNG
    let mut rng = OsRng;

    info!("Running threshold signing protocol...");

    // Run signing protocol
    let signature = cggmp24::signing(eid, my_index, participants, key_share)
        .sign(&mut rng, party, &data_to_sign)
        .await
        .map_err(|e| anyhow::anyhow!("Signing failed: {:?}", e))?;

    // Verify the signature immediately as a sanity check
    signature
        .verify(&key_share.shared_public_key, &data_to_sign)
        .map_err(|e| anyhow::anyhow!("Signature verification failed (sanity check): {:?}", e))?;

    info!("Signature verified successfully (sanity check passed)");

    // Encode signature as JSON
    let sig_json = serde_json::to_string(&signature)
        .map_err(|e| anyhow::anyhow!("Failed to serialize signature: {}", e))?;

    info!(
        "Signing completed! Signature length: {} bytes",
        sig_json.len()
    );

    Ok(sig_json)
}

/// Select which nodes should participate in signing
/// For now, just use the first `t` nodes
/// In production, this could be more sophisticated
pub fn select_participants(t: u16, _n: u16) -> Vec<u16> {
    (0..t).collect()
}
