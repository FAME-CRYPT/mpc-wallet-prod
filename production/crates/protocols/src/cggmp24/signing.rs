//! CGGMP24 Threshold Signing Protocol Runner
//!
//! This module runs the CGGMP24 threshold signing protocol with detailed
//! benchmarking for each step.

use async_channel::{Receiver, Sender};
use cggmp24::key_share::Validate;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tracing::{error, info};

use crate::bench::{BenchmarkRecorder, BenchmarkReport};
use crate::cggmp24::runner::{ChannelDelivery, ProtocolMessage};

/// Result of signing
#[derive(Debug)]
pub struct SigningResult {
    pub success: bool,
    pub signature: Option<SignatureData>,
    pub error: Option<String>,
    pub duration_secs: f64,
    /// Detailed benchmark report (if benchmarking enabled)
    pub benchmark: Option<BenchmarkReport>,
}

/// ECDSA signature data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureData {
    /// r component (32 bytes, big-endian)
    pub r: Vec<u8>,
    /// s component (32 bytes, big-endian)
    pub s: Vec<u8>,
    /// Recovery ID (0 or 1)
    pub v: u8,
}

impl SignatureData {
    /// Convert to DER format for Bitcoin
    pub fn to_der(&self) -> Vec<u8> {
        let mut r = self.r.clone();
        let mut s = self.s.clone();

        // Remove leading zeros but keep one if needed for sign bit
        while r.len() > 1 && r[0] == 0 && r[1] & 0x80 == 0 {
            r.remove(0);
        }
        while s.len() > 1 && s[0] == 0 && s[1] & 0x80 == 0 {
            s.remove(0);
        }

        // Add leading zero if high bit is set (ensures positive integer)
        if !r.is_empty() && r[0] & 0x80 != 0 {
            r.insert(0, 0x00);
        }
        if !s.is_empty() && s[0] & 0x80 != 0 {
            s.insert(0, 0x00);
        }

        let r_len = r.len();
        let s_len = s.len();
        let total_len = 2 + r_len + 2 + s_len;

        let mut der = Vec::with_capacity(2 + total_len);
        der.push(0x30); // SEQUENCE
        der.push(total_len as u8);
        der.push(0x02); // INTEGER
        der.push(r_len as u8);
        der.extend_from_slice(&r);
        der.push(0x02); // INTEGER
        der.push(s_len as u8);
        der.extend_from_slice(&s);

        der
    }

    /// Convert to compact 64-byte format (r || s)
    pub fn to_compact(&self) -> [u8; 64] {
        let mut compact = [0u8; 64];

        // Pad r to 32 bytes
        let r_start = 32 - self.r.len().min(32);
        compact[r_start..32].copy_from_slice(&self.r[self.r.len().saturating_sub(32)..]);

        // Pad s to 32 bytes
        let s_start = 64 - self.s.len().min(32);
        compact[s_start..64].copy_from_slice(&self.s[self.s.len().saturating_sub(32)..]);

        compact
    }
}

/// Run the CGGMP24 signing protocol with benchmarking
#[allow(clippy::too_many_arguments)]
pub async fn run_signing(
    party_index: u16,
    parties: &[u16],
    session_id: &str,
    message_hash: &[u8; 32],
    key_share_data: &[u8],
    aux_info_data: &[u8],
    incoming_rx: Receiver<ProtocolMessage>,
    outgoing_tx: Sender<ProtocolMessage>,
) -> SigningResult {
    run_signing_with_benchmark(
        party_index,
        parties,
        session_id,
        message_hash,
        key_share_data,
        aux_info_data,
        incoming_rx,
        outgoing_tx,
        true, // Enable benchmarking by default
    )
    .await
}

/// Run the CGGMP24 signing protocol with optional benchmarking
#[allow(clippy::too_many_arguments)]
pub async fn run_signing_with_benchmark(
    party_index: u16,
    parties: &[u16],
    session_id: &str,
    message_hash: &[u8; 32],
    key_share_data: &[u8],
    aux_info_data: &[u8],
    incoming_rx: Receiver<ProtocolMessage>,
    outgoing_tx: Sender<ProtocolMessage>,
    enable_benchmark: bool,
) -> SigningResult {
    let start = std::time::Instant::now();

    // Initialize benchmark recorder
    let recorder = Arc::new(Mutex::new(BenchmarkRecorder::new(
        "CGGMP24-Signing",
        party_index,
        session_id,
    )));

    // Convert party_index (keygen index) to signer_index (position in parties array)
    // The cggmp24 library expects the signer index to be the position within the signing group,
    // not the original keygen party index.
    let signer_index = match parties.iter().position(|&p| p == party_index) {
        Some(idx) => idx as u16,
        None => {
            error!(
                "Party {} is not in the signing parties list {:?}",
                party_index, parties
            );
            return SigningResult {
                success: false,
                signature: None,
                error: Some(format!(
                    "Party {} is not a signer in parties {:?}",
                    party_index, parties
                )),
                duration_secs: start.elapsed().as_secs_f64(),
                benchmark: None,
            };
        }
    };

    info!("========================================");
    info!("  CGGMP24 THRESHOLD SIGNING STARTING");
    info!("========================================");
    info!(
        "Party index: {} (signer index: {})",
        party_index, signer_index
    );
    info!("Signing parties: {:?}", parties);
    info!("Session ID: {}", session_id);
    info!("Message hash: {}", hex::encode(message_hash));
    info!(
        "Benchmarking: {}",
        if enable_benchmark {
            "enabled"
        } else {
            "disabled"
        }
    );

    // Step 1: Deserialize aux_info
    let step_start = std::time::Instant::now();
    let dirty_aux_info: cggmp24::key_share::DirtyAuxInfo<
        cggmp24::security_level::SecurityLevel128,
    > = match serde_json::from_slice(aux_info_data) {
        Ok(a) => {
            info!("Aux info deserialized successfully");
            a
        }
        Err(e) => {
            error!("Failed to deserialize aux_info: {}", e);
            return SigningResult {
                success: false,
                signature: None,
                error: Some(format!("Failed to deserialize aux_info: {}", e)),
                duration_secs: start.elapsed().as_secs_f64(),
                benchmark: None,
            };
        }
    };
    if enable_benchmark {
        if let Ok(mut rec) = recorder.lock() {
            rec.record_step("1. Deserialize aux_info", step_start.elapsed());
        }
    }

    // Step 2: Validate aux_info
    let step_start = std::time::Instant::now();
    let aux_info = match dirty_aux_info.validate() {
        Ok(a) => a,
        Err(e) => {
            error!("Failed to validate aux_info: {:?}", e);
            return SigningResult {
                success: false,
                signature: None,
                error: Some(format!("Invalid aux_info: {:?}", e)),
                duration_secs: start.elapsed().as_secs_f64(),
                benchmark: None,
            };
        }
    };
    if enable_benchmark {
        if let Ok(mut rec) = recorder.lock() {
            rec.record_step("2. Validate aux_info", step_start.elapsed());
        }
    }

    // Step 3: Deserialize key share
    let step_start = std::time::Instant::now();
    let dirty_key_share: cggmp24::key_share::DirtyIncompleteKeyShare<
        cggmp24::supported_curves::Secp256k1,
    > = match serde_json::from_slice(key_share_data) {
        Ok(k) => {
            info!("Key share deserialized successfully");
            k
        }
        Err(e) => {
            error!("Failed to deserialize key share: {}", e);
            return SigningResult {
                success: false,
                signature: None,
                error: Some(format!("Failed to deserialize key share: {}", e)),
                duration_secs: start.elapsed().as_secs_f64(),
                benchmark: None,
            };
        }
    };
    if enable_benchmark {
        if let Ok(mut rec) = recorder.lock() {
            rec.record_step("3. Deserialize key_share", step_start.elapsed());
        }
    }

    // Step 4: Validate key share
    let step_start = std::time::Instant::now();
    let incomplete_key_share = match dirty_key_share.validate() {
        Ok(k) => k,
        Err(e) => {
            error!("Failed to validate key share: {:?}", e);
            return SigningResult {
                success: false,
                signature: None,
                error: Some(format!("Invalid key share: {:?}", e)),
                duration_secs: start.elapsed().as_secs_f64(),
                benchmark: None,
            };
        }
    };
    if enable_benchmark {
        if let Ok(mut rec) = recorder.lock() {
            rec.record_step("4. Validate key_share", step_start.elapsed());
        }
    }

    // Step 5: Combine into full key share
    let step_start = std::time::Instant::now();
    let key_share = match cggmp24::KeyShare::from_parts((incomplete_key_share, aux_info)) {
        Ok(ks) => ks,
        Err(e) => {
            error!("Failed to construct key share: {:?}", e);
            return SigningResult {
                success: false,
                signature: None,
                error: Some(format!("Failed to construct key share: {:?}", e)),
                duration_secs: start.elapsed().as_secs_f64(),
                benchmark: None,
            };
        }
    };
    if enable_benchmark {
        if let Ok(mut rec) = recorder.lock() {
            rec.record_step("5. Construct full key_share", step_start.elapsed());
        }
    }

    // Step 6: Create execution ID and prepare protocol
    let step_start = std::time::Instant::now();
    let eid = cggmp24::ExecutionId::new(session_id.as_bytes());

    // Create delivery channels with party-to-signer index mapping
    // The mapping handles:
    // - Incoming: Converts network party_index to protocol signer_index
    // - Outgoing: Converts protocol recipient signer_index to network party_index
    let delivery = ChannelDelivery::new_with_mapping(
        incoming_rx,
        outgoing_tx,
        session_id.to_string(),
        party_index, // Use actual party_index for network messages
        parties,
    );
    let (incoming, outgoing) = delivery.split();
    let incoming_boxed = Box::pin(incoming);
    let outgoing_boxed = Box::pin(outgoing);

    // Create MPC party
    let party = round_based::MpcParty::connected((incoming_boxed, outgoing_boxed));

    // Create the message to sign from the prehashed data
    let scalar = cggmp24::generic_ec::Scalar::<cggmp24::supported_curves::Secp256k1>::from_be_bytes_mod_order(message_hash);
    let message = cggmp24::PrehashedDataToSign::from_scalar(scalar);
    if enable_benchmark {
        if let Ok(mut rec) = recorder.lock() {
            rec.record_step(
                "6. Protocol setup (channels, message)",
                step_start.elapsed(),
            );
        }
    }

    // Step 7: Run the signing protocol (this is the main MPC computation)
    info!("Starting signing protocol...");
    let step_start = std::time::Instant::now();
    // Use signer_index (position in parties array), not party_index (keygen index)
    let signing_result = cggmp24::signing(eid, signer_index, parties, &key_share)
        .sign(&mut OsRng, party, &message)
        .await;
    if enable_benchmark {
        if let Ok(mut rec) = recorder.lock() {
            rec.record_step("7. MPC signing protocol", step_start.elapsed());
        }
    }

    let elapsed = start.elapsed();

    match signing_result {
        Ok(signature) => {
            // Step 8: Extract signature components
            let step_start = std::time::Instant::now();
            info!(
                "Signing completed successfully in {:.2}s",
                elapsed.as_secs_f64()
            );

            // Extract r and s from the signature
            let r_bytes = (*signature.r).to_be_bytes().to_vec();
            let mut s_bytes = (*signature.s).to_be_bytes().to_vec();

            info!("Signature r: {}", hex::encode(&r_bytes));
            info!("Signature s (raw): {}", hex::encode(&s_bytes));
            if enable_benchmark {
                if let Ok(mut rec) = recorder.lock() {
                    rec.record_step("8. Extract signature components", step_start.elapsed());
                }
            }

            // Step 9: Normalize S for Bitcoin (low-S)
            let step_start = std::time::Instant::now();
            let half_order: [u8; 32] = [
                0x7F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                0xFF, 0xFF, 0x5D, 0x57, 0x6E, 0x73, 0x57, 0xA4, 0x50, 0x1D, 0xDF, 0xE9, 0x2F, 0x46,
                0x68, 0x1B, 0x20, 0xA0,
            ];

            let s_is_high = {
                let mut s_arr = [0u8; 32];
                s_arr.copy_from_slice(&s_bytes);
                s_arr > half_order
            };

            if s_is_high {
                info!("S is high, normalizing to low-S form for Bitcoin");
                let curve_order: [u8; 32] = [
                    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                    0xFF, 0xFF, 0xFE, 0xBA, 0xAE, 0xDC, 0xE6, 0xAF, 0x48, 0xA0, 0x3B, 0xBF, 0xD2,
                    0x5E, 0x8C, 0xD0, 0x36, 0x41, 0x41,
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
            if enable_benchmark {
                if let Ok(mut rec) = recorder.lock() {
                    rec.record_step("9. Normalize S (low-S for Bitcoin)", step_start.elapsed());
                }
            }

            let v = 0u8;

            let sig_data = SignatureData {
                r: r_bytes,
                s: s_bytes,
                v,
            };

            // Complete benchmark and generate report
            let benchmark_report = if enable_benchmark {
                if let Ok(mut rec) = recorder.lock() {
                    rec.complete();
                    let report = rec.report();
                    report.log();
                    Some(report)
                } else {
                    None
                }
            } else {
                None
            };

            SigningResult {
                success: true,
                signature: Some(sig_data),
                error: None,
                duration_secs: elapsed.as_secs_f64(),
                benchmark: benchmark_report,
            }
        }
        Err(e) => {
            error!("Signing failed: {:?}", e);

            // Complete benchmark even on failure
            let benchmark_report = if enable_benchmark {
                if let Ok(mut rec) = recorder.lock() {
                    rec.complete();
                    let report = rec.report();
                    report.log();
                    Some(report)
                } else {
                    None
                }
            } else {
                None
            };

            SigningResult {
                success: false,
                signature: None,
                error: Some(format!("Protocol error: {:?}", e)),
                duration_secs: elapsed.as_secs_f64(),
                benchmark: benchmark_report,
            }
        }
    }
}
