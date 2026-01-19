//! Local MPC simulation for benchmarking.
//!
//! This module provides infrastructure to run multi-party protocols locally
//! without network communication, enabling accurate benchmarking of protocol
//! performance.

use async_channel::{bounded, Receiver, Sender};

/// Type alias for FROST keygen channel pair (outgoing_tx, incoming_rx)
pub type FrostKeygenChannels = Vec<(
    Sender<crate::frost::keygen::ProtocolMessage>,
    Receiver<crate::frost::keygen::ProtocolMessage>,
)>;

/// Type alias for FROST signing channel pair (outgoing_tx, incoming_rx)
pub type FrostSigningChannels = Vec<(
    Sender<crate::frost::signing::ProtocolMessage>,
    Receiver<crate::frost::signing::ProtocolMessage>,
)>;

/// Type alias for CGGMP24 channel pair (outgoing_tx, incoming_rx)
pub type Cggmp24Channels = Vec<(
    Sender<crate::cggmp24::runner::ProtocolMessage>,
    Receiver<crate::cggmp24::runner::ProtocolMessage>,
)>;

/// Generic protocol message with routing information.
#[derive(Debug, Clone)]
pub struct RoutableMessage<P> {
    pub sender: u16,
    pub recipient: Option<u16>, // None = broadcast
    pub payload: P,
}

/// A message relay that routes messages between parties.
///
/// Each party gets an outgoing channel to send messages, and the relay
/// forwards them to the appropriate recipients.
pub struct MessageRelay {
    /// Receivers for outgoing messages from each party
    outgoing_rxs: Vec<Receiver<crate::frost::keygen::ProtocolMessage>>,
    /// Senders to deliver messages to each party
    incoming_txs: Vec<Sender<crate::frost::keygen::ProtocolMessage>>,
}

impl MessageRelay {
    /// Create channels for all parties and return the relay plus per-party channel pairs.
    ///
    /// Returns: (relay, Vec<(outgoing_tx, incoming_rx)>)
    /// - outgoing_tx: Party sends protocol messages here
    /// - incoming_rx: Party receives protocol messages here
    pub fn new_frost_keygen(
        num_parties: u16,
        channel_capacity: usize,
    ) -> (Self, FrostKeygenChannels) {
        let mut outgoing_rxs = Vec::new();
        let mut incoming_txs = Vec::new();
        let mut party_channels = Vec::new();

        for _ in 0..num_parties {
            // Channel for party to send outgoing messages
            let (out_tx, out_rx) = bounded(channel_capacity);
            // Channel for party to receive incoming messages
            let (in_tx, in_rx) = bounded(channel_capacity);

            outgoing_rxs.push(out_rx);
            incoming_txs.push(in_tx);
            party_channels.push((out_tx, in_rx));
        }

        let relay = Self {
            outgoing_rxs,
            incoming_txs,
        };

        (relay, party_channels)
    }

    /// Run the relay, forwarding messages between parties.
    /// This should be spawned as a background task.
    pub async fn run_frost_keygen(self) {
        let outgoing_rxs: Vec<_> = self.outgoing_rxs;
        let incoming_txs: Vec<_> = self.incoming_txs;

        // Spawn a task for each party's outgoing channel
        let mut handles = Vec::new();

        for (party_idx, out_rx) in outgoing_rxs.into_iter().enumerate() {
            let incoming_txs = incoming_txs.clone();

            let handle = tokio::spawn(async move {
                while let Ok(msg) = out_rx.recv().await {
                    // Route the message
                    match msg.recipient {
                        Some(recipient) => {
                            // P2P message - send only to recipient
                            if (recipient as usize) < incoming_txs.len() {
                                let _ = incoming_txs[recipient as usize].send(msg).await;
                            }
                        }
                        None => {
                            // Broadcast - send to all parties except sender
                            for (i, tx) in incoming_txs.iter().enumerate() {
                                if i != party_idx {
                                    let _ = tx.send(msg.clone()).await;
                                }
                            }
                        }
                    }
                }
            });

            handles.push(handle);
        }

        // Wait for all relay tasks (they'll end when channels close)
        for handle in handles {
            let _ = handle.await;
        }
    }
}

/// Message relay for FROST signing protocol.
pub struct FrostSigningRelay {
    outgoing_rxs: Vec<Receiver<crate::frost::signing::ProtocolMessage>>,
    incoming_txs: Vec<Sender<crate::frost::signing::ProtocolMessage>>,
}

impl FrostSigningRelay {
    pub fn new(num_parties: u16, channel_capacity: usize) -> (Self, FrostSigningChannels) {
        let mut outgoing_rxs = Vec::new();
        let mut incoming_txs = Vec::new();
        let mut party_channels = Vec::new();

        for _ in 0..num_parties {
            let (out_tx, out_rx) = bounded(channel_capacity);
            let (in_tx, in_rx) = bounded(channel_capacity);

            outgoing_rxs.push(out_rx);
            incoming_txs.push(in_tx);
            party_channels.push((out_tx, in_rx));
        }

        let relay = Self {
            outgoing_rxs,
            incoming_txs,
        };

        (relay, party_channels)
    }

    pub async fn run(self) {
        let outgoing_rxs = self.outgoing_rxs;
        let incoming_txs = self.incoming_txs;

        let mut handles = Vec::new();

        for (party_idx, out_rx) in outgoing_rxs.into_iter().enumerate() {
            let incoming_txs = incoming_txs.clone();

            let handle = tokio::spawn(async move {
                while let Ok(msg) = out_rx.recv().await {
                    match msg.recipient {
                        Some(recipient) => {
                            if (recipient as usize) < incoming_txs.len() {
                                let _ = incoming_txs[recipient as usize].send(msg).await;
                            }
                        }
                        None => {
                            for (i, tx) in incoming_txs.iter().enumerate() {
                                if i != party_idx {
                                    let _ = tx.send(msg.clone()).await;
                                }
                            }
                        }
                    }
                }
            });

            handles.push(handle);
        }

        for handle in handles {
            let _ = handle.await;
        }
    }
}

/// Message relay for CGGMP24 protocols.
pub struct Cggmp24Relay {
    outgoing_rxs: Vec<Receiver<crate::cggmp24::runner::ProtocolMessage>>,
    incoming_txs: Vec<Sender<crate::cggmp24::runner::ProtocolMessage>>,
}

impl Cggmp24Relay {
    pub fn new(num_parties: u16, channel_capacity: usize) -> (Self, Cggmp24Channels) {
        let mut outgoing_rxs = Vec::new();
        let mut incoming_txs = Vec::new();
        let mut party_channels = Vec::new();

        for _ in 0..num_parties {
            let (out_tx, out_rx) = bounded(channel_capacity);
            let (in_tx, in_rx) = bounded(channel_capacity);

            outgoing_rxs.push(out_rx);
            incoming_txs.push(in_tx);
            party_channels.push((out_tx, in_rx));
        }

        let relay = Self {
            outgoing_rxs,
            incoming_txs,
        };

        (relay, party_channels)
    }

    pub async fn run(self) {
        let outgoing_rxs = self.outgoing_rxs;
        let incoming_txs = self.incoming_txs;

        let mut handles = Vec::new();

        for (party_idx, out_rx) in outgoing_rxs.into_iter().enumerate() {
            let incoming_txs = incoming_txs.clone();

            let handle = tokio::spawn(async move {
                while let Ok(msg) = out_rx.recv().await {
                    match msg.recipient {
                        Some(recipient) => {
                            if (recipient as usize) < incoming_txs.len() {
                                let _ = incoming_txs[recipient as usize].send(msg).await;
                            }
                        }
                        None => {
                            for (i, tx) in incoming_txs.iter().enumerate() {
                                if i != party_idx {
                                    let _ = tx.send(msg.clone()).await;
                                }
                            }
                        }
                    }
                }
            });

            handles.push(handle);
        }

        for handle in handles {
            let _ = handle.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_message_relay_creation() {
        let (_, channels) = MessageRelay::new_frost_keygen(3, 100);
        assert_eq!(channels.len(), 3);
    }
}
