//! Fast threshold signing using presignatures.
//!
//! Uses pre-generated presignatures for sub-second signing.
//! Falls back to full signing protocol if no presignature is available.

use anyhow::{anyhow, Result};
use async_channel::{Receiver, Sender};
use cggmp24::signing::PartialSignature;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};

use crate::bench::BenchmarkRecorder;
use crate::cggmp24::runner::ProtocolMessage;
use crate::cggmp24::{presignature, presig_pool, SignatureData, SigningResult, StoredPresignature};
use crate::relay::RelayClient;

/// Sign a message using a presignature from the pool (fast path).
///
/// If no presignature is available, falls back to full signing protocol (slow path).
///
/// # Arguments
/// * `party_index` - This party's index
/// * `parties` - List of parties participating in signing
/// * `session_id` - Unique session ID for this signing operation
/// * `message_hash` - 32-byte hash of the message to sign
/// * `pool` - Presignature pool to get a presignature from
/// * `relay_client` - Client for exchanging partial signatures
/// * `key_share_data` - Serialized key share (for fallback to full signing)
/// * `aux_info_data` - Serialized auxiliary info (for fallback to full signing)
/// * `incoming_rx` - Channel for receiving protocol messages (for fallback)
/// * `outgoing_tx` - Channel for sending protocol messages (for fallback)
#[allow(clippy::too_many_arguments)]
pub async fn sign_message_fast(
    party_index: u16,
    parties: &[u16],
    session_id: &str,
    message_hash: &[u8; 32],
    pool: presig_pool::PresignaturePoolHandle<
        cggmp24::supported_curves::Secp256k1,
        cggmp24::security_level::SecurityLevel128,
    >,
    relay_client: &RelayClient,
    key_share_data: &[u8],
    aux_info_data: &[u8],
    incoming_rx: Receiver<ProtocolMessage>,
    outgoing_tx: Sender<ProtocolMessage>,
) -> SigningResult {
    // Check pool status
    let stats = pool.stats().await;
    info!(
        "Presignature pool status: {}/{} ({}% full)",
        stats.current_size, stats.target_size, stats.utilization
    );

    // Try to get a presignature from the pool
    if let Some(stored_presig) = pool.take().await {
        info!("Using FAST signing with presignature! (reduced network rounds)");

        // Use the presignature-based signing which is much faster
        match sign_with_presignature(
            party_index,
            parties,
            session_id,
            message_hash,
            relay_client,
            stored_presig,
        )
        .await
        {
            Ok(result) => {
                if result.success {
                    info!("Fast signing completed successfully!");
                    return result;
                } else {
                    warn!(
                        "Fast signing failed: {:?}, falling back to full protocol",
                        result.error
                    );
                }
            }
            Err(e) => {
                warn!("Fast signing error: {}, falling back to full protocol", e);
            }
        }
    } else {
        info!("No presignature available, using full signing protocol");
    }

    // Fallback: use full signing protocol
    crate::cggmp24::run_signing(
        party_index,
        parties,
        session_id,
        message_hash,
        key_share_data,
        aux_info_data,
        incoming_rx,
        outgoing_tx,
    )
    .await
}

/// Message for exchanging partial signatures.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PartialSignatureMessage {
    session_id: String,
    sender: u16,
    partial_signature: Vec<u8>, // Serialized PartialSignature
}

/// Sign using a pre-generated presignature (fast, requires partial signature exchange).
async fn sign_with_presignature(
    party_index: u16,
    parties: &[u16],
    session_id: &str,
    message_hash: &[u8; 32],
    relay_client: &RelayClient,
    stored_presig: StoredPresignature<
        cggmp24::supported_curves::Secp256k1,
        cggmp24::security_level::SecurityLevel128,
    >,
) -> Result<SigningResult> {
    let start = std::time::Instant::now();

    // Initialize benchmark recorder
    let recorder = Arc::new(Mutex::new(BenchmarkRecorder::new(
        "CGGMP24-FastSigning",
        party_index,
        session_id,
    )));

    info!("Computing partial signature locally (instant)...");

    // Step 1: Compute the message as DataToSign
    let step_start = std::time::Instant::now();
    let data_to_sign = cggmp24::signing::DataToSign::digest::<sha2::Sha256>(message_hash);
    if let Ok(mut rec) = recorder.lock() {
        rec.record_step("1. Prepare message", step_start.elapsed());
    }

    // Step 2: Locally issue partial signature using presignature (INSTANT - no network)
    let step_start = std::time::Instant::now();
    let my_partial_sig = presignature::issue_partial_signature(
        stored_presig.presignature.clone(),
        data_to_sign.clone(),
    );
    if let Ok(mut rec) = recorder.lock() {
        rec.record_step("2. Issue partial signature (local)", step_start.elapsed());
    }

    info!("Partial signature computed locally! Now exchanging with other nodes...");

    // Step 3: Serialize partial signature for transmission
    let step_start = std::time::Instant::now();
    let partial_sig_bytes = serde_json::to_vec(&my_partial_sig)
        .map_err(|e| anyhow::anyhow!("Failed to serialize partial signature: {}", e))?;
    if let Ok(mut rec) = recorder.lock() {
        rec.record_step("3. Serialize partial signature", step_start.elapsed());
    }

    // Step 4: Broadcast our partial signature via relay
    let step_start = std::time::Instant::now();
    info!("Broadcasting partial signature via relay...");
    let msg = PartialSignatureMessage {
        session_id: session_id.to_string(),
        sender: party_index,
        partial_signature: partial_sig_bytes.clone(),
    };

    // Use a dedicated message type for partial signatures
    let msg_type = format!("partial_sig_{}", session_id);
    let relay_msg = crate::relay::RelayMessage {
        session_id: msg_type.clone(),
        protocol: "fast_signing".to_string(),
        sender: party_index,
        recipient: None, // Broadcast to all
        round: 0,
        payload: serde_json::to_vec(&msg)?,
        seq: 0,
        timestamp: chrono::Utc::now().timestamp() as u64,
    };
    relay_client
        .submit_message(relay_msg)
        .await
        .map_err(|e| anyhow!("Failed to broadcast partial signature: {}", e))?;
    if let Ok(mut rec) = recorder.lock() {
        rec.record_step("4. Broadcast partial signature", step_start.elapsed());
    }

    // Step 5: Collect partial signatures from all participants (including ourselves)
    let step_start = std::time::Instant::now();
    info!(
        "Collecting partial signatures from {} participants...",
        parties.len()
    );
    let mut partial_sigs: HashMap<u16, PartialSignature<cggmp24::supported_curves::Secp256k1>> =
        HashMap::new();
    partial_sigs.insert(party_index, my_partial_sig);

    // Poll for other participants' partial signatures
    let max_wait = Duration::from_secs(10);
    let poll_start = std::time::Instant::now();

    while partial_sigs.len() < parties.len() && poll_start.elapsed() < max_wait {
        // Fetch messages from relay
        match relay_client.poll_messages(&msg_type).await {
            Ok(response) => {
                for relay_msg in &response.messages {
                    match serde_json::from_slice::<PartialSignatureMessage>(&relay_msg.payload) {
                        Ok(msg) => {
                            let sender_index = msg.sender;

                            // Skip if we already have this participant's signature
                            if partial_sigs.contains_key(&sender_index) {
                                continue;
                            }

                            // Skip if sender is not a participant
                            if !parties.contains(&sender_index) {
                                warn!("Received partial signature from non-participant: {}", sender_index);
                                continue;
                            }

                            // Parse the partial signature
                            match serde_json::from_slice::<PartialSignature<cggmp24::supported_curves::Secp256k1>>(&msg.partial_signature) {
                                Ok(partial_sig) => {
                                    info!("Received partial signature from party {}", sender_index);
                                    partial_sigs.insert(sender_index, partial_sig);
                                }
                                Err(e) => {
                                    warn!(
                                        "Failed to parse partial signature from party {}: {}",
                                        sender_index, e
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse PartialSignatureMessage: {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Failed to poll messages from relay: {}", e);
            }
        }

        if partial_sigs.len() < parties.len() {
            sleep(Duration::from_millis(100)).await;
        }
    }

    if partial_sigs.len() < parties.len() {
        error!(
            "Only received {}/{} partial signatures within timeout",
            partial_sigs.len(),
            parties.len()
        );
        return Ok(SigningResult {
            success: false,
            signature: None,
            error: Some(format!(
                "Only received {}/{} partial signatures within timeout",
                partial_sigs.len(),
                parties.len()
            )),
            duration_secs: start.elapsed().as_secs_f64(),
            benchmark: None,
        });
    }

    info!(
        "Collected all {} partial signatures! Combining...",
        partial_sigs.len()
    );
    if let Ok(mut rec) = recorder.lock() {
        rec.record_step("5. Collect partial signatures", step_start.elapsed());
    }

    // Step 6: Combine partial signatures into final signature
    let step_start = std::time::Instant::now();
    // Important: The partial signatures must be ordered by participant index
    let mut partial_sig_vec: Vec<PartialSignature<cggmp24::supported_curves::Secp256k1>> =
        Vec::new();
    for &participant_idx in parties {
        if let Some(partial_sig) = partial_sigs.get(&participant_idx) {
            partial_sig_vec.push(partial_sig.clone());
        } else {
            error!("Missing partial signature from participant {}", participant_idx);
            return Ok(SigningResult {
                success: false,
                signature: None,
                error: Some(format!(
                    "Missing partial signature from participant {}",
                    participant_idx
                )),
                duration_secs: start.elapsed().as_secs_f64(),
                benchmark: None,
            });
        }
    }

    info!(
        "Combining {} partial signatures in participant order: {:?}",
        partial_sig_vec.len(),
        parties
    );
    let signature = presignature::combine_partial_signatures(
        &partial_sig_vec,
        &stored_presig.public_data,
        data_to_sign,
    )
    .map_err(|e| anyhow::anyhow!("Combine failed: {}", e))?;
    if let Ok(mut rec) = recorder.lock() {
        rec.record_step("6. Combine partial signatures", step_start.elapsed());
    }

    info!("Fast signing: Signature combined successfully!");

    // Step 7: Extract signature components
    let step_start = std::time::Instant::now();
    let r_bytes = (*signature.r).to_be_bytes().to_vec();
    let mut s_bytes = (*signature.s).to_be_bytes().to_vec();

    info!("Signature r: {}", hex::encode(&r_bytes));
    info!("Signature s (raw): {}", hex::encode(&s_bytes));

    // Step 8: Normalize S for Bitcoin (low-S)
    let half_order: [u8; 32] = [
        0x7F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFF, 0x5D, 0x57, 0x6E, 0x73, 0x57, 0xA4, 0x50, 0x1D, 0xDF, 0xE9, 0x2F, 0x46, 0x68, 0x1B,
        0x20, 0xA0,
    ];

    let s_is_high = {
        let mut s_arr = [0u8; 32];
        s_arr.copy_from_slice(&s_bytes);
        s_arr > half_order
    };

    if s_is_high {
        info!("S is high, normalizing to low-S form for Bitcoin");
        let curve_order: [u8; 32] = [
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFE, 0xBA, 0xAE, 0xDC, 0xE6, 0xAF, 0x48, 0xA0, 0x3B, 0xBF, 0xD2, 0x5E, 0x8C,
            0xD0, 0x36, 0x41, 0x41,
        ];

        let mut result = [0u8; 32];
        let mut borrow: u16 = 0;
        for i in (0..32).rev() {
            let diff = (curve_order[i] as u16) - (s_bytes[i] as u16) - borrow;
            if diff > 255 {
                result[i] = (diff + 256) as u8;
                borrow = 1;
            } else {
                result[i] = diff as u8;
                borrow = 0;
            }
        }
        s_bytes = result.to_vec();
        info!("Signature s (normalized): {}", hex::encode(&s_bytes));
    }
    if let Ok(mut rec) = recorder.lock() {
        rec.record_step("7. Extract and normalize signature", step_start.elapsed());
    }

    let v = 0u8;
    let sig_data = SignatureData {
        r: r_bytes,
        s: s_bytes,
        v,
    };

    let elapsed = start.elapsed();

    // Complete benchmark and generate report
    let benchmark_report = if let Ok(mut rec) = recorder.lock() {
        rec.complete();
        let report = rec.report();
        report.log();
        Some(report)
    } else {
        None
    };

    Ok(SigningResult {
        success: true,
        signature: Some(sig_data),
        error: None,
        duration_secs: elapsed.as_secs_f64(),
        benchmark: benchmark_report,
    })
}
