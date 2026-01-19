// Distributed key generation for 3-of-4 threshold ECDSA
// Uses cggmp24-keygen to generate threshold key shares

use crate::http_transport;
use crate::message_board_client::MessageBoardClient;
use anyhow::Result;
use cggmp24::supported_curves::Secp256k1;
use log::info;
use rand::rngs::OsRng;
use round_based::MpcParty;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

/// Configuration for key generation
pub struct KeygenConfig {
    /// This node's index (0-based)
    pub my_index: u16,
    /// Total number of nodes
    pub n: u16,
    /// Threshold (how many nodes needed to sign)
    pub t: u16,
    /// Execution ID for this keygen session
    pub execution_id: [u8; 32],
}

/// Incomplete key share from keygen (needs AuxInfo to complete)
/// This is what keygen produces - must be completed with aux_info_gen
#[derive(Serialize, Deserialize)]
pub struct IncompleteKeyShare {
    /// The incomplete key share from cggmp24 keygen
    pub core: cggmp24::IncompleteKeyShare<Secp256k1>,
}

/// Run distributed key generation
///
/// All nodes must run this simultaneously with the same execution_id
/// Returns an incomplete key share that needs to be completed with AuxInfo
pub async fn run_keygen(
    config: KeygenConfig,
    client: MessageBoardClient,
    keygen_request_id: String,
) -> Result<IncompleteKeyShare> {
    info!(
        "Starting keygen: node {}/{}, threshold {}/{}",
        config.my_index + 1,
        config.n,
        config.t,
        config.n
    );

    // Create execution ID
    let eid = cggmp24::ExecutionId::new(&config.execution_id);

    // Create HTTP transport for this keygen session
    let transport = http_transport::http_transport(client, keygen_request_id, config.my_index);

    // Create MPC party (transport is a tuple of (Stream, Sink))
    let party = MpcParty::connected(transport);

    // Initialize RNG
    let mut rng = OsRng;

    info!("Running threshold keygen protocol...");

    // Run threshold key generation
    // This is the actual distributed protocol
    let key_share = cggmp24::keygen::<Secp256k1>(eid, config.my_index, config.n)
        .set_threshold(config.t)
        .start(&mut rng, party)
        .await
        .map_err(|e| anyhow::anyhow!("Keygen failed: {:?}", e))?;

    info!("Keygen completed successfully!");
    info!("Public key: {:?}", key_share.shared_public_key);

    Ok(IncompleteKeyShare { core: key_share })
}

/// Save incomplete key share to disk
pub async fn save_key_share(share: &IncompleteKeyShare, path: &Path) -> Result<()> {
    info!("Saving key share to {:?}", path);

    let json = serde_json::to_string_pretty(share)?;
    fs::write(path, json).await?;

    info!("Key share saved successfully");
    Ok(())
}

/// Load incomplete key share from disk
pub async fn load_key_share(path: &Path) -> Result<IncompleteKeyShare> {
    info!("Loading key share from {:?}", path);

    if !path.exists() {
        anyhow::bail!("Key share file not found: {:?}", path);
    }

    let json = fs::read_to_string(path).await?;
    let share: IncompleteKeyShare = serde_json::from_str(&json)?;

    info!("Key share loaded successfully");
    Ok(share)
}

/// Check if a key share exists
pub async fn key_share_exists(path: &Path) -> bool {
    path.exists()
}

/// Extract and save the shared public key from an incomplete key share
/// This public key can be used by anyone to verify signatures
pub async fn save_public_key(share: &IncompleteKeyShare, path: &Path) -> Result<()> {
    info!("Saving shared public key to {:?}", path);

    // Serialize the public key as JSON
    let json = serde_json::to_string_pretty(&share.core.shared_public_key)?;
    fs::write(path, json).await?;

    info!("Shared public key saved successfully");
    Ok(())
}
