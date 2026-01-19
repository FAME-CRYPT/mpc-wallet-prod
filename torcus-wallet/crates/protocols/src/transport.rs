//! Transport Abstraction for MPC Protocol Message Passing
//!
//! This module provides a transport-agnostic interface for sending and receiving
//! MPC protocol messages. The current implementation uses HTTP-based coordinator
//! relay, but the trait allows for future P2P transport implementations.
//!
//! ## Architecture
//!
//! ```text
//! Node Handler
//!     ↓
//! Transport trait (abstract)
//!     ├─ HttpTransport (current: coordinator relay)
//!     └─ P2pTransport (future: direct peer communication)
//!     ↓
//! Protocol Runner (CGGMP24, FROST)
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! // Create transport (currently HTTP-based)
//! let transport = HttpTransport::new("http://coordinator:3000", party_index);
//!
//! // Use via trait
//! transport.send_message(msg).await?;
//! let response = transport.poll_messages(session_id).await?;
//! ```

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::relay::{
    AuxInfoCompleteRequest, PollMessagesResponse, RelayMessage, SubmitMessageRequest,
};

/// Error type for transport operations.
#[derive(Debug, Clone)]
pub enum TransportError {
    /// Failed to connect to the transport backend.
    ConnectionFailed(String),
    /// Message send failed.
    SendFailed(String),
    /// Message receive/poll failed.
    ReceiveFailed(String),
    /// Transport is not connected or session expired.
    NotConnected(String),
    /// Invalid message format.
    InvalidMessage(String),
    /// Timeout waiting for response.
    Timeout(String),
    /// Other transport-specific error.
    Other(String),
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            Self::SendFailed(msg) => write!(f, "Send failed: {}", msg),
            Self::ReceiveFailed(msg) => write!(f, "Receive failed: {}", msg),
            Self::NotConnected(msg) => write!(f, "Not connected: {}", msg),
            Self::InvalidMessage(msg) => write!(f, "Invalid message: {}", msg),
            Self::Timeout(msg) => write!(f, "Timeout: {}", msg),
            Self::Other(msg) => write!(f, "Transport error: {}", msg),
        }
    }
}

impl std::error::Error for TransportError {}

/// Transport configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    /// Transport type identifier.
    pub transport_type: TransportType,
    /// For HTTP transport: coordinator URL.
    pub coordinator_url: Option<String>,
    /// Connection timeout in seconds.
    pub timeout_secs: u64,
    /// For future P2P: list of peer addresses.
    pub peers: Option<Vec<String>>,
    /// For future P2P: listen address.
    pub listen_addr: Option<String>,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            transport_type: TransportType::Http,
            coordinator_url: Some("http://mpc-coordinator:3000".to_string()),
            timeout_secs: 30,
            peers: None,
            listen_addr: None,
        }
    }
}

/// Transport type enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportType {
    /// HTTP-based coordinator relay (current implementation).
    Http,
    /// Direct peer-to-peer communication.
    P2p,
    /// Hybrid: P2P with HTTP fallback.
    Hybrid,
    /// For testing: in-memory message passing.
    InMemory,
}

impl std::fmt::Display for TransportType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http => write!(f, "http"),
            Self::P2p => write!(f, "p2p"),
            Self::Hybrid => write!(f, "hybrid"),
            Self::InMemory => write!(f, "in-memory"),
        }
    }
}

/// Transport trait for MPC protocol message passing.
///
/// This trait abstracts the underlying message transport mechanism,
/// allowing the protocol logic to be independent of whether messages
/// are relayed through a coordinator (HTTP) or sent directly (P2P).
#[async_trait]
pub trait Transport: Send + Sync {
    /// Get the party index for this transport.
    fn party_index(&self) -> u16;

    /// Get the transport type.
    fn transport_type(&self) -> TransportType;

    /// Send a message to the relay/network.
    ///
    /// For HTTP transport, this submits to the coordinator.
    /// For P2P transport, this would send directly to the recipient(s).
    async fn send_message(&self, msg: RelayMessage) -> Result<(), TransportError>;

    /// Poll for pending messages.
    ///
    /// For HTTP transport, this polls the coordinator.
    /// For P2P transport, this would check the local message queue.
    async fn poll_messages(&self, session_id: &str)
        -> Result<PollMessagesResponse, TransportError>;

    /// Notify completion of aux_info generation.
    ///
    /// This is specific to the aux_info protocol but kept in the trait
    /// for simplicity. P2P implementations may broadcast this instead.
    async fn notify_aux_info_complete(
        &self,
        session_id: &str,
        success: bool,
        aux_info_size: usize,
        error: Option<String>,
    ) -> Result<(), TransportError>;
}

/// HTTP-based transport using coordinator relay.
///
/// This is the current production transport that sends messages
/// through the coordinator's relay endpoints.
pub struct HttpTransport {
    client: reqwest::Client,
    coordinator_url: String,
    party_index: u16,
}

impl HttpTransport {
    /// Create a new HTTP transport.
    pub fn new(coordinator_url: &str, party_index: u16) -> Self {
        Self::with_timeout(coordinator_url, party_index, Duration::from_secs(30))
    }

    /// Create a new HTTP transport with custom timeout.
    pub fn with_timeout(coordinator_url: &str, party_index: u16, timeout: Duration) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .expect("Failed to create HTTP client"),
            coordinator_url: coordinator_url.to_string(),
            party_index,
        }
    }

    /// Create from configuration.
    pub fn from_config(config: &TransportConfig, party_index: u16) -> Result<Self, TransportError> {
        let url = config.coordinator_url.as_ref().ok_or_else(|| {
            TransportError::ConnectionFailed("No coordinator URL configured".to_string())
        })?;

        Ok(Self::with_timeout(
            url,
            party_index,
            Duration::from_secs(config.timeout_secs),
        ))
    }

    /// Get the coordinator URL.
    pub fn coordinator_url(&self) -> &str {
        &self.coordinator_url
    }
}

#[async_trait]
impl Transport for HttpTransport {
    fn party_index(&self) -> u16 {
        self.party_index
    }

    fn transport_type(&self) -> TransportType {
        TransportType::Http
    }

    async fn send_message(&self, msg: RelayMessage) -> Result<(), TransportError> {
        let url = format!("{}/relay/submit", self.coordinator_url);
        let request = SubmitMessageRequest { message: msg };

        self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| TransportError::SendFailed(format!("HTTP request failed: {}", e)))?
            .error_for_status()
            .map_err(|e| TransportError::SendFailed(format!("Relay error: {}", e)))?;

        Ok(())
    }

    async fn poll_messages(
        &self,
        session_id: &str,
    ) -> Result<PollMessagesResponse, TransportError> {
        let url = format!(
            "{}/relay/poll/{}/{}",
            self.coordinator_url, session_id, self.party_index
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| TransportError::ReceiveFailed(format!("HTTP request failed: {}", e)))?
            .error_for_status()
            .map_err(|e| TransportError::ReceiveFailed(format!("Relay error: {}", e)))?
            .json::<PollMessagesResponse>()
            .await
            .map_err(|e| {
                TransportError::ReceiveFailed(format!("Failed to parse response: {}", e))
            })?;

        Ok(response)
    }

    async fn notify_aux_info_complete(
        &self,
        session_id: &str,
        success: bool,
        aux_info_size: usize,
        error: Option<String>,
    ) -> Result<(), TransportError> {
        let url = format!("{}/relay/aux-info/complete", self.coordinator_url);
        let request = AuxInfoCompleteRequest {
            session_id: session_id.to_string(),
            party_index: self.party_index,
            success,
            aux_info_size,
            error,
        };

        self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| TransportError::SendFailed(format!("HTTP request failed: {}", e)))?
            .error_for_status()
            .map_err(|e| TransportError::SendFailed(format!("Relay error: {}", e)))?;

        Ok(())
    }
}

/// Type alias for shared transport reference.
pub type SharedTransport = Arc<dyn Transport>;

/// Create a transport from configuration.
///
/// Note: For P2P transport, use `P2pTransport::new()` directly with the
/// incoming queue and spawn the receiver task. This factory only works
/// for HTTP transport.
pub fn create_transport(
    config: &TransportConfig,
    party_index: u16,
) -> Result<SharedTransport, TransportError> {
    match config.transport_type {
        TransportType::Http => {
            let transport = HttpTransport::from_config(config, party_index)?;
            Ok(Arc::new(transport))
        }
        TransportType::P2p => {
            // P2P transport requires an incoming queue and receiver task
            // Use P2pTransport::new() directly
            Err(TransportError::ConnectionFailed(
                "P2P transport requires manual setup with incoming queue. Use P2pTransport::new() directly.".to_string(),
            ))
        }
        TransportType::Hybrid => {
            // Hybrid mode requires both HTTP and P2P setup
            Err(TransportError::ConnectionFailed(
                "Hybrid transport requires manual setup. Create both HttpTransport and P2pTransport.".to_string(),
            ))
        }
        TransportType::InMemory => {
            // Future: In-memory transport for testing
            Err(TransportError::ConnectionFailed(
                "In-memory transport not yet implemented".to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_config_default() {
        let config = TransportConfig::default();
        assert_eq!(config.transport_type, TransportType::Http);
        assert!(config.coordinator_url.is_some());
        assert_eq!(config.timeout_secs, 30);
    }

    #[test]
    fn test_transport_type_display() {
        assert_eq!(TransportType::Http.to_string(), "http");
        assert_eq!(TransportType::P2p.to_string(), "p2p");
        assert_eq!(TransportType::InMemory.to_string(), "in-memory");
    }

    #[test]
    fn test_transport_error_display() {
        let err = TransportError::SendFailed("test error".to_string());
        assert!(err.to_string().contains("Send failed"));
        assert!(err.to_string().contains("test error"));
    }

    #[test]
    fn test_http_transport_creation() {
        let transport = HttpTransport::new("http://localhost:3000", 0);
        assert_eq!(transport.party_index(), 0);
        assert_eq!(transport.coordinator_url(), "http://localhost:3000");
        assert_eq!(transport.transport_type(), TransportType::Http);
    }

    #[test]
    fn test_create_transport_http() {
        let config = TransportConfig::default();
        let result = create_transport(&config, 0);
        assert!(result.is_ok());
        let transport = result.unwrap();
        assert_eq!(transport.transport_type(), TransportType::Http);
    }

    #[test]
    fn test_create_transport_p2p_not_implemented() {
        let config = TransportConfig {
            transport_type: TransportType::P2p,
            ..Default::default()
        };
        let result = create_transport(&config, 0);
        assert!(result.is_err());
    }
}
