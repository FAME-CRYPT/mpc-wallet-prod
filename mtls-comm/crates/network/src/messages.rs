use serde::{Deserialize, Serialize};
use threshold_types::Vote;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMessage {
    Vote(VoteMessage),
    Ping,
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteMessage {
    pub vote: Vote,
}

impl VoteMessage {
    pub fn new(vote: Vote) -> Self {
        Self { vote }
    }
}

impl NetworkMessage {
    /// Serialize to JSON bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// Deserialize from JSON bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    pub fn into_vote(self) -> anyhow::Result<Vote> {
        match self {
            NetworkMessage::Vote(vote_msg) => Ok(vote_msg.vote),
            _ => Err(anyhow::anyhow!("Message is not a Vote")),
        }
    }
}
