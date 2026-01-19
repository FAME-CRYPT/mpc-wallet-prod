use libp2p::{
    gossipsub::{self, IdentTopic, MessageId},
    identify,
    kad::{self, store::MemoryStore},
    ping,
    request_response,
    swarm::NetworkBehaviour,
    PeerId,
};
use std::time::Duration;
use crate::request_response::DirectMessageCodec;

#[derive(NetworkBehaviour)]
pub struct ThresholdBehavior {
    pub gossipsub: gossipsub::Behaviour,
    pub kademlia: kad::Behaviour<MemoryStore>,
    pub identify: identify::Behaviour,
    pub ping: ping::Behaviour,
    pub request_response: request_response::Behaviour<DirectMessageCodec>,
}

impl ThresholdBehavior {
    pub fn new(local_peer_id: PeerId, public_key: libp2p::identity::PublicKey, keypair: libp2p::identity::Keypair) -> Self {
        let message_id_fn = |message: &gossipsub::Message| {
            let mut s = std::collections::hash_map::DefaultHasher::new();
            use std::hash::{Hash, Hasher};
            message.data.hash(&mut s);
            message.source.hash(&mut s);  // Include source to make messages unique per sender
            MessageId::from(s.finish().to_string())
        };

        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(1))
            .validation_mode(gossipsub::ValidationMode::Strict)
            .message_id_fn(message_id_fn)
            .max_transmit_size(262144)
            .build()
            .expect("Valid gossipsub config");

        let gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(keypair),  // Use actual node keypair!
            gossipsub_config,
        )
        .expect("Valid gossipsub behaviour");

        let kademlia = kad::Behaviour::new(local_peer_id, MemoryStore::new(local_peer_id));

        let identify = identify::Behaviour::new(identify::Config::new(
            "/threshold-voting/1.0.0".to_string(),
            public_key,
        ));

        let ping = ping::Behaviour::new(ping::Config::new());

        let request_response = crate::request_response::create_request_response_behaviour();

        Self {
            gossipsub,
            kademlia,
            identify,
            ping,
            request_response,
        }
    }

    pub fn subscribe_to_topic(&mut self, topic_name: &str) -> Result<bool, String> {
        let topic = IdentTopic::new(topic_name);
        self.gossipsub
            .subscribe(&topic)
            .map_err(|e| format!("Failed to subscribe: {:?}", e))
    }

    pub fn publish_message(&mut self, topic_name: &str, data: Vec<u8>) -> Result<MessageId, String> {
        let topic = IdentTopic::new(topic_name);
        self.gossipsub
            .publish(topic, data)
            .map_err(|e| format!("Failed to publish: {:?}", e))
    }

    pub fn add_peer_to_dht(&mut self, peer_id: PeerId, addr: libp2p::Multiaddr) {
        self.kademlia.add_address(&peer_id, addr);
    }
}

pub const VOTE_TOPIC: &str = "threshold-votes";
