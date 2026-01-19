use crate::byzantine::{ByzantineCheckResult, ByzantineDetector};
use crate::fsm::{VoteFSM, VoteState};
use std::collections::HashMap;
use std::sync::Arc;
use threshold_storage::{EtcdStorage, PostgresStorage};
use threshold_types::{ConsensusResult, Result, TransactionId, Vote, VotingError};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

pub struct VoteProcessor {
    byzantine_detector: Arc<Mutex<ByzantineDetector>>,
    fsm_registry: Arc<Mutex<HashMap<String, VoteFSM>>>,
}

impl VoteProcessor {
    pub fn new(etcd: EtcdStorage, postgres: PostgresStorage) -> Self {
        let detector = ByzantineDetector::new(etcd, postgres);

        Self {
            byzantine_detector: Arc::new(Mutex::new(detector)),
            fsm_registry: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn process_vote(&self, vote: Vote) -> Result<VoteProcessingResult> {
        let mut fsm_registry = self.fsm_registry.lock().await;
        let fsm = fsm_registry
            .entry(vote.tx_id.0.clone())
            .or_insert_with(|| {
                let mut fsm = VoteFSM::new(vote.tx_id.clone());
                let _ = fsm.start_collecting();
                fsm
            });

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

        drop(fsm_registry);

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

    pub async fn get_fsm(&self, tx_id: &TransactionId) -> Option<VoteState> {
        let fsm_registry = self.fsm_registry.lock().await;
        fsm_registry
            .get(&tx_id.0)
            .map(|fsm| fsm.current_state())
    }

    pub async fn mark_submitted(&self, tx_id: &TransactionId) -> Result<()> {
        let mut fsm_registry = self.fsm_registry.lock().await;
        if let Some(fsm) = fsm_registry.get_mut(&tx_id.0) {
            fsm.submit()?;
        }
        Ok(())
    }

    pub async fn mark_confirmed(&self, tx_id: &TransactionId) -> Result<()> {
        let mut fsm_registry = self.fsm_registry.lock().await;
        if let Some(fsm) = fsm_registry.get_mut(&tx_id.0) {
            fsm.confirm()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum VoteProcessingResult {
    Accepted {
        count: u64,
    },
    ConsensusReached(ConsensusResult),
    Rejected {
        violation_type: threshold_types::ByzantineViolationType,
    },
    Idempotent,
}

#[cfg(test)]
mod tests {
    use super::*;
    use threshold_crypto::KeyPair;
    use threshold_types::{NodeId, PeerId, TransactionId};

    #[tokio::test]
    #[ignore]
    async fn test_vote_processor() {
        let etcd = EtcdStorage::new(vec!["127.0.0.1:2379".to_string()])
            .await
            .unwrap();
        let postgres = PostgresStorage::new("postgresql://localhost/threshold_voting")
            .await
            .unwrap();

        let processor = VoteProcessor::new(etcd, postgres);

        let keypair = KeyPair::generate();
        let tx_id = TransactionId::from("test_tx_processor");
        let node_id = NodeId::from("node_1");
        let peer_id = PeerId::from("peer_1");
        let value = 42u64;

        let message = format!("{}||{}", tx_id, value);
        let signature = keypair.sign(message.as_bytes());

        let vote = Vote::new(
            tx_id.clone(),
            node_id,
            peer_id,
            value,
            signature,
            keypair.public_key(),
        );

        let result = processor.process_vote(vote).await;
        assert!(result.is_ok());
    }
}
