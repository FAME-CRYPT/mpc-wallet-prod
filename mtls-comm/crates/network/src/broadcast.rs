use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::messages::NetworkMessage;
use crate::mtls_node::MtlsNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MessageId(Uuid);

impl MessageId {
    pub fn from_message(message: &NetworkMessage) -> Self {
        // Generate deterministic ID from message content
        // For votes, use tx_id + node_id
        // For ping/pong, generate random UUID
        match message {
            NetworkMessage::Vote(vote_msg) => {
                // Create deterministic UUID from tx_id + node_id
                let content = format!("{}:{}", vote_msg.vote.tx_id, vote_msg.vote.node_id.0);
                let uuid = Uuid::new_v5(&Uuid::NAMESPACE_OID, content.as_bytes());
                MessageId(uuid)
            }
            _ => MessageId(Uuid::new_v4()),
        }
    }
}

pub struct BroadcastManager {
    seen_messages: Arc<RwLock<HashSet<MessageId>>>,
}

impl BroadcastManager {
    pub fn new() -> Self {
        Self {
            seen_messages: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    pub async fn broadcast(&self, node: &MtlsNode, message: NetworkMessage) -> anyhow::Result<()> {
        // Generate message ID
        let msg_id = MessageId::from_message(&message);

        // Check if already seen (avoid duplicates)
        {
            let mut seen = self.seen_messages.write().await;
            if seen.contains(&msg_id) {
                return Ok(()); // Already broadcasted
            }
            seen.insert(msg_id);
        }

        // Send to all peers (replaces GossipSub)
        match message {
            NetworkMessage::Vote(vote_msg) => {
                node.broadcast_vote(vote_msg.vote).await?;
            }
            _ => {
                // Other message types not broadcasted
            }
        }

        Ok(())
    }
}

impl Default for BroadcastManager {
    fn default() -> Self {
        Self::new()
    }
}
