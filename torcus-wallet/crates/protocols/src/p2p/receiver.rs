//! P2P receiver task for handling incoming connections and messages.
//!
//! Spawns a TCP listener and routes incoming messages to session queues.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use super::error::P2pError;
use super::protocol::P2pMessage;
use crate::relay::RelayMessage;

/// Message queue for incoming P2P messages, organized by session.
pub type IncomingQueue = Arc<RwLock<HashMap<String, Vec<RelayMessage>>>>;

/// Spawn the P2P receiver task.
///
/// Listens for incoming TCP connections and routes messages to session queues.
pub fn spawn_receiver(
    listen_addr: SocketAddr,
    party_index: u16,
    incoming_queue: IncomingQueue,
) -> Result<JoinHandle<()>, P2pError> {
    info!(
        "Starting P2P receiver on {} (party {})",
        listen_addr, party_index
    );

    let handle = tokio::spawn(async move {
        match run_receiver(listen_addr, party_index, incoming_queue).await {
            Ok(()) => info!("P2P receiver stopped normally"),
            Err(e) => error!("P2P receiver error: {}", e),
        }
    });

    Ok(handle)
}

/// Run the receiver loop.
async fn run_receiver(
    listen_addr: SocketAddr,
    party_index: u16,
    incoming_queue: IncomingQueue,
) -> Result<(), P2pError> {
    let listener = TcpListener::bind(listen_addr)
        .await
        .map_err(|e| P2pError::BindFailed {
            address: listen_addr.to_string(),
            source: e,
        })?;

    info!("P2P listener bound to {}", listen_addr);

    loop {
        match listener.accept().await {
            Ok((socket, peer_addr)) => {
                debug!("Accepted P2P connection from {}", peer_addr);

                // Disable Nagle's algorithm
                socket.set_nodelay(true).ok();

                // Spawn handler for this connection
                let queue = incoming_queue.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(socket, peer_addr, party_index, queue).await {
                        debug!("Connection from {} ended: {}", peer_addr, e);
                    }
                });
            }
            Err(e) => {
                warn!("Failed to accept connection: {}", e);
                // Continue listening
            }
        }
    }
}

/// Handle a single incoming connection.
async fn handle_connection(
    mut socket: TcpStream,
    peer_addr: SocketAddr,
    _party_index: u16,
    incoming_queue: IncomingQueue,
) -> Result<(), P2pError> {
    debug!("Handling P2P connection from {}", peer_addr);

    loop {
        // Read length prefix
        let mut len_buf = [0u8; 4];
        match socket.read_exact(&mut len_buf).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                debug!("Connection from {} closed", peer_addr);
                return Ok(());
            }
            Err(e) => {
                return Err(P2pError::ReceiveFailed(e));
            }
        }

        let len = u32::from_be_bytes(len_buf);

        // Sanity check on message size
        if len > 16 * 1024 * 1024 {
            return Err(P2pError::CodecError(format!(
                "message too large: {} bytes",
                len
            )));
        }

        // Read payload
        let mut payload = vec![0u8; len as usize];
        socket.read_exact(&mut payload).await?;

        // Decode message
        let msg = match P2pMessage::decode(&payload) {
            Ok(m) => m,
            Err(e) => {
                warn!("Failed to decode message from {}: {}", peer_addr, e);
                continue;
            }
        };

        debug!(
            "Received P2P message: session={}, from={}, round={}, seq={}",
            msg.session_id, msg.sender, msg.round, msg.seq
        );

        // Convert to RelayMessage and queue
        let relay_msg = RelayMessage {
            session_id: msg.session_id.clone(),
            protocol: String::new(), // Protocol type not needed for P2P routing
            sender: msg.sender,
            recipient: msg.recipient,
            round: msg.round,
            payload: msg.payload,
            seq: msg.seq,
            timestamp: msg.timestamp,
        };

        // Add to session queue
        {
            let mut queue = incoming_queue.write().await;
            queue
                .entry(msg.session_id.clone())
                .or_insert_with(Vec::new)
                .push(relay_msg);
        }
    }
}

/// Create a new incoming queue.
pub fn new_incoming_queue() -> IncomingQueue {
    Arc::new(RwLock::new(HashMap::new()))
}

/// Drain messages for a specific session from the queue.
pub async fn drain_session_messages(queue: &IncomingQueue, session_id: &str) -> Vec<RelayMessage> {
    let mut q = queue.write().await;
    q.remove(session_id).unwrap_or_default()
}

/// Check if a session has any pending messages.
#[allow(dead_code)]
pub async fn has_pending_messages(queue: &IncomingQueue, session_id: &str) -> bool {
    let q = queue.read().await;
    q.get(session_id).map(|v| !v.is_empty()).unwrap_or(false)
}

/// Get the number of pending messages for a session.
#[allow(dead_code)]
pub async fn pending_message_count(queue: &IncomingQueue, session_id: &str) -> usize {
    let q = queue.read().await;
    q.get(session_id).map(|v| v.len()).unwrap_or(0)
}

/// Clear all messages for a session.
#[allow(dead_code)]
pub async fn clear_session(queue: &IncomingQueue, session_id: &str) {
    let mut q = queue.write().await;
    q.remove(session_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_incoming_queue_operations() {
        let queue = new_incoming_queue();

        // Initially empty
        assert!(!has_pending_messages(&queue, "session-1").await);
        assert_eq!(pending_message_count(&queue, "session-1").await, 0);

        // Add a message
        {
            let mut q = queue.write().await;
            q.entry("session-1".to_string())
                .or_insert_with(Vec::new)
                .push(RelayMessage {
                    session_id: "session-1".to_string(),
                    protocol: "test".to_string(),
                    sender: 0,
                    recipient: None,
                    round: 1,
                    payload: vec![1, 2, 3],
                    seq: 1,
                    timestamp: 0,
                });
        }

        // Now has messages
        assert!(has_pending_messages(&queue, "session-1").await);
        assert_eq!(pending_message_count(&queue, "session-1").await, 1);

        // Drain messages
        let messages = drain_session_messages(&queue, "session-1").await;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].sender, 0);

        // Empty again
        assert!(!has_pending_messages(&queue, "session-1").await);
    }
}
