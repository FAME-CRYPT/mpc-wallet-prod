//! FROST Signing using Givre library.
//!
//! This module implements threshold Schnorr signing for Taproot Bitcoin
//! transactions using the FROST protocol with detailed benchmarking.

use async_channel::{Receiver, Sender};
use pin_project_lite::pin_project;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tracing::{debug, error, info};

use givre::ciphersuite::{Bitcoin, Ciphersuite};

use crate::bench::{BenchmarkRecorder, BenchmarkReport};

/// Type alias for FROST signing message
/// Using Bitcoin ciphersuite for BIP-340 compliant signatures
type FrostSigningMsg = givre::signing::full_signing::Msg<<Bitcoin as Ciphersuite>::Curve>;

/// Protocol message for FROST signing relay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolMessage {
    pub session_id: String,
    pub sender: u16,
    pub recipient: Option<u16>,
    pub round: u16,
    pub payload: Vec<u8>,
    pub seq: u64,
}

/// Schnorr signature data (64 bytes for Taproot).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchnorrSignature {
    /// The signature R component (32 bytes, x-only)
    pub r: Vec<u8>,
    /// The signature s component (32 bytes)
    pub s: Vec<u8>,
}

impl SchnorrSignature {
    /// Convert to 64-byte Schnorr signature format for Bitcoin Taproot.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut sig = Vec::with_capacity(64);
        sig.extend_from_slice(&self.r);
        sig.extend_from_slice(&self.s);
        sig
    }
}

/// Result of FROST signing.
#[derive(Debug)]
pub struct FrostSigningResult {
    pub success: bool,
    /// The Schnorr signature (64 bytes)
    pub signature: Option<SchnorrSignature>,
    /// Error message if failed
    pub error: Option<String>,
    /// Duration of the protocol
    pub duration_secs: f64,
    /// Detailed benchmark report (if benchmarking enabled)
    pub benchmark: Option<BenchmarkReport>,
}

pin_project! {
    /// Wrapper to adapt our async channels to round_based Stream.
    pub struct ChannelStream {
        #[pin]
        receiver: Receiver<ProtocolMessage>,
    }
}

impl futures::Stream for ChannelStream {
    type Item = Result<round_based::Incoming<FrostSigningMsg>, std::io::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();

        match this.receiver.poll_next(cx) {
            Poll::Ready(Some(msg)) => match serde_json::from_slice(&msg.payload) {
                Ok(protocol_msg) => {
                    let incoming = round_based::Incoming {
                        id: msg.seq,
                        sender: msg.sender,
                        msg_type: if msg.recipient.is_some() {
                            round_based::MessageType::P2P
                        } else {
                            round_based::MessageType::Broadcast
                        },
                        msg: protocol_msg,
                    };
                    Poll::Ready(Some(Ok(incoming)))
                }
                Err(e) => {
                    error!("Failed to deserialize FROST signing message: {}", e);
                    Poll::Ready(Some(Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        e,
                    ))))
                }
            },
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

pin_project! {
    /// Wrapper to adapt our async channels to round_based Sink.
    pub struct ChannelSink {
        sender: Sender<ProtocolMessage>,
        session_id: String,
        party_index: u16,
        seq: u64,
    }
}

impl futures::Sink<round_based::Outgoing<FrostSigningMsg>> for ChannelSink {
    type Error = std::io::Error;

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(
        self: Pin<&mut Self>,
        item: round_based::Outgoing<FrostSigningMsg>,
    ) -> Result<(), Self::Error> {
        let this = self.project();
        *this.seq += 1;
        let seq = *this.seq;

        let (recipient, round) = match &item.recipient {
            round_based::MessageDestination::AllParties => (None, 0),
            round_based::MessageDestination::OneParty(p) => (Some(*p), 0),
        };

        let payload = serde_json::to_vec(&item.msg)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let msg = ProtocolMessage {
            session_id: this.session_id.clone(),
            sender: *this.party_index,
            recipient,
            round,
            payload,
            seq,
        };

        this.sender
            .try_send(msg)
            .map_err(|e| std::io::Error::other(e.to_string()))?;

        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

/// FROST key share type alias for convenience.
/// Using Bitcoin ciphersuite for BIP-340 compliant signatures
pub type FrostKeyShare = givre::KeyShare<<Bitcoin as Ciphersuite>::Curve>;

/// Run FROST threshold signing with benchmarking.
///
/// This produces a Schnorr signature that can be used in Taproot Bitcoin
/// transactions. The signature is 64 bytes (R || s).
pub async fn run_frost_signing(
    party_index: u16,
    parties_at_keygen: &[u16],
    session_id: &str,
    message_hash: &[u8; 32],
    key_share_data: &[u8],
    incoming_rx: Receiver<ProtocolMessage>,
    outgoing_tx: Sender<ProtocolMessage>,
) -> FrostSigningResult {
    run_frost_signing_with_benchmark(
        party_index,
        parties_at_keygen,
        session_id,
        message_hash,
        key_share_data,
        incoming_rx,
        outgoing_tx,
        true, // Enable benchmarking by default
    )
    .await
}

/// Run FROST threshold signing with optional benchmarking.
#[allow(clippy::too_many_arguments)]
pub async fn run_frost_signing_with_benchmark(
    party_index: u16,
    parties_at_keygen: &[u16],
    session_id: &str,
    message_hash: &[u8; 32],
    key_share_data: &[u8],
    incoming_rx: Receiver<ProtocolMessage>,
    outgoing_tx: Sender<ProtocolMessage>,
    enable_benchmark: bool,
) -> FrostSigningResult {
    let start = std::time::Instant::now();

    // Initialize benchmark recorder
    let recorder = Arc::new(Mutex::new(BenchmarkRecorder::new(
        "FROST-Signing",
        party_index,
        session_id,
    )));

    info!("========================================");
    info!("  FROST SIGNING STARTING");
    info!("========================================");
    info!("Party index: {}", party_index);
    info!("Parties at keygen: {:?}", parties_at_keygen);
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

    // Step 1: Deserialize the key share
    let step_start = std::time::Instant::now();
    let key_share: FrostKeyShare = match serde_json::from_slice(key_share_data) {
        Ok(ks) => ks,
        Err(e) => {
            error!("Failed to deserialize key share: {}", e);
            return FrostSigningResult {
                success: false,
                signature: None,
                error: Some(format!("Key share deserialization error: {}", e)),
                duration_secs: start.elapsed().as_secs_f64(),
                benchmark: None,
            };
        }
    };
    if enable_benchmark {
        if let Ok(mut rec) = recorder.lock() {
            rec.record_step("1. Deserialize key_share", step_start.elapsed());
        }
    }

    // Log the shared public key for debugging
    let shared_pubkey = key_share.shared_public_key();
    let pubkey_bytes = shared_pubkey.to_bytes(true); // compressed format
    debug!(
        "Shared public key (compressed): {}",
        hex::encode(&pubkey_bytes)
    );

    // Step 2: Create Stream and Sink adapters
    let step_start = std::time::Instant::now();
    let incoming_stream = ChannelStream {
        receiver: incoming_rx,
    };

    let outgoing_sink = ChannelSink {
        sender: outgoing_tx,
        session_id: session_id.to_string(),
        party_index,
        seq: 0,
    };

    // Box the stream and sink for the MpcParty
    let incoming_boxed = Box::pin(incoming_stream);
    let outgoing_boxed = Box::pin(outgoing_sink);

    // Create the MPC party
    let party = round_based::MpcParty::connected((incoming_boxed, outgoing_boxed));
    if enable_benchmark {
        if let Ok(mut rec) = recorder.lock() {
            rec.record_step("2. Protocol setup (channels, party)", step_start.elapsed());
        }
    }

    // Step 3: Create signing builder
    let step_start = std::time::Instant::now();
    info!("Starting FROST signing protocol (Bitcoin/BIP-340 ciphersuite)...");
    let signing_builder =
        givre::signing::<Bitcoin>(party_index, &key_share, parties_at_keygen, message_hash);
    if enable_benchmark {
        if let Ok(mut rec) = recorder.lock() {
            rec.record_step("3. Create signing builder", step_start.elapsed());
        }
    }

    // Step 4: Set taproot tweak
    let step_start = std::time::Instant::now();
    let signing_builder = match signing_builder.set_taproot_tweak(None) {
        Ok(builder) => builder,
        Err(e) => {
            error!("Failed to set taproot tweak: {:?}", e);
            return FrostSigningResult {
                success: false,
                signature: None,
                error: Some(format!("Failed to set taproot tweak: {:?}", e)),
                duration_secs: start.elapsed().as_secs_f64(),
                benchmark: None,
            };
        }
    };
    if enable_benchmark {
        if let Ok(mut rec) = recorder.lock() {
            rec.record_step("4. Set taproot tweak (BIP-341)", step_start.elapsed());
        }
    }

    // Step 5: Run the signing protocol (main MPC computation)
    let step_start = std::time::Instant::now();
    let signing_result = signing_builder.sign(&mut OsRng, party).await;
    if enable_benchmark {
        if let Ok(mut rec) = recorder.lock() {
            rec.record_step("5. MPC signing protocol", step_start.elapsed());
        }
    }

    let elapsed = start.elapsed();

    match signing_result {
        Ok(signature) => {
            info!(
                "FROST signing completed successfully in {:.2}s",
                elapsed.as_secs_f64()
            );

            // Step 6: Extract signature components
            let step_start = std::time::Instant::now();
            let r_point_bytes: Vec<u8> = signature.r.to_bytes().into();
            let r = if r_point_bytes.len() == 33 {
                debug!(
                    "R point in compressed format, prefix: 0x{:02x}",
                    r_point_bytes[0]
                );
                r_point_bytes[1..33].to_vec()
            } else if r_point_bytes.len() == 32 {
                debug!("R point already in x-only format");
                r_point_bytes
            } else {
                error!("Unexpected R point length: {}", r_point_bytes.len());
                return FrostSigningResult {
                    success: false,
                    signature: None,
                    error: Some(format!(
                        "Unexpected R point length: {}",
                        r_point_bytes.len()
                    )),
                    duration_secs: elapsed.as_secs_f64(),
                    benchmark: None,
                };
            };

            let z_bytes = signature.z.to_be_bytes();
            let s = z_bytes.as_ref().to_vec();

            if r.len() != 32 || s.len() != 32 {
                error!(
                    "Unexpected signature component lengths: R={}, s={}",
                    r.len(),
                    s.len()
                );
                return FrostSigningResult {
                    success: false,
                    signature: None,
                    error: Some(format!(
                        "Unexpected signature lengths: R={}, s={}",
                        r.len(),
                        s.len()
                    )),
                    duration_secs: elapsed.as_secs_f64(),
                    benchmark: None,
                };
            }
            if enable_benchmark {
                if let Ok(mut rec) = recorder.lock() {
                    rec.record_step("6. Extract signature components", step_start.elapsed());
                }
            }

            info!("BIP-340 Signature R: {}", hex::encode(&r));
            info!("BIP-340 Signature s: {}", hex::encode(&s));

            let schnorr_sig = SchnorrSignature { r, s };

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

            FrostSigningResult {
                success: true,
                signature: Some(schnorr_sig),
                error: None,
                duration_secs: elapsed.as_secs_f64(),
                benchmark: benchmark_report,
            }
        }
        Err(e) => {
            error!("FROST signing failed: {:?}", e);

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

            FrostSigningResult {
                success: false,
                signature: None,
                error: Some(format!("Protocol error: {:?}", e)),
                duration_secs: elapsed.as_secs_f64(),
                benchmark: benchmark_report,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schnorr_signature_to_bytes() {
        let sig = SchnorrSignature {
            r: vec![1u8; 32],
            s: vec![2u8; 32],
        };

        let bytes = sig.to_bytes();
        assert_eq!(bytes.len(), 64);
        assert_eq!(&bytes[..32], &[1u8; 32]);
        assert_eq!(&bytes[32..], &[2u8; 32]);
    }
}
