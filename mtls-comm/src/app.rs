use crate::config::AppConfig;
use std::collections::HashMap;
use std::sync::Arc;
use threshold_consensus::VoteProcessor;
use threshold_crypto::KeyPair;
use threshold_network::{MeshTopology, MtlsNode};
use threshold_storage::{EtcdStorage, PostgresStorage};
use threshold_types::{Result, TransactionId, Vote};
use tokio::sync::mpsc;
use tracing::{error, info};

pub struct ThresholdVotingApp {
    config: AppConfig,
    vote_processor: Arc<VoteProcessor>,
    keypair: KeyPair,
    mtls_node: Option<Arc<MtlsNode>>,
}

impl ThresholdVotingApp {
    pub async fn new(config: AppConfig) -> Result<Self> {
        config.validate()?;

        info!("Initializing mTLS Threshold Voting System");
        info!("Node ID: {}", config.node.node_id);
        info!("Total nodes: {}", config.consensus.total_nodes);
        info!("Threshold: {}", config.consensus.threshold);
        info!("TLS Version: {}", config.mtls.tls_version);

        let etcd = EtcdStorage::new(config.etcd.endpoints.clone()).await?;
        let postgres = PostgresStorage::new(&config.postgres.url).await?;

        let vote_processor = Arc::new(VoteProcessor::new(etcd, postgres));

        let keypair = KeyPair::generate();

        info!("Node public key: {}", hex::encode(&keypair.public_key()));

        Ok(Self {
            config,
            vote_processor,
            keypair,
            mtls_node: None,
        })
    }

    pub async fn initialize_etcd_config(&self) -> Result<()> {
        let mut etcd = EtcdStorage::new(self.config.etcd.endpoints.clone()).await?;

        etcd.set_config_total_nodes(self.config.consensus.total_nodes)
            .await?;
        etcd.set_config_threshold(self.config.consensus.threshold)
            .await?;

        info!(
            "Initialized etcd config: N={}, t={}",
            self.config.consensus.total_nodes, self.config.consensus.threshold
        );

        Ok(())
    }

    pub async fn run(mut self) -> Result<()> {
        self.initialize_etcd_config().await?;

        let (vote_tx, mut vote_rx) = mpsc::unbounded_channel();

        // Create mTLS node
        let mtls_node = MtlsNode::new(
            self.config.node_id(),
            &self.config.mtls.node_cert_path,
            &self.config.mtls.node_key_path,
            &self.config.mtls.ca_cert_path,
            vote_tx.clone(),
        )
        .map_err(|e| threshold_types::VotingError::NetworkError(format!("Failed to create mTLS node: {}", e)))?;

        // Listen for incoming connections
        let listen_addr = self.config.listen_addr()?;
        mtls_node
            .listen_on(listen_addr)
            .await
            .map_err(|e| threshold_types::VotingError::NetworkError(format!("Failed to listen: {}", e)))?;

        info!("Listening on {}", listen_addr);

        // Connect to bootstrap peers
        let bootstrap_peers = self.config.bootstrap_peers()?;
        let mut mesh = MeshTopology::new(bootstrap_peers.clone());

        info!("Connecting to bootstrap peers...");
        mesh.discover_peers(&mtls_node)
            .await
            .map_err(|e| threshold_types::VotingError::NetworkError(format!("Failed to discover peers: {}", e)))?;

        // Spawn connection maintenance task
        let mtls_node_for_maintenance = Arc::new(mtls_node);
        self.mtls_node = Some(Arc::clone(&mtls_node_for_maintenance));
        let mtls_node_clone = Arc::clone(&mtls_node_for_maintenance);
        tokio::spawn(async move {
            MeshTopology::maintain_connections(&mtls_node_clone, bootstrap_peers).await;
        });

        // Spawn vote processor
        let vote_processor = Arc::clone(&self.vote_processor);
        tokio::spawn(async move {
            info!("Vote processor started");
            while let Some(vote) = vote_rx.recv().await {
                info!(
                    "Processing vote: tx_id={} node_id={} value={}",
                    vote.tx_id, vote.node_id.0, vote.value
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

        // Test broadcast after 30 seconds (only for node-1)
        let node_id = self.config.node.node_id;
        if node_id == 1 {
            let test_vote = self.submit_vote(TransactionId::from("AUTO_TEST_BROADCAST_001"), 9999)?;
            let mtls_for_test = Arc::clone(&mtls_node_for_maintenance);
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                info!("🚀 AUTO-TEST: Broadcasting test vote from node-1...");
                match mtls_for_test.broadcast_vote(test_vote).await {
                    Ok(_) => info!("✅ AUTO-TEST: Vote broadcast successful!"),
                    Err(e) => error!("Auto-test broadcast failed: {}", e),
                }
            });
        }

        // Main loop - keep alive and report status
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            let connected_peers = mtls_node_for_maintenance.connected_peers().await;
            info!(
                "Status: {} peers connected: {:?}",
                connected_peers.len(),
                connected_peers.iter().map(|id| id.0).collect::<Vec<_>>()
            );
        }
    }

    pub fn submit_vote(&self, tx_id: TransactionId, value: u64) -> Result<Vote> {
        let message = format!("{}||{}", tx_id, value);
        let signature = self.keypair.sign(message.as_bytes());

        // Generate peer_id from node_id for mTLS compatibility
        let peer_id = threshold_types::PeerId::from(format!("mtls-node-{}", self.config.node.node_id));

        let vote = Vote::new(
            tx_id,
            self.config.node_id(),
            peer_id,
            value,
            signature,
            self.keypair.public_key(),
        );

        info!("Created vote: tx_id={} value={}", vote.tx_id, vote.value);

        Ok(vote)
    }

    /// Broadcast a vote to all connected peers via mTLS
    pub async fn broadcast_vote(&self, vote: Vote) -> Result<()> {
        if let Some(node) = &self.mtls_node {
            node.broadcast_vote(vote)
                .await
                .map_err(|e| threshold_types::VotingError::NetworkError(format!("Failed to broadcast vote: {}", e)))?;
            info!("Vote broadcasted to all peers");
            Ok(())
        } else {
            Err(threshold_types::VotingError::NetworkError(
                "mTLS node not initialized - can only broadcast from running node".to_string(),
            ))
        }
    }

    // === CLI Support Methods ===

    /// Get vote counts for a transaction from etcd
    pub async fn get_vote_counts(&self, tx_id: &TransactionId) -> Result<HashMap<u64, u64>> {
        let mut etcd = EtcdStorage::new(self.config.etcd.endpoints.clone()).await?;
        etcd.get_all_vote_counts(tx_id).await
    }

    /// Get current threshold from etcd
    pub async fn get_threshold(&self) -> Result<u64> {
        let mut etcd = EtcdStorage::new(self.config.etcd.endpoints.clone()).await?;
        let threshold = etcd.get_config_threshold().await?;
        Ok(threshold as u64)
    }

    /// Get node's public key
    pub fn get_public_key(&self) -> Vec<u8> {
        self.keypair.public_key()
    }

    /// Get node ID
    pub fn get_node_id(&self) -> u64 {
        self.config.node.node_id
    }

    /// Get transaction state from etcd
    pub async fn get_transaction_state(
        &self,
        tx_id: &TransactionId,
    ) -> Result<threshold_types::TransactionState> {
        let mut etcd = EtcdStorage::new(self.config.etcd.endpoints.clone()).await?;
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
