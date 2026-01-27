//! Automatic voting logic for transaction approval

use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use threshold_consensus::VoteProcessor;
use threshold_storage::PostgresStorage;
use threshold_types::{NodeId, Transaction, TransactionState, Vote, VoteRequest};

/// Automatic voter that processes vote requests
pub struct AutoVoter {
    node_id: NodeId,
    postgres: Arc<PostgresStorage>,
    vote_processor: Arc<VoteProcessor>,
    receiver: mpsc::Receiver<VoteRequest>,
}

impl AutoVoter {
    /// Create a new auto voter
    pub fn new(
        node_id: NodeId,
        postgres: Arc<PostgresStorage>,
        vote_processor: Arc<VoteProcessor>,
        receiver: mpsc::Receiver<VoteRequest>,
    ) -> Self {
        Self {
            node_id,
            postgres,
            vote_processor,
            receiver,
        }
    }

    /// Start the auto voter background task
    pub fn start(mut self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            info!("AutoVoter started for node {}", self.node_id);

            while let Some(vote_request) = self.receiver.recv().await {
                if let Err(e) = self.process_vote_request(vote_request).await {
                    error!("Failed to process vote request: {}", e);
                }
            }

            warn!("AutoVoter stopped for node {}", self.node_id);
        })
    }

    /// Process a vote request
    async fn process_vote_request(
        &self,
        req: VoteRequest,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            "Processing vote request: tx_id={} round={}",
            req.tx_id, req.round_number
        );

        // 1. Load transaction from database
        let tx = self
            .postgres
            .get_transaction(&req.tx_id)
            .await?
            .ok_or_else(|| format!("Transaction not found: {}", req.tx_id))?;

        info!(
            "Loaded transaction: recipient={} amount={} sats",
            tx.recipient, tx.amount_sats
        );

        // 2. Make voting decision (for demo: auto-approve all valid transactions)
        let should_approve = self.evaluate_transaction(&tx).await;

        if !should_approve {
            warn!("Transaction rejected by policy: {}", req.tx_id);
            // Could send reject vote here
            return Ok(());
        }

        // 3. Create and sign vote
        let vote = Vote {
            node_id: self.node_id,
            peer_id: threshold_types::PeerId(format!("node-{}", self.node_id.0)),
            tx_id: req.tx_id.clone(),
            round_id: req.round_number as u64,  // Use round_number (logical), not round_id (database ID)
            approve: true,
            value: tx.amount_sats,
            signature: vec![], // For demo: empty signature
            public_key: vec![], // For demo: empty public key
            timestamp: chrono::Utc::now(),
        };

        info!(
            "Casting vote: node_id={} tx_id={} approve=true",
            self.node_id, req.tx_id
        );

        // 4. Submit vote to vote processor
        match self.vote_processor.process_vote(vote).await {
            Ok(result) => {
                info!("Vote processed: {:?}", result);
            }
            Err(e) => {
                error!("Failed to process vote: {}", e);
            }
        }

        Ok(())
    }

    /// Evaluate whether to approve a transaction
    async fn evaluate_transaction(&self, tx: &Transaction) -> bool {
        // FOR DEMO: Simple validation rules
        // In production: check whitelist, limits, compliance, etc.

        // Rule 1: Amount must be reasonable (< 1 BTC for testnet)
        if tx.amount_sats > 100_000_000 {
            warn!("Transaction amount too high: {} sats", tx.amount_sats);
            return false;
        }

        // Rule 2: Recipient must not be empty
        if tx.recipient.is_empty() {
            warn!("Empty recipient address");
            return false;
        }

        // Rule 3: Transaction must be in valid state
        if tx.state != TransactionState::Voting {
            warn!("Transaction not in voting state: {:?}", tx.state);
            return false;
        }

        info!("Transaction approved by policy");
        true
    }
}
