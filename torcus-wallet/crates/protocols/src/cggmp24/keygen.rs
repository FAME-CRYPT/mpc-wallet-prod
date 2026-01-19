//! CGGMP24 Key Generation Protocol Runner
//!
//! This module runs the CGGMP24 distributed key generation protocol.

use async_channel::{Receiver, Sender};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::cggmp24::runner::{ChannelDelivery, ProtocolMessage};

/// Result of key generation
#[derive(Debug)]
pub struct KeygenResult {
    pub success: bool,
    pub key_share_data: Option<Vec<u8>>,
    pub public_key: Option<Vec<u8>>,
    pub error: Option<String>,
    pub duration_secs: f64,
}

/// Stored key share data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredKeyShare {
    pub version: u32,
    pub wallet_id: String,
    pub party_index: u16,
    pub threshold: u16,
    pub num_parties: u16,
    /// Serialized IncompleteKeyShare
    pub incomplete_key_share: Vec<u8>,
    /// Serialized AuxInfo
    pub aux_info: Vec<u8>,
    /// Public key bytes (compressed)
    pub public_key: Vec<u8>,
    pub created_at: u64,
}

impl StoredKeyShare {
    pub const CURRENT_VERSION: u32 = 1;
}

/// Run the CGGMP24 key generation protocol
pub async fn run_keygen(
    party_index: u16,
    num_parties: u16,
    threshold: u16,
    session_id: &str,
    incoming_rx: Receiver<ProtocolMessage>,
    outgoing_tx: Sender<ProtocolMessage>,
) -> KeygenResult {
    let start = std::time::Instant::now();

    info!("========================================");
    info!("  CGGMP24 KEY GENERATION STARTING");
    info!("========================================");
    info!("Party index: {}", party_index);
    info!("Num parties: {}", num_parties);
    info!("Threshold: {}", threshold);
    info!("Session ID: {}", session_id);

    // Create execution ID
    let eid = cggmp24::ExecutionId::new(session_id.as_bytes());

    // Create delivery channels
    let delivery = ChannelDelivery::new(
        incoming_rx,
        outgoing_tx,
        session_id.to_string(),
        party_index,
    );
    let (incoming, outgoing) = delivery.split();
    let incoming_boxed = Box::pin(incoming);
    let outgoing_boxed = Box::pin(outgoing);

    // Create MPC party
    let party = round_based::MpcParty::connected((incoming_boxed, outgoing_boxed));

    // Run the keygen protocol
    info!("Starting keygen protocol...");
    let keygen_result =
        cggmp24::keygen::<cggmp24::supported_curves::Secp256k1>(eid, party_index, num_parties)
            .set_threshold(threshold)
            .start(&mut OsRng, party)
            .await;

    let elapsed = start.elapsed();

    match keygen_result {
        Ok(incomplete_key_share) => {
            info!(
                "Key generation completed successfully in {:.2}s",
                elapsed.as_secs_f64()
            );

            // Get public key
            let public_key_point = incomplete_key_share.shared_public_key();
            let public_key_bytes = public_key_point.to_bytes(true).to_vec();
            info!("Public key: {}", hex::encode(&public_key_bytes));

            // Serialize the incomplete key share using JSON (supports deserialize_any)
            match serde_json::to_vec(&incomplete_key_share) {
                Ok(key_data) => {
                    info!("Key share serialized: {} bytes", key_data.len());
                    KeygenResult {
                        success: true,
                        key_share_data: Some(key_data),
                        public_key: Some(public_key_bytes),
                        error: None,
                        duration_secs: elapsed.as_secs_f64(),
                    }
                }
                Err(e) => {
                    error!("Failed to serialize key share: {}", e);
                    KeygenResult {
                        success: false,
                        key_share_data: None,
                        public_key: None,
                        error: Some(format!("Failed to serialize key share: {}", e)),
                        duration_secs: elapsed.as_secs_f64(),
                    }
                }
            }
        }
        Err(e) => {
            error!("Key generation failed: {:?}", e);
            KeygenResult {
                success: false,
                key_share_data: None,
                public_key: None,
                error: Some(format!("Protocol error: {:?}", e)),
                duration_secs: elapsed.as_secs_f64(),
            }
        }
    }
}

/// Save key share to disk
pub fn save_key_share(path: &std::path::Path, key_share: &StoredKeyShare) -> Result<(), String> {
    let json = serde_json::to_string_pretty(key_share)
        .map_err(|e| format!("Failed to serialize key share: {}", e))?;
    std::fs::write(path, json).map_err(|e| format!("Failed to write key share file: {}", e))?;
    info!("Key share saved to {:?}", path);
    Ok(())
}

/// Load key share from disk
pub fn load_key_share(path: &std::path::Path) -> Result<StoredKeyShare, String> {
    let json = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read key share file: {}", e))?;
    let key_share: StoredKeyShare =
        serde_json::from_str(&json).map_err(|e| format!("Failed to parse key share: {}", e))?;
    Ok(key_share)
}

/// Default path for key share storage
pub fn default_key_share_path(party_index: u16, wallet_id: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(format!(
        "/data/keyshare-party-{}-{}.json",
        party_index, wallet_id
    ))
}
