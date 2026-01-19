use async_trait::async_trait;
use libp2p::{
    request_response::{self, Codec, ProtocolSupport},
    StreamProtocol,
};
use serde::{Deserialize, Serialize};
use std::io;
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Direct peer-to-peer request message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DirectRequest {
    /// Request vote status for a transaction
    GetVoteStatus { tx_id: String },
    /// Request peer's public key
    GetPublicKey,
    /// Request node reputation
    GetReputation { node_id: String },
    /// Custom message
    CustomMessage { message: String },
}

/// Direct peer-to-peer response message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DirectResponse {
    /// Vote status response
    VoteStatus {
        tx_id: String,
        voted: bool,
        value: Option<u64>,
    },
    /// Public key response
    PublicKey { key: Vec<u8> },
    /// Reputation response
    Reputation { node_id: String, score: i64 },
    /// Custom message response
    CustomMessage { message: String },
    /// Error response
    Error { message: String },
}

/// Codec for request-response protocol
#[derive(Debug, Clone, Default)]
pub struct DirectMessageCodec;

#[async_trait]
impl Codec for DirectMessageCodec {
    type Protocol = StreamProtocol;
    type Request = DirectRequest;
    type Response = DirectResponse;

    async fn read_request<T>(&mut self, _protocol: &StreamProtocol, io: &mut T) -> io::Result<DirectRequest>
    where
        T: AsyncRead + Unpin + Send,
    {
        let mut buf = Vec::new();
        io.read_to_end(&mut buf).await?;

        serde_json::from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    async fn read_response<T>(&mut self, _protocol: &StreamProtocol, io: &mut T) -> io::Result<DirectResponse>
    where
        T: AsyncRead + Unpin + Send,
    {
        let mut buf = Vec::new();
        io.read_to_end(&mut buf).await?;

        serde_json::from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    async fn write_request<T>(
        &mut self,
        _protocol: &StreamProtocol,
        io: &mut T,
        req: DirectRequest,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let data = serde_json::to_vec(&req)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        io.write_all(&data).await?;
        io.close().await
    }

    async fn write_response<T>(
        &mut self,
        _protocol: &StreamProtocol,
        io: &mut T,
        res: DirectResponse,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let data = serde_json::to_vec(&res)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        io.write_all(&data).await?;
        io.close().await
    }
}

/// Create a request-response behaviour
pub fn create_request_response_behaviour() -> request_response::Behaviour<DirectMessageCodec> {
    let protocols = std::iter::once((
        StreamProtocol::new("/threshold-voting/direct-message/1.0.0"),
        ProtocolSupport::Full,
    ));

    request_response::Behaviour::new(
        protocols,
        request_response::Config::default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let req = DirectRequest::GetVoteStatus {
            tx_id: "tx_001".to_string(),
        };

        let bytes = serde_json::to_vec(&req).unwrap();
        let deserialized: DirectRequest = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(req, deserialized);
    }

    #[test]
    fn test_response_serialization() {
        let res = DirectResponse::VoteStatus {
            tx_id: "tx_001".to_string(),
            voted: true,
            value: Some(42),
        };

        let bytes = serde_json::to_vec(&res).unwrap();
        let deserialized: DirectResponse = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(res, deserialized);
    }
}
