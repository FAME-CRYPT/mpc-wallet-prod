use crate::behavior::{ThresholdBehavior, VOTE_TOPIC};
use crate::messages::{NetworkMessage, VoteMessage};
use crate::request_response::{DirectRequest, DirectResponse};
use futures::StreamExt;
use libp2p::{
    identity::Keypair,
    noise,
    request_response::{Event as RequestResponseEvent, Message, OutboundRequestId, ResponseChannel},
    swarm::SwarmEvent,
    tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder,
};
use threshold_types::{NodeId, Result, Vote, VotingError};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

pub struct P2PNode {
    swarm: Swarm<ThresholdBehavior>,
    #[allow(dead_code)]
    node_id: NodeId,
    #[allow(dead_code)]
    vote_tx: mpsc::UnboundedSender<Vote>,
}

impl P2PNode {
    pub fn new(
        node_id: NodeId,
        keypair: Keypair,
        vote_tx: mpsc::UnboundedSender<Vote>,
    ) -> Result<Self> {
        let local_peer_id = PeerId::from(keypair.public());
        let public_key = keypair.public();
        info!("Local peer id: {}", local_peer_id);

        let swarm = SwarmBuilder::with_existing_identity(keypair.clone())
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )
            .map_err(|e| VotingError::NetworkError(format!("Failed to configure TCP: {}", e)))?
            .with_behaviour(|_| ThresholdBehavior::new(local_peer_id, public_key, keypair))
            .map_err(|e| VotingError::NetworkError(format!("Failed to create behaviour: {}", e)))?
            .build();

        info!("P2P node initialized for node_id={}", node_id);

        let mut node = Self {
            swarm,
            node_id,
            vote_tx,
        };

        node.swarm
            .behaviour_mut()
            .subscribe_to_topic(VOTE_TOPIC)
            .map_err(|e| VotingError::NetworkError(format!("Failed to subscribe: {}", e)))?;

        Ok(node)
    }

    pub fn listen_on(&mut self, addr: Multiaddr) -> Result<()> {
        self.swarm
            .listen_on(addr.clone())
            .map_err(|e| VotingError::NetworkError(format!("Failed to listen: {}", e)))?;

        info!("Listening on {}", addr);

        Ok(())
    }

    pub fn dial(&mut self, addr: Multiaddr) -> Result<()> {
        self.swarm
            .dial(addr.clone())
            .map_err(|e| VotingError::NetworkError(format!("Failed to dial: {}", e)))?;

        info!("Dialing {}", addr);

        Ok(())
    }

    pub fn broadcast_vote(&mut self, vote: Vote) -> Result<()> {
        let message = NetworkMessage::Vote(VoteMessage::new(vote));
        let data = message
            .to_bytes()
            .map_err(|e| VotingError::NetworkError(format!("Failed to serialize: {}", e)))?;

        self.swarm
            .behaviour_mut()
            .publish_message(VOTE_TOPIC, data)
            .map_err(|e| VotingError::NetworkError(format!("Failed to broadcast: {}", e)))?;

        Ok(())
    }

    /// Send a direct request to a specific peer
    pub fn send_request(&mut self, peer_id: PeerId, request: DirectRequest) -> Result<OutboundRequestId> {
        info!("Sending direct request to peer {}: {:?}", peer_id, request);

        let request_id = self.swarm
            .behaviour_mut()
            .request_response
            .send_request(&peer_id, request);

        Ok(request_id)
    }

    /// Send a response to a request
    pub fn send_response(&mut self, channel: ResponseChannel<DirectResponse>, response: DirectResponse) -> Result<()> {
        info!("Sending response: {:?}", response);

        self.swarm
            .behaviour_mut()
            .request_response
            .send_response(channel, response)
            .map_err(|_| VotingError::NetworkError("Failed to send response".to_string()))?;

        Ok(())
    }

    /// Get list of connected peers
    pub fn connected_peers(&self) -> Vec<PeerId> {
        self.swarm.connected_peers().cloned().collect()
    }

    pub async fn run(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                event = self.swarm.select_next_some() => {
                    match event {
                        SwarmEvent::Behaviour(event) => {
                            self.handle_behavior_event(event).await;
                        }
                        SwarmEvent::NewListenAddr { address, .. } => {
                            info!("Listening on {}", address);
                        }
                        SwarmEvent::ConnectionEstablished {
                            peer_id, endpoint, ..
                        } => {
                            info!("Connection established with peer: {} at {}", peer_id, endpoint.get_remote_address());
                        }
                        SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                            info!("Connection closed with peer: {} cause: {:?}", peer_id, cause);
                        }
                        SwarmEvent::IncomingConnection { .. } => {}
                        SwarmEvent::IncomingConnectionError { error, .. } => {
                            warn!("Incoming connection error: {}", error);
                        }
                        SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                            warn!("Outgoing connection error to {:?}: {}", peer_id, error);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Run the P2P node with an optional delayed vote broadcast (for testing)
    pub async fn run_with_delayed_broadcast(
        &mut self,
        test_vote: Option<threshold_types::Vote>,
        delay_secs: u64,
    ) -> Result<()> {
        use tokio::time::{sleep, Duration};

        // Setup delayed broadcast timer if test vote provided
        let mut broadcast_timer = if test_vote.is_some() {
            info!("⏳ Scheduled test vote broadcast in {} seconds", delay_secs);
            Some(Box::pin(sleep(Duration::from_secs(delay_secs))))
        } else {
            None
        };

        let mut test_vote_to_send = test_vote;

        loop {
            tokio::select! {
                // Handle swarm events
                event = self.swarm.select_next_some() => {
                    match event {
                        SwarmEvent::Behaviour(event) => {
                            self.handle_behavior_event(event).await;
                        }
                        SwarmEvent::NewListenAddr { address, .. } => {
                            info!("Listening on {}", address);
                        }
                        SwarmEvent::ConnectionEstablished {
                            peer_id, endpoint, ..
                        } => {
                            info!("Connection established with peer: {} at {}", peer_id, endpoint.get_remote_address());
                        }
                        SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                            info!("Connection closed with peer: {} cause: {:?}", peer_id, cause);
                        }
                        SwarmEvent::IncomingConnection { .. } => {}
                        SwarmEvent::IncomingConnectionError { error, .. } => {
                            warn!("Incoming connection error: {}", error);
                        }
                        SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                            warn!("Outgoing connection error to {:?}: {}", peer_id, error);
                        }
                        _ => {}
                    }
                }

                // Handle delayed broadcast
                _ = async {
                    match &mut broadcast_timer {
                        Some(timer) => timer.await,
                        None => std::future::pending().await,
                    }
                }, if broadcast_timer.is_some() => {
                    if let Some(vote) = test_vote_to_send.take() {
                        info!("🧪 Broadcasting test vote: tx_id={} value={}", vote.tx_id, vote.value);

                        match self.broadcast_vote(vote) {
                            Ok(_) => info!("✅ Test vote broadcasted successfully!"),
                            Err(e) => error!("❌ Failed to broadcast test vote: {}", e),
                        }

                        // Clear the timer so this branch doesn't fire again
                        broadcast_timer = None;
                    }
                }
            }
        }
    }

    /// Handle behavior-specific events (GossipSub, RequestResponse, etc.)
    async fn handle_behavior_event(&mut self, event: crate::behavior::ThresholdBehaviorEvent) {
        use crate::behavior::ThresholdBehaviorEvent;

        match event {
            ThresholdBehaviorEvent::RequestResponse(rr_event) => {
                self.handle_request_response_event(rr_event).await;
            }
            ThresholdBehaviorEvent::Gossipsub(gs_event) => {
                use libp2p::gossipsub::Event;
                match gs_event {
                    Event::Message { message, .. } => {
                        info!("Received GossipSub message from peer: {:?}", message.source);

                        // Deserialize and process vote
                        match NetworkMessage::from_bytes(&message.data) {
                            Ok(NetworkMessage::Vote(vote_msg)) => {
                                info!("Received vote: tx_id={}, node_id={}, value={}",
                                    vote_msg.vote.tx_id, vote_msg.vote.node_id, vote_msg.vote.value);

                                // Send to vote processing channel
                                if let Err(e) = self.vote_tx.send(vote_msg.vote.clone()) {
                                    error!("Failed to send vote to processing channel: {}", e);
                                }
                            }
                            Ok(NetworkMessage::Ping) => {
                                info!("Received Ping from peer: {:?}", message.source);
                            }
                            Ok(NetworkMessage::Pong) => {
                                info!("Received Pong from peer: {:?}", message.source);
                            }
                            Err(e) => {
                                warn!("Failed to deserialize vote message: {}", e);
                            }
                        }
                    }
                    Event::Subscribed { peer_id, topic } => {
                        info!("Peer {} subscribed to topic: {}", peer_id, topic);
                    }
                    Event::Unsubscribed { peer_id, topic } => {
                        info!("Peer {} unsubscribed from topic: {}", peer_id, topic);
                    }
                    _ => {}
                }
            }
            ThresholdBehaviorEvent::Identify(id_event) => {
                use libp2p::identify::Event;
                match id_event {
                    Event::Received { peer_id, info } => {
                        info!("Identified peer {}: protocol_version={}, agent_version={}",
                            peer_id, info.protocol_version, info.agent_version);
                    }
                    Event::Sent { .. } => {}
                    Event::Pushed { .. } => {}
                    Event::Error { peer_id, error } => {
                        warn!("Identify error with peer {}: {}", peer_id, error);
                    }
                }
            }
            ThresholdBehaviorEvent::Kademlia(kad_event) => {
                use libp2p::kad::Event;
                match kad_event {
                    Event::RoutingUpdated { peer, .. } => {
                        info!("Kademlia routing table updated: peer={}", peer);
                    }
                    _ => {}
                }
            }
            ThresholdBehaviorEvent::Ping(ping_event) => {
                use libp2p::ping::Event;
                match ping_event {
                    Event { peer, result: Ok(_), .. } => {
                        info!("Ping successful to peer: {}", peer);
                    }
                    Event { peer, result: Err(e), .. } => {
                        warn!("Ping failed to peer {}: {}", peer, e);
                    }
                }
            }
        }
    }

    /// Handle request-response events
    async fn handle_request_response_event(&mut self, event: RequestResponseEvent<DirectRequest, DirectResponse>) {
        match event {
            RequestResponseEvent::Message { peer, message } => {
                match message {
                    Message::Request { request, channel, .. } => {
                        info!("Received request from {}: {:?}", peer, request);

                        // Handle different request types
                        let response = self.process_request(&request).await;

                        if let Err(e) = self.send_response(channel, response) {
                            error!("Failed to send response: {}", e);
                        }
                    }
                    Message::Response { response, .. } => {
                        info!("Received response from {}: {:?}", peer, response);

                        if let DirectResponse::Error { message } = &response {
                            warn!("Request failed: {}", message);
                        }
                        // Response handling can be extended with callbacks
                    }
                }
            }
            RequestResponseEvent::OutboundFailure { peer, request_id, error } => {
                error!("Outbound request failed to {:?}, request_id={:?}: {}", peer, request_id, error);
            }
            RequestResponseEvent::InboundFailure { peer, error, .. } => {
                error!("Inbound request failed from {}: {}", peer, error);
            }
            RequestResponseEvent::ResponseSent { peer, .. } => {
                info!("Response sent to peer: {}", peer);
            }
        }
    }

    /// Process incoming requests
    async fn process_request(&self, request: &DirectRequest) -> DirectResponse {
        match request {
            DirectRequest::GetVoteStatus { tx_id } => {
                // TODO: Query etcd for vote counts
                info!("Processing GetVoteStatus for tx_id={}", tx_id);
                DirectResponse::Error {
                    message: "Not yet implemented - requires etcd integration".to_string(),
                }
            }
            DirectRequest::GetPublicKey => {
                // TODO: Return node's public key
                info!("Processing GetPublicKey request");
                DirectResponse::Error {
                    message: "Not yet implemented".to_string(),
                }
            }
            DirectRequest::GetReputation { node_id } => {
                // TODO: Query PostgreSQL for reputation
                info!("Processing GetReputation for node_id={}", node_id);
                DirectResponse::Error {
                    message: "Not yet implemented - requires PostgreSQL integration".to_string(),
                }
            }
            DirectRequest::CustomMessage { message } => {
                info!("Received custom message: {}", message);
                DirectResponse::CustomMessage {
                    message: format!("Echo: {}", message),
                }
            }
        }
    }
}
