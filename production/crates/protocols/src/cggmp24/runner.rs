//! Shared MPC Protocol Runner Infrastructure
//!
//! This module provides reusable Stream/Sink adapters for running CGGMP24 protocols
//! over HTTP relay networking.

use std::collections::HashMap;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_channel::{Receiver, Sender};
use futures::sink::Sink;
use futures::stream::Stream;
use pin_project_lite::pin_project;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::warn;

/// Network message wrapper for MPC protocols
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolMessage {
    pub session_id: String,
    pub sender: u16,
    pub recipient: Option<u16>,
    pub round: u16,
    pub payload: Vec<u8>,
    pub seq: u64,
}

/// Channel-based delivery for MPC protocols
pub struct ChannelDelivery {
    incoming_rx: Receiver<ProtocolMessage>,
    outgoing_tx: Sender<ProtocolMessage>,
    session_id: String,
    /// Our actual party index (for network messages)
    party_index: u16,
    seq: u64,
    /// Maps party_index (keygen) -> signer_index (position in signing group)
    /// Used to convert incoming message sender indices
    party_to_signer: Option<HashMap<u16, u16>>,
    /// Maps signer_index (position in signing group) -> party_index (keygen)
    /// Used to convert outgoing message recipient indices
    signer_to_party: Option<Vec<u16>>,
}

impl ChannelDelivery {
    pub fn new(
        incoming_rx: Receiver<ProtocolMessage>,
        outgoing_tx: Sender<ProtocolMessage>,
        session_id: String,
        party_index: u16,
    ) -> Self {
        Self {
            incoming_rx,
            outgoing_tx,
            session_id,
            party_index,
            seq: 0,
            party_to_signer: None,
            signer_to_party: None,
        }
    }

    /// Create a new ChannelDelivery with party-to-signer index mapping.
    ///
    /// This is needed for signing when the signing group uses non-contiguous
    /// party indices (e.g., parties [0, 1, 3] in a 4-party setup).
    ///
    /// The mappings work as follows:
    /// - `party_to_signer`: Converts incoming message sender (party_index) to
    ///   protocol-internal index (signer_index = position in parties array)
    /// - `signer_to_party`: Converts outgoing message recipient (signer_index)
    ///   to network-level party_index
    ///
    /// Example with parties = [0, 1, 3]:
    /// - party_to_signer: {0->0, 1->1, 3->2}
    /// - signer_to_party: [0, 1, 3] (index 0 -> party 0, index 1 -> party 1, index 2 -> party 3)
    pub fn new_with_mapping(
        incoming_rx: Receiver<ProtocolMessage>,
        outgoing_tx: Sender<ProtocolMessage>,
        session_id: String,
        party_index: u16,
        parties: &[u16],
    ) -> Self {
        // Build mapping: party_index -> signer_index (for incoming)
        let party_to_signer: HashMap<u16, u16> = parties
            .iter()
            .enumerate()
            .map(|(idx, &party)| (party, idx as u16))
            .collect();

        // Build reverse mapping: signer_index -> party_index (for outgoing)
        // This is just the parties array itself
        let signer_to_party: Vec<u16> = parties.to_vec();

        Self {
            incoming_rx,
            outgoing_tx,
            session_id,
            party_index, // Use actual party_index for network messages
            seq: 0,
            party_to_signer: Some(party_to_signer),
            signer_to_party: Some(signer_to_party),
        }
    }

    /// Split into stream and sink for round_based
    pub fn split<M>(self) -> (IncomingStream<M>, OutgoingSink<M>) {
        let incoming = IncomingStream {
            receiver: self.incoming_rx,
            session_id: self.session_id.clone(),
            party_to_signer: self.party_to_signer.clone(),
            _phantom: PhantomData,
        };
        let outgoing = OutgoingSink {
            sender: self.outgoing_tx,
            session_id: self.session_id,
            party_index: self.party_index,
            seq: self.seq,
            signer_to_party: self.signer_to_party,
            _phantom: PhantomData,
        };
        (incoming, outgoing)
    }
}

pin_project! {
    /// Stream adapter for incoming protocol messages
    pub struct IncomingStream<M> {
        #[pin]
        receiver: Receiver<ProtocolMessage>,
        session_id: String,
        // Maps party_index -> signer_index for incoming messages
        party_to_signer: Option<HashMap<u16, u16>>,
        _phantom: PhantomData<M>,
    }
}

impl<M: DeserializeOwned> Stream for IncomingStream<M> {
    type Item = Result<round_based::Incoming<M>, std::io::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();

        match this.receiver.poll_next(cx) {
            Poll::Ready(Some(msg)) => {
                // Filter by session
                if msg.session_id != *this.session_id {
                    warn!(
                        "Skipping message from wrong session: got {} expected {}",
                        msg.session_id, this.session_id
                    );
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }

                // Convert sender from party_index to signer_index if mapping exists
                let sender = if let Some(mapping) = this.party_to_signer {
                    match mapping.get(&msg.sender) {
                        Some(&signer_idx) => signer_idx,
                        None => {
                            warn!(
                                "Unknown sender party {} not in signing group, dropping message",
                                msg.sender
                            );
                            cx.waker().wake_by_ref();
                            return Poll::Pending;
                        }
                    }
                } else {
                    msg.sender
                };

                // Deserialize the protocol message
                match bincode::deserialize::<M>(&msg.payload) {
                    Ok(protocol_msg) => {
                        let msg_type = if msg.recipient.is_none() {
                            round_based::MessageType::Broadcast
                        } else {
                            round_based::MessageType::P2P
                        };

                        let incoming = round_based::Incoming {
                            id: msg.seq,
                            sender, // Use converted signer_index
                            msg_type,
                            msg: protocol_msg,
                        };

                        Poll::Ready(Some(Ok(incoming)))
                    }
                    Err(e) => {
                        warn!("Failed to deserialize protocol message: {}", e);
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                }
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

pin_project! {
    /// Sink adapter for outgoing protocol messages
    pub struct OutgoingSink<M> {
        sender: Sender<ProtocolMessage>,
        session_id: String,
        // Our actual party index (for network messages)
        party_index: u16,
        seq: u64,
        // Maps signer_index -> party_index for recipient conversion
        // Used when signing with non-contiguous participant sets
        signer_to_party: Option<Vec<u16>>,
        _phantom: PhantomData<M>,
    }
}

impl<M: Serialize> Sink<round_based::Outgoing<M>> for OutgoingSink<M> {
    type Error = std::io::Error;

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: round_based::Outgoing<M>) -> Result<(), Self::Error> {
        let this = self.project();
        *this.seq += 1;

        let payload =
            bincode::serialize(&item.msg).map_err(|e| std::io::Error::other(e.to_string()))?;

        // Convert recipient from signer_index to party_index if mapping exists
        let recipient = match item.recipient {
            round_based::MessageDestination::AllParties => None,
            round_based::MessageDestination::OneParty(signer_idx) => {
                // If we have a mapping, convert signer_index to party_index
                let party_idx = if let Some(mapping) = this.signer_to_party {
                    match mapping.get(signer_idx as usize) {
                        Some(&party) => party,
                        None => {
                            warn!(
                                "Invalid signer index {} (max {}), using as-is",
                                signer_idx,
                                mapping.len().saturating_sub(1)
                            );
                            signer_idx
                        }
                    }
                } else {
                    // No mapping, use as-is (keygen case where indices are contiguous)
                    signer_idx
                };
                Some(party_idx)
            }
        };

        let msg = ProtocolMessage {
            session_id: this.session_id.clone(),
            sender: *this.party_index,
            recipient,
            round: 0,
            payload,
            seq: *this.seq,
        };

        this.sender
            .try_send(msg)
            .map_err(|e| std::io::Error::other(e.to_string()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

/// Create an MPC party from channels
#[allow(clippy::type_complexity)]
pub fn create_mpc_party<M: Serialize + DeserializeOwned>(
    incoming_rx: Receiver<ProtocolMessage>,
    outgoing_tx: Sender<ProtocolMessage>,
    session_id: String,
    party_index: u16,
) -> round_based::MpcParty<M, (Pin<Box<IncomingStream<M>>>, Pin<Box<OutgoingSink<M>>>)> {
    let delivery = ChannelDelivery::new(incoming_rx, outgoing_tx, session_id, party_index);
    let (incoming, outgoing) = delivery.split();
    let incoming_boxed = Box::pin(incoming);
    let outgoing_boxed = Box::pin(outgoing);
    round_based::MpcParty::connected((incoming_boxed, outgoing_boxed))
}

/// Result of aux_info generation
#[derive(Debug)]
pub struct AuxInfoGenResult {
    pub success: bool,
    pub aux_info_data: Option<Vec<u8>>,
    pub error: Option<String>,
    pub duration_secs: f64,
}

/// Run the aux_info generation protocol
pub async fn run_aux_info_gen(
    party_index: u16,
    num_parties: u16,
    session_id: &str,
    primes_data: &[u8],
    incoming_rx: Receiver<ProtocolMessage>,
    outgoing_tx: Sender<ProtocolMessage>,
) -> AuxInfoGenResult {
    use rand::rngs::OsRng;
    use tracing::{error, info};

    let start = std::time::Instant::now();

    info!("========================================");
    info!("  AUX INFO GENERATION STARTING");
    info!("========================================");
    info!("Party index: {}", party_index);
    info!("Num parties: {}", num_parties);
    info!("Session ID: {}", session_id);

    // Deserialize primes
    let primes: cggmp24::PregeneratedPrimes<cggmp24::security_level::SecurityLevel128> =
        match bincode::deserialize(primes_data) {
            Ok(p) => p,
            Err(e) => {
                return AuxInfoGenResult {
                    success: false,
                    aux_info_data: None,
                    error: Some(format!("Failed to deserialize primes: {}", e)),
                    duration_secs: start.elapsed().as_secs_f64(),
                }
            }
        };

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

    // Box the stream to satisfy Unpin requirement for round_based::Delivery
    let incoming_boxed = Box::pin(incoming);
    let outgoing_boxed = Box::pin(outgoing);

    // Create MPC party (takes a tuple of (stream, sink))
    let party = round_based::MpcParty::connected((incoming_boxed, outgoing_boxed));

    // Run the protocol
    info!("Starting aux_info_gen protocol...");
    let result = cggmp24::aux_info_gen(eid, party_index, num_parties, primes)
        .start(&mut OsRng, party)
        .await;

    let elapsed = start.elapsed();

    match result {
        Ok(aux_info) => {
            info!(
                "Aux info generation completed successfully in {:.2}s",
                elapsed.as_secs_f64()
            );

            // Serialize aux_info using JSON (supports deserialize_any)
            match serde_json::to_vec(&aux_info) {
                Ok(data) => {
                    info!("Aux info serialized: {} bytes", data.len());
                    AuxInfoGenResult {
                        success: true,
                        aux_info_data: Some(data),
                        error: None,
                        duration_secs: elapsed.as_secs_f64(),
                    }
                }
                Err(e) => AuxInfoGenResult {
                    success: false,
                    aux_info_data: None,
                    error: Some(format!("Failed to serialize aux_info: {}", e)),
                    duration_secs: elapsed.as_secs_f64(),
                },
            }
        }
        Err(e) => {
            error!("Aux info generation failed: {:?}", e);
            AuxInfoGenResult {
                success: false,
                aux_info_data: None,
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
            session_id: "test".to_string(),
            sender: 0,
            recipient: Some(1),
            round: 1,
            payload: vec![1, 2, 3],
            seq: 42,
        };

        let serialized = bincode::serialize(&msg).unwrap();
        let deserialized: ProtocolMessage = bincode::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.session_id, msg.session_id);
        assert_eq!(deserialized.sender, msg.sender);
        assert_eq!(deserialized.seq, msg.seq);
    }
}
