//! FROST Distributed Key Generation using Givre library.
//!
//! This module implements threshold key generation for Schnorr signatures
//! using the FROST protocol. The resulting keys can be used for Taproot
//! Bitcoin transactions.

use async_channel::{Receiver, Sender};
use pin_project_lite::pin_project;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::task::{Context, Poll};
use tracing::{error, info};

use givre::ciphersuite::{Bitcoin, Ciphersuite};
use givre::keygen::security_level::SecurityLevel128;

/// Type alias for the FROST keygen message type
/// The Msg type takes: Curve, SecurityLevel, Digest
/// Using Bitcoin ciphersuite for BIP-340 compliant signatures
type FrostKeygenMsg = givre::keygen::msg::threshold::Msg<
    <Bitcoin as Ciphersuite>::Curve,
    SecurityLevel128,
    sha2::Sha256,
>;

/// Protocol message for FROST keygen relay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolMessage {
    pub session_id: String,
    pub sender: u16,
    pub recipient: Option<u16>,
    pub round: u16,
    pub payload: Vec<u8>,
    pub seq: u64,
}

/// Result of FROST key generation.
#[derive(Debug)]
pub struct FrostKeygenResult {
    pub success: bool,
    /// Serialized key share (for storage)
    pub key_share_data: Option<Vec<u8>>,
    /// The aggregated public key (x-only, 32 bytes) for address derivation
    pub public_key: Option<Vec<u8>>,
    /// Error message if failed
    pub error: Option<String>,
    /// Duration of the protocol
    pub duration_secs: f64,
}

pin_project! {
    /// Wrapper to adapt our async channels to round_based Stream.
    pub struct ChannelStream {
        #[pin]
        receiver: Receiver<ProtocolMessage>,
    }
}

impl futures::Stream for ChannelStream {
    type Item = Result<round_based::Incoming<FrostKeygenMsg>, std::io::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();

        match this.receiver.poll_next(cx) {
            Poll::Ready(Some(msg)) => {
                // Deserialize the payload
                match serde_json::from_slice(&msg.payload) {
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
                        error!("Failed to deserialize FROST keygen message: {}", e);
                        Poll::Ready(Some(Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            e,
                        ))))
                    }
                }
            }
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

impl futures::Sink<round_based::Outgoing<FrostKeygenMsg>> for ChannelSink {
    type Error = std::io::Error;

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(
        self: Pin<&mut Self>,
        item: round_based::Outgoing<FrostKeygenMsg>,
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

        // Use try_send for non-blocking send
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

/// Run FROST distributed key generation.
///
/// This generates threshold Schnorr key shares that can be used for
/// Taproot Bitcoin transactions.
pub async fn run_frost_keygen(
    party_index: u16,
    num_parties: u16,
    threshold: u16,
    session_id: &str,
    incoming_rx: Receiver<ProtocolMessage>,
    outgoing_tx: Sender<ProtocolMessage>,
) -> FrostKeygenResult {
    info!("========================================");
    info!("  FROST KEY GENERATION STARTING");
    info!("========================================");
    info!("Party index: {}", party_index);
    info!("Number of parties: {}", num_parties);
    info!("Threshold: {}-of-{}", threshold, num_parties);
    info!("Session ID: {}", session_id);

    let start = std::time::Instant::now();

    // Create execution ID from session
    let eid = givre::keygen::ExecutionId::new(session_id.as_bytes());

    // Create Stream and Sink adapters
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

    // Run FROST keygen using Givre with Bitcoin ciphersuite for BIP-340 compliance
    info!("Starting FROST keygen protocol (Bitcoin/BIP-340 ciphersuite)...");
    let keygen_result =
        givre::keygen::<<Bitcoin as Ciphersuite>::Curve>(eid, party_index, num_parties)
            .set_threshold(threshold)
            .start(&mut OsRng, party)
            .await;

    let elapsed = start.elapsed();

    match keygen_result {
        Ok(key_share) => {
            info!(
                "FROST keygen completed successfully in {:.2}s",
                elapsed.as_secs_f64()
            );

            // Get the public key from the key share
            // For FROST/Schnorr, we need the x-only public key (32 bytes)
            let shared_public_key = key_share.shared_public_key();

            // Convert to bytes - get the x-coordinate for Taproot
            // The Point type should have a method to get the coordinates
            let pk_bytes = shared_public_key.to_bytes(true); // compressed format

            // For compressed format (33 bytes): prefix || x-coordinate
            // We need just the x-coordinate (32 bytes) for Taproot
            let public_key_bytes = if pk_bytes.len() == 33 {
                // Compressed: 0x02/0x03 || x (32 bytes)
                pk_bytes[1..33].to_vec()
            } else if pk_bytes.len() == 32 {
                // Already x-only
                pk_bytes.to_vec()
            } else {
                error!("Unexpected public key length: {}", pk_bytes.len());
                return FrostKeygenResult {
                    success: false,
                    key_share_data: None,
                    public_key: None,
                    error: Some(format!("Unexpected public key length: {}", pk_bytes.len())),
                    duration_secs: elapsed.as_secs_f64(),
                };
            };

            info!(
                "Shared public key (x-only): {}",
                hex::encode(&public_key_bytes)
            );

            // Serialize the key share for storage
            let key_share_data = match serde_json::to_vec(&key_share) {
                Ok(data) => {
                    info!("Key share serialized: {} bytes", data.len());
                    data
                }
                Err(e) => {
                    error!("Failed to serialize key share: {}", e);
                    return FrostKeygenResult {
                        success: false,
                        key_share_data: None,
                        public_key: None,
                        error: Some(format!("Serialization error: {}", e)),
                        duration_secs: elapsed.as_secs_f64(),
                    };
                }
            };

            FrostKeygenResult {
                success: true,
                key_share_data: Some(key_share_data),
                public_key: Some(public_key_bytes),
                error: None,
                duration_secs: elapsed.as_secs_f64(),
            }
        }
        Err(e) => {
            error!("FROST keygen failed: {:?}", e);
            FrostKeygenResult {
                success: false,
                key_share_data: None,
                public_key: None,
                error: Some(format!("Protocol error: {:?}", e)),
                duration_secs: elapsed.as_secs_f64(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_message_serialization() {
        let msg = ProtocolMessage {
            session_id: "test-session".to_string(),
            sender: 0,
            recipient: None,
            round: 1,
            payload: vec![1, 2, 3],
            seq: 1,
        };

        let serialized = serde_json::to_string(&msg).unwrap();
        let deserialized: ProtocolMessage = serde_json::from_str(&serialized).unwrap();

        assert_eq!(msg.session_id, deserialized.session_id);
        assert_eq!(msg.sender, deserialized.sender);
    }
}
