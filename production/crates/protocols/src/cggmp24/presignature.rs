//! Presignature generation and management for fast threshold signing.
//!
//! Presignatures allow splitting threshold signing into two phases:
//! 1. Offline: Generate presignatures in background (slow, ~2 seconds)
//! 2. Online: Use presignature to sign message (fast, <500ms)
//!
//! This enables much faster signing by pre-computing the expensive parts
//! before the message is known.

use anyhow::Result;
use async_channel::{Receiver, Sender};
use cggmp24::key_share::Validate;
use cggmp24::signing::{PartialSignature, Presignature, PresignaturePublicData};
use rand::rngs::OsRng;
use std::sync::{Arc, Mutex};
use tracing::{error, info};

use crate::bench::BenchmarkRecorder;
use crate::cggmp24::runner::{ChannelDelivery, ProtocolMessage};

/// A presignature along with its public data and metadata.
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
    /// Create a new stored presignature.
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

    /// Check if the presignature has expired (older than 1 hour).
    pub fn is_expired(&self) -> bool {
        if let Ok(age) = self.generated_at.elapsed() {
            age.as_secs() > 3600 // 1 hour
        } else {
            true
        }
    }
}

/// Result of presignature generation.
pub struct PresignatureResult<E: generic_ec::Curve, L: cggmp24::security_level::SecurityLevel> {
    pub success: bool,
    pub presignature: Option<StoredPresignature<E, L>>,
    pub error: Option<String>,
    pub duration_secs: f64,
}

/// Generate a presignature by coordinating with other nodes.
///
/// This is the slow part of threshold signing (~2 seconds).
/// Presignatures can be generated in advance, before knowing the message.
///
/// # Arguments
/// * `party_index` - This party's index (keygen index)
/// * `parties` - List of all parties participating in presignature generation
/// * `session_id` - Unique session ID for this presignature generation
/// * `key_share_data` - Serialized key share
/// * `aux_info_data` - Serialized auxiliary info
/// * `incoming_rx` - Channel for receiving protocol messages
/// * `outgoing_tx` - Channel for sending protocol messages
#[allow(clippy::too_many_arguments)]
pub async fn generate_presignature(
    party_index: u16,
    parties: &[u16],
    session_id: &str,
    key_share_data: &[u8],
    aux_info_data: &[u8],
    incoming_rx: Receiver<ProtocolMessage>,
    outgoing_tx: Sender<ProtocolMessage>,
) -> PresignatureResult<cggmp24::supported_curves::Secp256k1, cggmp24::security_level::SecurityLevel128>
{
    let start = std::time::Instant::now();

    // Initialize benchmark recorder
    let recorder = Arc::new(Mutex::new(BenchmarkRecorder::new(
        "CGGMP24-Presignature",
        party_index,
        session_id,
    )));

    // Convert party_index to signer_index
    let signer_index = match parties.iter().position(|&p| p == party_index) {
        Some(idx) => idx as u16,
        None => {
            error!(
                "Party {} is not in the presignature parties list {:?}",
                party_index, parties
            );
            return PresignatureResult {
                success: false,
                presignature: None,
                error: Some(format!(
                    "Party {} is not a participant in parties {:?}",
                    party_index, parties
                )),
                duration_secs: start.elapsed().as_secs_f64(),
            };
        }
    };

    info!("========================================");
    info!("  PRESIGNATURE GENERATION STARTING");
    info!("========================================");
    info!(
        "Party index: {} (signer index: {})",
        party_index, signer_index
    );
    info!("Parties: {:?}", parties);
    info!("Session ID: {}", session_id);

    // Step 1: Deserialize aux_info
    let step_start = std::time::Instant::now();
    let dirty_aux_info: cggmp24::key_share::DirtyAuxInfo<
        cggmp24::security_level::SecurityLevel128,
    > = match serde_json::from_slice(aux_info_data) {
        Ok(a) => a,
        Err(e) => {
            error!("Failed to deserialize aux_info: {}", e);
            return PresignatureResult {
                success: false,
                presignature: None,
                error: Some(format!("Failed to deserialize aux_info: {}", e)),
                duration_secs: start.elapsed().as_secs_f64(),
            };
        }
    };
    if let Ok(mut rec) = recorder.lock() {
        rec.record_step("1. Deserialize aux_info", step_start.elapsed());
    }

    // Step 2: Validate aux_info
    let step_start = std::time::Instant::now();
    let aux_info = match dirty_aux_info.validate() {
        Ok(a) => a,
        Err(e) => {
            error!("Failed to validate aux_info: {:?}", e);
            return PresignatureResult {
                success: false,
                presignature: None,
                error: Some(format!("Invalid aux_info: {:?}", e)),
                duration_secs: start.elapsed().as_secs_f64(),
            };
        }
    };
    if let Ok(mut rec) = recorder.lock() {
        rec.record_step("2. Validate aux_info", step_start.elapsed());
    }

    // Step 3: Deserialize key share
    let step_start = std::time::Instant::now();
    let dirty_key_share: cggmp24::key_share::DirtyIncompleteKeyShare<
        cggmp24::supported_curves::Secp256k1,
    > = match serde_json::from_slice(key_share_data) {
        Ok(k) => k,
        Err(e) => {
            error!("Failed to deserialize key share: {}", e);
            return PresignatureResult {
                success: false,
                presignature: None,
                error: Some(format!("Failed to deserialize key share: {}", e)),
                duration_secs: start.elapsed().as_secs_f64(),
            };
        }
    };
    if let Ok(mut rec) = recorder.lock() {
        rec.record_step("3. Deserialize key_share", step_start.elapsed());
    }

    // Step 4: Validate key share
    let step_start = std::time::Instant::now();
    let incomplete_key_share = match dirty_key_share.validate() {
        Ok(k) => k,
        Err(e) => {
            error!("Failed to validate key share: {:?}", e);
            return PresignatureResult {
                success: false,
                presignature: None,
                error: Some(format!("Invalid key share: {:?}", e)),
                duration_secs: start.elapsed().as_secs_f64(),
            };
        }
    };
    if let Ok(mut rec) = recorder.lock() {
        rec.record_step("4. Validate key_share", step_start.elapsed());
    }

    // Step 5: Combine into full key share
    let step_start = std::time::Instant::now();
    let key_share = match cggmp24::KeyShare::from_parts((incomplete_key_share, aux_info)) {
        Ok(ks) => ks,
        Err(e) => {
            error!("Failed to construct key share: {:?}", e);
            return PresignatureResult {
                success: false,
                presignature: None,
                error: Some(format!("Failed to construct key share: {:?}", e)),
                duration_secs: start.elapsed().as_secs_f64(),
            };
        }
    };
    if let Ok(mut rec) = recorder.lock() {
        rec.record_step("5. Construct full key_share", step_start.elapsed());
    }

    // Step 6: Create execution ID and prepare protocol
    let step_start = std::time::Instant::now();
    let eid = cggmp24::ExecutionId::new(session_id.as_bytes());

    let delivery = ChannelDelivery::new_with_mapping(
        incoming_rx,
        outgoing_tx,
        session_id.to_string(),
        party_index,
        parties,
    );
    let (incoming, outgoing) = delivery.split();
    let incoming_boxed = Box::pin(incoming);
    let outgoing_boxed = Box::pin(outgoing);

    let party = round_based::MpcParty::connected((incoming_boxed, outgoing_boxed));
    if let Ok(mut rec) = recorder.lock() {
        rec.record_step("6. Protocol setup (channels, party)", step_start.elapsed());
    }

    // Step 7: Run presignature generation protocol
    info!("Running presignature generation protocol...");
    let step_start = std::time::Instant::now();
    let presig_result = cggmp24::signing(eid, signer_index, parties, &key_share)
        .generate_presignature(&mut OsRng, party)
        .await;
    if let Ok(mut rec) = recorder.lock() {
        rec.record_step("7. MPC presignature protocol", step_start.elapsed());
    }

    let elapsed = start.elapsed();

    match presig_result {
        Ok((presignature, public_data)) => {
            info!(
                "Presignature generated successfully in {:.2}s",
                elapsed.as_secs_f64()
            );

            let stored = StoredPresignature::new(presignature, public_data, parties.to_vec());

            // Complete benchmark
            if let Ok(mut rec) = recorder.lock() {
                rec.complete();
                let report = rec.report();
                report.log();
            }

            PresignatureResult {
                success: true,
                presignature: Some(stored),
                error: None,
                duration_secs: elapsed.as_secs_f64(),
            }
        }
        Err(e) => {
            error!("Presignature generation failed: {:?}", e);

            // Complete benchmark even on failure
            if let Ok(mut rec) = recorder.lock() {
                rec.complete();
                let report = rec.report();
                report.log();
            }

            PresignatureResult {
                success: false,
                presignature: None,
                error: Some(format!("Protocol error: {:?}", e)),
                duration_secs: elapsed.as_secs_f64(),
            }
        }
    }
}

/// Issue a partial signature using a presignature.
///
/// This is fast and non-interactive (local computation only).
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

/// Combine partial signatures into a full signature.
///
/// Requires threshold number of partial signatures.
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
