// HTTP-based transport adapter for cggmp24
// Bridges between cggmp24's Stream/Sink interface and our HTTP MessageBoard

use crate::message_board_client::MessageBoardClient;
use anyhow::Result;
use futures::{Sink, Stream};
use log::{debug, error};
use round_based::{Incoming, MessageDestination, MessageType, Outgoing};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

/// Stream for incoming messages from HTTP polling
pub struct IncomingStream<Msg> {
    rx: mpsc::UnboundedReceiver<Incoming<Msg>>,
}

/// Sink for outgoing messages to HTTP posting
pub struct OutgoingSink<Msg> {
    tx: mpsc::UnboundedSender<Outgoing<Msg>>,
}

/// Create a new HTTP transport (returns a tuple of Stream and Sink)
///
/// Spawns background tasks for:
/// - Polling MessageBoard for incoming messages
/// - Posting outgoing messages to MessageBoard
///
/// Returns (incoming_stream, outgoing_sink) which together implement Delivery
pub fn http_transport<Msg>(
    client: MessageBoardClient,
    request_id: String,
    _my_index: u16,
) -> (IncomingStream<Msg>, OutgoingSink<Msg>)
where
    Msg: Serialize + for<'de> Deserialize<'de> + Send + 'static,
{
    let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
    let (outgoing_tx, outgoing_rx) = mpsc::unbounded_channel();

    // Spawn task to poll for incoming messages
    let poll_client = client.clone();
    let poll_request_id = request_id.clone();
    tokio::spawn(async move {
        if let Err(e) = poll_messages(poll_client, poll_request_id, incoming_tx).await {
            error!("Polling task failed: {}", e);
        }
    });

    // Spawn task to post outgoing messages
    let post_client = client;
    let post_request_id = request_id;
    tokio::spawn(async move {
        if let Err(e) = post_messages(post_client, post_request_id, outgoing_rx).await {
            error!("Posting task failed: {}", e);
        }
    });

    (
        IncomingStream { rx: incoming_rx },
        OutgoingSink { tx: outgoing_tx },
    )
}

/// Create a new HTTP transport for presignature generation (uses separate endpoints)
///
/// This is identical to http_transport but uses presignature-specific endpoints
/// to avoid conflicts with regular signing requests
pub fn http_transport_presignature<Msg>(
    client: MessageBoardClient,
    request_id: String,
    _my_index: u16,
) -> (IncomingStream<Msg>, OutgoingSink<Msg>)
where
    Msg: Serialize + for<'de> Deserialize<'de> + Send + 'static,
{
    let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
    let (outgoing_tx, outgoing_rx) = mpsc::unbounded_channel();

    // Spawn task to poll for incoming presignature messages
    let poll_client = client.clone();
    let poll_request_id = request_id.clone();
    tokio::spawn(async move {
        if let Err(e) = poll_presignature_messages(poll_client, poll_request_id, incoming_tx).await
        {
            error!("Presignature polling task failed: {}", e);
        }
    });

    // Spawn task to post outgoing presignature messages
    let post_client = client;
    let post_request_id = request_id;
    tokio::spawn(async move {
        if let Err(e) = post_presignature_messages(post_client, post_request_id, outgoing_rx).await
        {
            error!("Presignature posting task failed: {}", e);
        }
    });

    (
        IncomingStream { rx: incoming_rx },
        OutgoingSink { tx: outgoing_tx },
    )
}

/// Background task that polls MessageBoard for incoming messages
async fn poll_messages<Msg>(
    client: MessageBoardClient,
    request_id: String,
    tx: mpsc::UnboundedSender<Incoming<Msg>>,
) -> Result<()>
where
    Msg: for<'de> Deserialize<'de>,
{
    let mut seen_message_ids = HashSet::new();
    let poll_interval = Duration::from_millis(500);
    let mut msg_id_counter: u64 = 0;

    loop {
        // Fetch messages from MessageBoard
        match client.get_messages(&request_id).await {
            Ok(messages) => {
                for msg in messages {
                    // Skip duplicates
                    if let Some(id) = &msg.id {
                        if seen_message_ids.contains(id) {
                            continue;
                        }
                        seen_message_ids.insert(id.clone());
                    }

                    // Parse sender index from node ID (e.g., "node-1" -> 0)
                    let sender = match parse_node_index(&msg.from_node) {
                        Ok(idx) => idx,
                        Err(e) => {
                            error!("Invalid sender node ID {}: {}", msg.from_node, e);
                            continue;
                        }
                    };

                    // Deserialize protocol message
                    let protocol_msg: Msg = match serde_json::from_str(&msg.payload) {
                        Ok(m) => m,
                        Err(e) => {
                            error!("Failed to deserialize message: {}", e);
                            continue;
                        }
                    };

                    // Determine message type (broadcast or p2p)
                    let msg_type = if msg.to_node == "*" {
                        MessageType::Broadcast
                    } else {
                        MessageType::P2P
                    };

                    // Create Incoming message with proper structure
                    let incoming = Incoming {
                        id: msg_id_counter,
                        sender,
                        msg_type,
                        msg: protocol_msg,
                    };
                    msg_id_counter += 1;

                    if tx.send(incoming).is_err() {
                        // Receiver dropped, stop polling
                        debug!("Receiver dropped, stopping poll task");
                        return Ok(());
                    }

                    debug!("Received message from node-{}", sender + 1);
                }
            }
            Err(e) => {
                error!("Failed to poll messages: {}", e);
            }
        }

        sleep(poll_interval).await;
    }
}

/// Background task that posts outgoing messages to MessageBoard
async fn post_messages<Msg>(
    client: MessageBoardClient,
    request_id: String,
    mut rx: mpsc::UnboundedReceiver<Outgoing<Msg>>,
) -> Result<()>
where
    Msg: Serialize,
{
    while let Some(outgoing) = rx.recv().await {
        // Serialize message payload
        let payload = match serde_json::to_string(&outgoing.msg) {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to serialize message: {}", e);
                continue;
            }
        };

        // Determine recipient
        // MessageDestination is an enum: AllParties (broadcast) or OneParty(idx) (p2p)
        let to_node = match outgoing.recipient {
            MessageDestination::AllParties => "*".to_string(),
            MessageDestination::OneParty(idx) => format_node_id(idx),
        };

        // Post to MessageBoard (round is always 0 for now, could be extracted from message)
        match client
            .post_message(&request_id, &to_node, 0, &payload)
            .await
        {
            Ok(_) => {
                debug!("Posted message to {}", to_node);
            }
            Err(e) => {
                error!("Failed to post message to {}: {}", to_node, e);
            }
        }
    }

    Ok(())
}

/// Parse node index from node ID string
/// "node-1" -> 0, "node-2" -> 1, etc.
fn parse_node_index(node_id: &str) -> Result<u16> {
    let parts: Vec<&str> = node_id.split('-').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid node ID format: {}", node_id);
    }

    let num: u16 = parts[1].parse()?;
    // Convert 1-based to 0-based indexing
    if num == 0 {
        anyhow::bail!("Node ID must be 1-based");
    }
    Ok(num - 1)
}

/// Format node index as node ID string
/// 0 -> "node-1", 1 -> "node-2", etc.
fn format_node_id(index: u16) -> String {
    format!("node-{}", index + 1)
}

/// Background task that polls MessageBoard for incoming presignature messages
/// This is identical to poll_messages but uses the presignature-specific endpoint
async fn poll_presignature_messages<Msg>(
    client: MessageBoardClient,
    request_id: String,
    tx: mpsc::UnboundedSender<Incoming<Msg>>,
) -> Result<()>
where
    Msg: for<'de> Deserialize<'de>,
{
    let mut seen_message_ids = HashSet::new();
    let poll_interval = Duration::from_millis(500);
    let mut msg_id_counter: u64 = 0;

    loop {
        match client.get_presignature_messages(&request_id).await {
            Ok(messages) => {
                for msg in messages {
                    let msg_id = msg.id.clone().unwrap_or_default();
                    if seen_message_ids.contains(&msg_id) {
                        continue;
                    }
                    seen_message_ids.insert(msg_id.clone());

                    let sender = match parse_node_index(&msg.from_node) {
                        Ok(idx) => idx,
                        Err(e) => {
                            error!("Failed to parse sender node ID: {}", e);
                            continue;
                        }
                    };

                    let protocol_msg: Msg = match serde_json::from_str(&msg.payload) {
                        Ok(m) => m,
                        Err(e) => {
                            error!("Failed to deserialize presignature message: {}", e);
                            continue;
                        }
                    };

                    let msg_type = if msg.to_node == "*" {
                        MessageType::Broadcast
                    } else {
                        MessageType::P2P
                    };

                    let incoming = Incoming {
                        id: msg_id_counter,
                        sender,
                        msg_type,
                        msg: protocol_msg,
                    };
                    msg_id_counter += 1;

                    if tx.send(incoming).is_err() {
                        debug!("Receiver dropped, stopping presignature poll task");
                        return Ok(());
                    }

                    debug!("Received presignature message from node-{}", sender + 1);
                }
            }
            Err(e) => {
                error!("Failed to poll presignature messages: {}", e);
            }
        }

        sleep(poll_interval).await;
    }
}

/// Background task that posts outgoing presignature messages to MessageBoard
/// This is identical to post_messages but uses the presignature-specific endpoint
async fn post_presignature_messages<Msg>(
    client: MessageBoardClient,
    request_id: String,
    mut rx: mpsc::UnboundedReceiver<Outgoing<Msg>>,
) -> Result<()>
where
    Msg: Serialize,
{
    while let Some(outgoing) = rx.recv().await {
        let payload = match serde_json::to_string(&outgoing.msg) {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to serialize presignature message: {}", e);
                continue;
            }
        };

        let to_node = match outgoing.recipient {
            MessageDestination::AllParties => "*".to_string(),
            MessageDestination::OneParty(idx) => format_node_id(idx),
        };

        match client
            .post_presignature_message(&request_id, &to_node, 0, &payload)
            .await
        {
            Ok(_) => {
                debug!("Posted presignature message to {}", to_node);
            }
            Err(e) => {
                error!("Failed to post presignature message to {}: {}", to_node, e);
            }
        }
    }

    Ok(())
}

// Implement Stream for IncomingStream
impl<Msg> Stream for IncomingStream<Msg>
where
    Msg: Unpin,
{
    type Item = Result<Incoming<Msg>, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Poll the channel from the background polling task
        match self.rx.poll_recv(cx) {
            Poll::Ready(Some(msg)) => Poll::Ready(Some(Ok(msg))),
            Poll::Ready(None) => Poll::Ready(None), // Channel closed
            Poll::Pending => Poll::Pending,
        }
    }
}

// Implement Sink for OutgoingSink
impl<Msg> Sink<Outgoing<Msg>> for OutgoingSink<Msg>
where
    Msg: Unpin,
{
    type Error = std::io::Error;

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Channel is always ready (unbounded)
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: Outgoing<Msg>) -> Result<(), Self::Error> {
        // Send to background posting task
        self.tx.send(item).map_err(|_e| {
            std::io::Error::new(std::io::ErrorKind::BrokenPipe, "Posting task stopped")
        })
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Messages are buffered in the channel, nothing to flush
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Dropping the sender will close the channel
        Poll::Ready(Ok(()))
    }
}
