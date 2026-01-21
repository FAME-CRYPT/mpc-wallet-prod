use crate::byzantine::{ByzantineCheckResult, ByzantineDetector};
use crate::fsm::{VoteFSM, VoteState};
use std::collections::HashMap;
use std::sync::Arc;
use threshold_storage::{EtcdStorage, PostgresStorage};
use threshold_types::{ConsensusResult, Result, TransactionId, Vote, VotingError};
use tokio::sync::Mutex;
use tracing::{info, warn};

/// Vote Processor orchestrates the consensus process
///
/// Responsibilities:
/// - Receives votes from network layer
/// - Validates votes using ByzantineDetector
/// - Counts votes using etcd
/// - Updates transaction FSM based on vote results
/// - Records to PostgreSQL for audit trail
pub struct VoteProcessor {
    byzantine_detector: Arc<Mutex<ByzantineDetector>>,
    fsm_registry: Arc<Mutex<HashMap<String, VoteFSM>>>,
}

impl VoteProcessor {
    /// Create a new VoteProcessor with storage backends
    pub fn new(etcd: EtcdStorage, postgres: PostgresStorage) -> Self {
        let detector = ByzantineDetector::new(etcd, postgres);

        Self {
            byzantine_detector: Arc::new(Mutex::new(detector)),
            fsm_registry: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Process an incoming vote from the network
    ///
    /// This is the main entry point for consensus. It:
    /// 1. Checks FSM state to ensure votes can be accepted
    /// 2. Validates vote through ByzantineDetector (signature, double-vote, etc.)
    /// 3. Updates vote counts in etcd
    /// 4. Checks if threshold reached
    /// 5. Updates FSM state accordingly
    /// 6. Records vote to PostgreSQL
    pub async fn process_vote(&self, vote: Vote) -> Result<VoteProcessingResult> {
        // Get or create FSM for this transaction
        let mut fsm_registry = self.fsm_registry.lock().await;
        let fsm = fsm_registry
            .entry(vote.tx_id.0.clone())
            .or_insert_with(|| {
                let mut fsm = VoteFSM::new(vote.tx_id.clone());
                // Start collecting votes automatically
                let _ = fsm.start_collecting();
                fsm
            });

        // Check if FSM can accept votes
        if !fsm.can_accept_votes() {
            warn!(
                "Cannot accept vote for tx_id={}: FSM state is {:?}",
                vote.tx_id,
                fsm.current_state()
            );
            return Err(VotingError::TransactionAlreadyProcessed {
                tx_id: vote.tx_id.0.clone(),
            });
        }

        // Release FSM lock before calling Byzantine detector (which needs storage locks)
        drop(fsm_registry);

        // Validate vote through Byzantine detector
        let mut detector = self.byzantine_detector.lock().await;
        let check_result = detector.check_vote(&vote).await?;

        match check_result {
            ByzantineCheckResult::Accepted { count } => {
                info!(
                    "Vote accepted for tx_id={} from node_id={} value={} count={}",
                    vote.tx_id, vote.node_id, vote.value, count
                );
                Ok(VoteProcessingResult::Accepted { count })
            }
            ByzantineCheckResult::ThresholdReached { value, count } => {
                // Update FSM to threshold reached
                let mut fsm_registry = self.fsm_registry.lock().await;
                if let Some(fsm) = fsm_registry.get_mut(&vote.tx_id.0) {
                    fsm.reach_threshold()?;
                }

                info!(
                    "Threshold reached for tx_id={} value={} count={}",
                    vote.tx_id, value, count
                );

                Ok(VoteProcessingResult::ConsensusReached(ConsensusResult {
                    tx_id: vote.tx_id.clone(),
                    value,
                    vote_count: count,
                    reached_at: chrono::Utc::now(),
                }))
            }
            ByzantineCheckResult::Rejected(violation_type) => {
                // Update FSM to aborted state
                let mut fsm_registry = self.fsm_registry.lock().await;
                if let Some(fsm) = fsm_registry.get_mut(&vote.tx_id.0) {
                    fsm.abort_byzantine()?;
                }

                warn!(
                    "Vote rejected for tx_id={} from peer_id={}: {:?}",
                    vote.tx_id, vote.peer_id, violation_type
                );

                Ok(VoteProcessingResult::Rejected { violation_type })
            }
            ByzantineCheckResult::Idempotent => {
                info!(
                    "Idempotent vote received for tx_id={} from node_id={}",
                    vote.tx_id, vote.node_id
                );
                Ok(VoteProcessingResult::Idempotent)
            }
        }
    }

    /// Get the current FSM state for a transaction
    pub async fn get_fsm(&self, tx_id: &TransactionId) -> Option<VoteState> {
        let fsm_registry = self.fsm_registry.lock().await;
        fsm_registry
            .get(&tx_id.0)
            .map(|fsm| fsm.current_state())
    }

    /// Mark a transaction as submitted for signing
    pub async fn mark_submitted(&self, tx_id: &TransactionId) -> Result<()> {
        let mut fsm_registry = self.fsm_registry.lock().await;
        if let Some(fsm) = fsm_registry.get_mut(&tx_id.0) {
            fsm.submit()?;
        }
        Ok(())
    }

    /// Mark a transaction as confirmed on blockchain
    pub async fn mark_confirmed(&self, tx_id: &TransactionId) -> Result<()> {
        let mut fsm_registry = self.fsm_registry.lock().await;
        if let Some(fsm) = fsm_registry.get_mut(&tx_id.0) {
            fsm.confirm()?;
        }
        Ok(())
    }

    /// Mark a transaction as timed out
    pub async fn mark_timeout(&self, tx_id: &TransactionId) -> Result<()> {
        let mut fsm_registry = self.fsm_registry.lock().await;
        if let Some(fsm) = fsm_registry.get_mut(&tx_id.0) {
            fsm.abort_timeout()?;
        }
        Ok(())
    }
}

/// Result of processing a vote
#[derive(Debug, Clone)]
pub enum VoteProcessingResult {
    /// Vote accepted and counted
    Accepted {
        count: u64,
    },
    /// Consensus reached - threshold met
    ConsensusReached(ConsensusResult),
    /// Vote rejected due to Byzantine violation
    Rejected {
        violation_type: threshold_types::ByzantineViolationType,
    },
    /// Idempotent vote (duplicate)
    Idempotent,
}

#[cfg(test)]
mod tests {
    use super::*;
    use threshold_crypto::KeyPair;
    use threshold_types::{NodeId, PeerId, PostgresConfig, TransactionId};

    #[tokio::test]
    #[ignore]
    async fn test_vote_processor() {
        let etcd = EtcdStorage::new(vec!["127.0.0.1:2379".to_string()])
            .await
            .unwrap();
        let postgres = PostgresStorage::new(&PostgresConfig {
            url: "postgresql://localhost/threshold_voting".to_string(),
            max_connections: 10,
            connect_timeout_secs: 5,
        })
        .await
        .unwrap();

        let processor = VoteProcessor::new(etcd, postgres);

        let keypair = KeyPair::generate();
        let tx_id = TransactionId::from("test_tx_processor");
        let node_id = NodeId::from(1);
        let peer_id = PeerId::from("peer_1");
        let value = 42u64;

        let message = format!("{}||{}", tx_id, value);
        let signature = keypair.sign(message.as_bytes());

        let vote = Vote::new(node_id, tx_id.clone(), 1, true, Some(value))
            .with_signature(signature)
            .with_public_key(keypair.public_key());

        let result = processor.process_vote(vote).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[ignore]
    async fn test_vote_processor_fsm_integration() {
        let etcd = EtcdStorage::new(vec!["127.0.0.1:2379".to_string()])
            .await
            .unwrap();
        let postgres = PostgresStorage::new(&PostgresConfig {
            url: "postgresql://localhost/threshold_voting".to_string(),
            max_connections: 10,
            connect_timeout_secs: 5,
        })
        .await
        .unwrap();

        let processor = VoteProcessor::new(etcd, postgres);
        let tx_id = TransactionId::from("test_fsm_integration");

        // Initially no FSM exists
        assert!(processor.get_fsm(&tx_id).await.is_none());

        // Create a vote to trigger FSM creation
        let keypair = KeyPair::generate();
        let node_id = NodeId::from(1);
        let value = 42u64;

        let message = format!("{}||{}", tx_id, value);
        let signature = keypair.sign(message.as_bytes());

        let vote = Vote::new(node_id, tx_id.clone(), 1, true, Some(value))
            .with_signature(signature)
            .with_public_key(keypair.public_key());

        processor.process_vote(vote).await.unwrap();

        // Now FSM should exist in Collecting state
        let state = processor.get_fsm(&tx_id).await;
        assert!(state.is_some());
        assert_eq!(state.unwrap(), VoteState::Collecting);
    }
}
