//! P2P wire protocol for message framing and encoding.
//!
//! Messages are sent as length-prefixed frames:
//! ```text
//! +----------------+------------------+
//! | length (4 bytes, big-endian)      |
//! +----------------+------------------+
//! | payload (bincode-encoded message) |
//! +-----------------------------------+
//! ```
//!
//! ## Message Types (Phase 4)
//!
//! The P2P protocol supports two types of messages:
//!
//! 1. **Protocol messages** (`P2pMessage`): Round-based MPC protocol messages
//!    for keygen and signing operations.
//!
//! 2. **Control messages** (`SessionControlMessage`): Session coordination
//!    messages for P2P-initiated sessions (proposals, acks, start signals).
//!
//! The `P2pMessageType` enum wraps both types for unified handling.

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::error::P2pError;
use super::session::SessionControlMessage;

/// Maximum message size (16 MB).
const MAX_MESSAGE_SIZE: u32 = 16 * 1024 * 1024;

/// P2P message sent over the wire.
///
/// This mirrors `RelayMessage` but is optimized for P2P transmission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2pMessage {
    /// Session identifier.
    pub session_id: String,
    /// Sender party index.
    pub sender: u16,
    /// Recipient party index (None = broadcast).
    pub recipient: Option<u16>,
    /// Protocol round number.
    pub round: u16,
    /// Serialized protocol message payload.
    pub payload: Vec<u8>,
    /// Sequence number for ordering.
    pub seq: u64,
    /// Unix timestamp in milliseconds.
    pub timestamp: u64,
}

impl P2pMessage {
    /// Create a new P2P message.
    pub fn new(
        session_id: String,
        sender: u16,
        recipient: Option<u16>,
        round: u16,
        payload: Vec<u8>,
        seq: u64,
    ) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Self {
            session_id,
            sender,
            recipient,
            round,
            payload,
            seq,
            timestamp,
        }
    }

    /// Encode message to bytes with length prefix.
    pub fn encode(&self) -> Result<Vec<u8>, P2pError> {
        let payload = bincode::serialize(self).map_err(|e| P2pError::CodecError(e.to_string()))?;

        if payload.len() > MAX_MESSAGE_SIZE as usize {
            return Err(P2pError::CodecError(format!(
                "message too large: {} bytes (max {})",
                payload.len(),
                MAX_MESSAGE_SIZE
            )));
        }

        let mut buf = Vec::with_capacity(4 + payload.len());
        buf.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        buf.extend_from_slice(&payload);
        Ok(buf)
    }

    /// Decode message from bytes (without length prefix).
    pub fn decode(bytes: &[u8]) -> Result<Self, P2pError> {
        bincode::deserialize(bytes).map_err(|e| P2pError::CodecError(e.to_string()))
    }

    /// Write message to an async writer.
    pub async fn write_to<W: AsyncWrite + Unpin>(&self, writer: &mut W) -> Result<(), P2pError> {
        let encoded = self.encode()?;
        writer
            .write_all(&encoded)
            .await
            .map_err(|e| P2pError::SendFailed {
                party_index: self.recipient.unwrap_or(0),
                source: e,
            })?;
        writer.flush().await.map_err(|e| P2pError::SendFailed {
            party_index: self.recipient.unwrap_or(0),
            source: e,
        })?;
        Ok(())
    }

    /// Read message from an async reader.
    pub async fn read_from<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Self, P2pError> {
        // Read length prefix
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf);

        if len > MAX_MESSAGE_SIZE {
            return Err(P2pError::CodecError(format!(
                "message too large: {} bytes (max {})",
                len, MAX_MESSAGE_SIZE
            )));
        }

        // Read payload
        let mut payload = vec![0u8; len as usize];
        reader.read_exact(&mut payload).await?;

        Self::decode(&payload)
    }

    /// Write message to a sync writer (for testing).
    #[allow(dead_code)]
    pub fn write_to_sync<W: Write>(&self, writer: &mut W) -> Result<(), P2pError> {
        let encoded = self.encode()?;
        writer.write_all(&encoded)?;
        writer.flush()?;
        Ok(())
    }

    /// Read message from a sync reader (for testing).
    #[allow(dead_code)]
    pub fn read_from_sync<R: Read>(reader: &mut R) -> Result<Self, P2pError> {
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)?;
        let len = u32::from_be_bytes(len_buf);

        if len > MAX_MESSAGE_SIZE {
            return Err(P2pError::CodecError(format!(
                "message too large: {} bytes (max {})",
                len, MAX_MESSAGE_SIZE
            )));
        }

        let mut payload = vec![0u8; len as usize];
        reader.read_exact(&mut payload)?;

        Self::decode(&payload)
    }
}

impl From<crate::relay::RelayMessage> for P2pMessage {
    fn from(msg: crate::relay::RelayMessage) -> Self {
        Self {
            session_id: msg.session_id,
            sender: msg.sender,
            recipient: msg.recipient,
            round: msg.round,
            payload: msg.payload,
            seq: msg.seq,
            timestamp: msg.timestamp,
        }
    }
}

impl From<P2pMessage> for crate::relay::RelayMessage {
    fn from(msg: P2pMessage) -> Self {
        Self {
            session_id: msg.session_id,
            protocol: String::new(), // Will be filled by caller if needed
            sender: msg.sender,
            recipient: msg.recipient,
            round: msg.round,
            payload: msg.payload,
            seq: msg.seq,
            timestamp: msg.timestamp,
        }
    }
}

// ============================================================================
// P2P Message Type Enum (Phase 4)
// ============================================================================

/// Unified P2P message type for both protocol and control messages.
///
/// This enum allows a single message handling path for all P2P communication,
/// distinguishing between MPC protocol messages and session control messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum P2pMessageType {
    /// MPC protocol message (keygen, signing rounds).
    Protocol(P2pMessage),
    /// Session control message (proposal, ack, start, abort).
    Control(SessionControlMessage),
}

impl P2pMessageType {
    /// Encode message to bytes with length prefix.
    pub fn encode(&self) -> Result<Vec<u8>, P2pError> {
        let payload = bincode::serialize(self).map_err(|e| P2pError::CodecError(e.to_string()))?;

        if payload.len() > MAX_MESSAGE_SIZE as usize {
            return Err(P2pError::CodecError(format!(
                "message too large: {} bytes (max {})",
                payload.len(),
                MAX_MESSAGE_SIZE
            )));
        }

        let mut buf = Vec::with_capacity(4 + payload.len());
        buf.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        buf.extend_from_slice(&payload);
        Ok(buf)
    }

    /// Decode message from bytes (without length prefix).
    pub fn decode(bytes: &[u8]) -> Result<Self, P2pError> {
        bincode::deserialize(bytes).map_err(|e| P2pError::CodecError(e.to_string()))
    }

    /// Write message to an async writer.
    pub async fn write_to<W: AsyncWrite + Unpin>(&self, writer: &mut W) -> Result<(), P2pError> {
        let encoded = self.encode()?;
        writer
            .write_all(&encoded)
            .await
            .map_err(|e| P2pError::SendFailed {
                party_index: 0,
                source: e,
            })?;
        writer.flush().await.map_err(|e| P2pError::SendFailed {
            party_index: 0,
            source: e,
        })?;
        Ok(())
    }

    /// Read message from an async reader.
    pub async fn read_from<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Self, P2pError> {
        // Read length prefix
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf);

        if len > MAX_MESSAGE_SIZE {
            return Err(P2pError::CodecError(format!(
                "message too large: {} bytes (max {})",
                len, MAX_MESSAGE_SIZE
            )));
        }

        // Read payload
        let mut payload = vec![0u8; len as usize];
        reader.read_exact(&mut payload).await?;

        Self::decode(&payload)
    }

    /// Get session ID if available.
    pub fn session_id(&self) -> Option<String> {
        match self {
            P2pMessageType::Protocol(msg) => Some(msg.session_id.clone()),
            P2pMessageType::Control(ctrl) => ctrl.session_id(),
        }
    }

    /// Check if this is a protocol message.
    pub fn is_protocol(&self) -> bool {
        matches!(self, P2pMessageType::Protocol(_))
    }

    /// Check if this is a control message.
    pub fn is_control(&self) -> bool {
        matches!(self, P2pMessageType::Control(_))
    }
}

impl From<P2pMessage> for P2pMessageType {
    fn from(msg: P2pMessage) -> Self {
        P2pMessageType::Protocol(msg)
    }
}

impl From<SessionControlMessage> for P2pMessageType {
    fn from(msg: SessionControlMessage) -> Self {
        P2pMessageType::Control(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_encode_decode() {
        let msg = P2pMessage::new(
            "test-session".to_string(),
            0,
            Some(1),
            1,
            vec![1, 2, 3, 4],
            42,
        );

        let encoded = msg.encode().unwrap();
        assert!(encoded.len() > 4); // At least length prefix

        // Skip length prefix for decode
        let decoded = P2pMessage::decode(&encoded[4..]).unwrap();
        assert_eq!(decoded.session_id, "test-session");
        assert_eq!(decoded.sender, 0);
        assert_eq!(decoded.recipient, Some(1));
        assert_eq!(decoded.round, 1);
        assert_eq!(decoded.payload, vec![1, 2, 3, 4]);
        assert_eq!(decoded.seq, 42);
    }

    #[test]
    fn test_sync_io() {
        let msg = P2pMessage::new(
            "test-session".to_string(),
            0,
            None,
            2,
            vec![5, 6, 7, 8],
            100,
        );

        let mut buf = Vec::new();
        msg.write_to_sync(&mut buf).unwrap();

        let mut cursor = Cursor::new(buf);
        let decoded = P2pMessage::read_from_sync(&mut cursor).unwrap();

        assert_eq!(decoded.session_id, "test-session");
        assert_eq!(decoded.sender, 0);
        assert_eq!(decoded.recipient, None);
        assert_eq!(decoded.round, 2);
        assert_eq!(decoded.payload, vec![5, 6, 7, 8]);
        assert_eq!(decoded.seq, 100);
    }

    #[test]
    fn test_relay_message_conversion() {
        let relay_msg = crate::relay::RelayMessage {
            session_id: "session-123".to_string(),
            protocol: "cggmp24".to_string(),
            sender: 1,
            recipient: Some(2),
            round: 3,
            payload: vec![10, 20, 30],
            seq: 5,
            timestamp: 1234567890,
        };

        let p2p_msg: P2pMessage = relay_msg.clone().into();
        assert_eq!(p2p_msg.session_id, relay_msg.session_id);
        assert_eq!(p2p_msg.sender, relay_msg.sender);
        assert_eq!(p2p_msg.recipient, relay_msg.recipient);

        let back: crate::relay::RelayMessage = p2p_msg.into();
        assert_eq!(back.session_id, relay_msg.session_id);
        assert_eq!(back.sender, relay_msg.sender);
    }
}
