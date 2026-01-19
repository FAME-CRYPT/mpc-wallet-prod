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

/// Serialization format for network messages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerializationFormat {
    /// JSON (human-readable, slower, larger)
    Json,
    /// Bincode (binary, faster, smaller)
    Binary,
}

impl NetworkMessage {
    /// Serialize to bytes using specified format
    pub fn to_bytes_with_format(&self, format: SerializationFormat) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        match format {
            SerializationFormat::Json => {
                serde_json::to_vec(self).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
            }
            SerializationFormat::Binary => {
                bincode::serialize(self).map_err(|e| Box::new(*e) as Box<dyn std::error::Error>)
            }
        }
    }

    /// Deserialize from bytes using specified format
    pub fn from_bytes_with_format(bytes: &[u8], format: SerializationFormat) -> Result<Self, Box<dyn std::error::Error>> {
        match format {
            SerializationFormat::Json => {
                serde_json::from_slice(bytes).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
            }
            SerializationFormat::Binary => {
                bincode::deserialize(bytes).map_err(|e| Box::new(*e) as Box<dyn std::error::Error>)
            }
        }
    }

    /// Legacy JSON serialization (backward compatibility)
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// Legacy JSON deserialization (backward compatibility)
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    /// Get serialized size for both formats (for benchmarking)
    pub fn serialized_sizes(&self) -> (usize, usize) {
        let json_size = self.to_bytes_with_format(SerializationFormat::Json)
            .map(|b| b.len())
            .unwrap_or(0);
        let binary_size = self.to_bytes_with_format(SerializationFormat::Binary)
            .map(|b| b.len())
            .unwrap_or(0);
        (json_size, binary_size)
    }
}
