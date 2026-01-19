use crate::config::AppConfig;
use libp2p::identity::Keypair;
use std::collections::HashMap;
use std::sync::Arc;
use threshold_consensus::VoteProcessor;
use threshold_crypto::KeyPair;
use threshold_network::P2PNode;
use threshold_storage::{EtcdStorage, PostgresStorage};
use threshold_types::{Result, TransactionId, Vote};
use tokio::sync::mpsc;
use tracing::{error, info};

pub struct ThresholdVotingApp {
    config: AppConfig,
    vote_processor: Arc<VoteProcessor>,
    keypair: KeyPair,
}

impl ThresholdVotingApp {
    pub async fn new(config: AppConfig) -> Result<Self> {
        config.validate()?;

        info!("Initializing Threshold Voting System");
        info!("Node ID: {}", config.node.node_id);
        info!("Peer ID: {}", config.node.peer_id);
        info!("Total nodes: {}", config.consensus.total_nodes);
        info!("Threshold: {}", config.consensus.threshold);

        let etcd = EtcdStorage::new(config.storage.etcd_endpoints.clone()).await?;
        let postgres = PostgresStorage::new(&config.storage.postgres_url).await?;

        let vote_processor = Arc::new(VoteProcessor::new(etcd, postgres));

        let keypair = KeyPair::generate();

        info!("Node public key: {}", hex::encode(&keypair.public_key()));

        Ok(Self {
            config,
            vote_processor,
            keypair,
        })
    }

    pub async fn initialize_etcd_config(&self) -> Result<()> {
        let mut etcd = EtcdStorage::new(self.config.storage.etcd_endpoints.clone()).await?;

        etcd.set_config_total_nodes(self.config.consensus.total_nodes).await?;
        etcd.set_config_threshold(self.config.consensus.threshold).await?;

        info!(
            "Initialized etcd config: N={}, t={}",
            self.config.consensus.total_nodes, self.config.consensus.threshold
        );

        Ok(())
    }

    pub async fn run(self) -> Result<()> {
        self.initialize_etcd_config().await?;

        let (vote_tx, mut vote_rx) = mpsc::unbounded_channel();

        let libp2p_keypair = Keypair::generate_ed25519();
        let mut p2p_node = P2PNode::new(
            self.config.node_id(),
            libp2p_keypair,
            vote_tx.clone(),
        )?;

        let listen_addr: libp2p::Multiaddr = self
            .config
            .node
            .listen_addr
            .parse()
            .map_err(|e| threshold_types::VotingError::NetworkError(format!("Invalid listen address: {}", e)))?;

        p2p_node.listen_on(listen_addr)?;

        for peer_addr in &self.config.network.bootstrap_peers {
            if let Ok(addr) = peer_addr.parse::<libp2p::Multiaddr>() {
                info!("Dialing bootstrap peer: {}", addr);
                p2p_node.dial(addr)?;
            } else {
                error!("Invalid bootstrap peer address: {}", peer_addr);
            }
        }

        let vote_processor = Arc::clone(&self.vote_processor);

        tokio::spawn(async move {
            info!("Vote processor started");
            while let Some(vote) = vote_rx.recv().await {
                info!(
                    "Processing vote: tx_id={} node_id={} value={}",
                    vote.tx_id, vote.node_id, vote.value
                );

                match vote_processor.process_vote(vote.clone()).await {
                    Ok(result) => {
                        info!("Vote processed successfully: {:?}", result);
                    }
                    Err(e) => {
                        error!("Failed to process vote: {}", e);
                    }
                }
            }
        });

        info!("P2P node starting...");

        // Schedule test vote broadcast for node_1
        let test_vote_opt = if self.config.node_id().to_string() == "node_1" {
            Some(self.submit_vote(TransactionId::from("auto_broadcast_test"), 888)?)
        } else {
            None
        };

        p2p_node.run_with_delayed_broadcast(test_vote_opt, 10).await?;

        Ok(())
    }

    pub fn submit_vote(&self, tx_id: TransactionId, value: u64) -> Result<Vote> {
        let message = format!("{}||{}", tx_id, value);
        let signature = self.keypair.sign(message.as_bytes());

        let vote = Vote::new(
            tx_id,
            self.config.node_id(),
            self.config.peer_id(),
            value,
            signature,
            self.keypair.public_key(),
        );

        info!("Created vote: tx_id={} value={}", vote.tx_id, vote.value);

        Ok(vote)
    }

    // === CLI Support Methods ===

    /// Get vote counts for a transaction from etcd
    pub async fn get_vote_counts(&self, tx_id: &TransactionId) -> Result<HashMap<u64, u64>> {
        let mut etcd = EtcdStorage::new(self.config.storage.etcd_endpoints.clone()).await?;
        etcd.get_all_vote_counts(tx_id).await
    }

    /// Get current threshold from etcd
    pub async fn get_threshold(&self) -> Result<u64> {
        let mut etcd = EtcdStorage::new(self.config.storage.etcd_endpoints.clone()).await?;
        let threshold = etcd.get_config_threshold().await?;
        Ok(threshold as u64)
    }

    /// Get node's public key
    pub fn get_public_key(&self) -> Vec<u8> {
        self.keypair.public_key()
    }

    /// Get node's peer ID
    pub fn get_peer_id(&self) -> String {
        self.config.node.peer_id.clone()
    }

    /// Get node ID
    pub fn get_node_id(&self) -> String {
        self.config.node.node_id.clone()
    }

    /// Query reputation score for a peer from PostgreSQL
    pub async fn get_reputation(&self, peer_id: &str) -> Result<f64> {
        let postgres = PostgresStorage::new(&self.config.storage.postgres_url).await?;
        postgres.get_reputation_score(&threshold_types::PeerId::from(peer_id)).await
    }

    /// Get transaction state from etcd
    pub async fn get_transaction_state(&self, tx_id: &TransactionId) -> Result<threshold_types::TransactionState> {
        let mut etcd = EtcdStorage::new(self.config.storage.etcd_endpoints.clone()).await?;
        etcd.get_transaction_state(tx_id).await
    }

    /// Check if blockchain submission exists (simplified - full implementation in storage layer)
    pub async fn has_blockchain_submission(&self, _tx_id: &TransactionId) -> Result<bool> {
        // Simplified: In production, query PostgreSQL blockchain_submissions table
        Ok(false)
    }
}

mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}
