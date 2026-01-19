//! HTTP-based Message Relay for MPC Protocols
//!
//! This module provides a simple HTTP-based message relay system that works
//! well with Docker networking. Instead of complex libp2p peer discovery,
//! nodes communicate through the coordinator which acts as a message relay.
//!
//! This is simpler and more reliable for container environments than libp2p.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::transport::Transport;

/// Protocol message for relay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayMessage {
    /// Unique session identifier
    pub session_id: String,
    /// Protocol type (aux_info, keygen, signing)
    pub protocol: String,
    /// Sender party index
    pub sender: u16,
    /// Recipient party index (None for broadcast)
    pub recipient: Option<u16>,
    /// Protocol round number
    pub round: u16,
    /// Serialized message payload
    pub payload: Vec<u8>,
    /// Message sequence number
    pub seq: u64,
    /// Timestamp (unix millis)
    pub timestamp: u64,
}

/// Message queue for a protocol session
#[derive(Debug, Default)]
pub struct SessionMessageQueue {
    /// Messages waiting to be delivered to each party
    queues: HashMap<u16, Vec<RelayMessage>>,
    /// All broadcast messages
    broadcasts: Vec<RelayMessage>,
}

impl SessionMessageQueue {
    pub fn new(num_parties: u16) -> Self {
        let mut queues = HashMap::new();
        for i in 0..num_parties {
            queues.insert(i, Vec::new());
        }
        Self {
            queues,
            broadcasts: Vec::new(),
        }
    }

    /// Add a message to the queue
    pub fn push(&mut self, msg: RelayMessage) {
        if let Some(recipient) = msg.recipient {
            // P2P message
            if let Some(queue) = self.queues.get_mut(&recipient) {
                queue.push(msg);
            }
        } else {
            // Broadcast message - add to all queues except sender
            let sender = msg.sender;
            self.broadcasts.push(msg.clone());
            for (party, queue) in self.queues.iter_mut() {
                if *party != sender {
                    queue.push(msg.clone());
                }
            }
        }
    }

    /// Get pending messages for a party
    pub fn pop_for_party(&mut self, party: u16) -> Vec<RelayMessage> {
        self.queues
            .get_mut(&party)
            .map(std::mem::take)
            .unwrap_or_default()
    }

    /// Check if all parties have submitted messages for a round
    pub fn round_complete(&self, round: u16, num_parties: u16) -> bool {
        let round_msgs: Vec<_> = self
            .broadcasts
            .iter()
            .filter(|m| m.round == round)
            .collect();
        round_msgs.len() >= num_parties as usize
    }
}

/// Request to submit a protocol message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitMessageRequest {
    pub message: RelayMessage,
}

/// Response with pending messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollMessagesResponse {
    pub messages: Vec<RelayMessage>,
    pub session_active: bool,
}

/// Protocol session state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolSession {
    pub session_id: String,
    pub protocol: String,
    pub num_parties: u16,
    pub threshold: u16,
    pub started_at: u64,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SessionStatus {
    WaitingForParties,
    Running,
    Completed,
    Failed(String),
}

/// Start aux_info generation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartAuxInfoGenRequest {
    pub session_id: String,
}

/// Start aux_info generation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartAuxInfoGenResponse {
    pub success: bool,
    pub session_id: String,
    pub party_index: u16,
    pub message: String,
}

/// Aux info generation completion notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxInfoCompleteRequest {
    pub session_id: String,
    pub party_index: u16,
    pub success: bool,
    pub aux_info_size: usize,
    pub error: Option<String>,
}

/// HTTP client for communicating with coordinator relay.
///
/// This is a convenience wrapper around `HttpTransport` that maintains
/// backward compatibility with existing code. New code should prefer
/// using the `Transport` trait directly for better abstraction.
///
/// # Example
///
/// ```ignore
/// // Legacy usage (still supported)
/// let client = RelayClient::new("http://coordinator:3000", 0);
/// client.submit_message(msg).await?;
///
/// // Preferred: use Transport trait
/// use protocols::{HttpTransport, Transport};
/// let transport = HttpTransport::new("http://coordinator:3000", 0);
/// transport.send_message(msg).await?;
/// ```
pub struct RelayClient {
    inner: crate::transport::HttpTransport,
}

impl RelayClient {
    pub fn new(coordinator_url: &str, party_index: u16) -> Self {
        Self {
            inner: crate::transport::HttpTransport::new(coordinator_url, party_index),
        }
    }

    /// Get the party index
    pub fn party_index(&self) -> u16 {
        self.inner.party_index()
    }

    /// Get the coordinator URL
    pub fn coordinator_url(&self) -> &str {
        self.inner.coordinator_url()
    }

    /// Submit a message to the relay
    pub async fn submit_message(&self, msg: RelayMessage) -> Result<(), String> {
        self.inner
            .send_message(msg)
            .await
            .map_err(|e| e.to_string())
    }

    /// Poll for pending messages
    pub async fn poll_messages(&self, session_id: &str) -> Result<PollMessagesResponse, String> {
        self.inner
            .poll_messages(session_id)
            .await
            .map_err(|e| e.to_string())
    }

    /// Notify completion of aux_info generation
    pub async fn notify_aux_info_complete(
        &self,
        session_id: &str,
        success: bool,
        aux_info_size: usize,
        error: Option<String>,
    ) -> Result<(), String> {
        self.inner
            .notify_aux_info_complete(session_id, success, aux_info_size, error)
            .await
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_message_queue() {
        let mut queue = SessionMessageQueue::new(3);

        // Add a broadcast message
        let broadcast = RelayMessage {
            session_id: "test".to_string(),
            protocol: "aux_info".to_string(),
            sender: 0,
            recipient: None,
            round: 1,
            payload: vec![1, 2, 3],
            seq: 1,
            timestamp: 0,
        };
        queue.push(broadcast);

        // Party 0 (sender) should not receive it
        let msgs = queue.pop_for_party(0);
        assert!(msgs.is_empty());

        // Party 1 should receive it
        let msgs = queue.pop_for_party(1);
        assert_eq!(msgs.len(), 1);

        // Party 2 should receive it
        let msgs = queue.pop_for_party(2);
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn test_p2p_message() {
        let mut queue = SessionMessageQueue::new(3);

        // Add a P2P message from 0 to 1
        let p2p = RelayMessage {
            session_id: "test".to_string(),
            protocol: "aux_info".to_string(),
            sender: 0,
            recipient: Some(1),
            round: 1,
            payload: vec![1, 2, 3],
            seq: 1,
            timestamp: 0,
        };
        queue.push(p2p);

        // Only party 1 should receive it
        assert!(queue.pop_for_party(0).is_empty());
        assert_eq!(queue.pop_for_party(1).len(), 1);
        assert!(queue.pop_for_party(2).is_empty());
    }
}
