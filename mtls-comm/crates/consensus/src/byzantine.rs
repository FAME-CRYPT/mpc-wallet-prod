use chrono::Utc;
use std::collections::HashMap;
use threshold_crypto::verify_vote;
use threshold_storage::{EtcdStorage, PostgresStorage};
use threshold_types::{
    ByzantineViolation, ByzantineViolationType, NodeId, PeerId, Result, TransactionId,
    TransactionState, Vote, VotingError,
};
use tracing::{error, info, warn};

pub struct ByzantineDetector {
    etcd: EtcdStorage,
    postgres: PostgresStorage,
}

impl ByzantineDetector {
    pub fn new(etcd: EtcdStorage, postgres: PostgresStorage) -> Self {
        Self { etcd, postgres }
    }

    pub async fn check_vote(&mut self, vote: &Vote) -> Result<ByzantineCheckResult> {
        if self.etcd.is_node_banned(&vote.peer_id).await? {
            return Err(VotingError::NodeBanned {
                peer_id: vote.peer_id.0.clone(),
            });
        }

        if let Err(e) = verify_vote(vote) {
            warn!(
                "Invalid signature from peer_id={} for tx_id={}",
                vote.peer_id, vote.tx_id
            );

            let violation = ByzantineViolation {
                peer_id: vote.peer_id.clone(),
                node_id: Some(vote.node_id.clone()),
                tx_id: vote.tx_id.clone(),
                violation_type: ByzantineViolationType::InvalidSignature,
                evidence: serde_json::json!({
                    "vote": {
                        "tx_id": vote.tx_id.0,
                        "node_id": vote.node_id.0,
                        "value": vote.value,
                        "signature": hex::encode(&vote.signature),
                        "public_key": hex::encode(&vote.public_key),
                    },
                    "error": e.to_string(),
                }),
                detected_at: Utc::now(),
            };

            self.handle_byzantine_violation(&violation).await?;

            return Ok(ByzantineCheckResult::Rejected(
                ByzantineViolationType::InvalidSignature,
            ));
        }

        if let Some(existing_vote) = self.etcd.store_vote(vote).await? {
            if existing_vote.value != vote.value {
                warn!(
                    "Double voting detected: peer_id={} node_id={} old_value={} new_value={}",
                    vote.peer_id, vote.node_id, existing_vote.value, vote.value
                );

                let violation = ByzantineViolation {
                    peer_id: vote.peer_id.clone(),
                    node_id: Some(vote.node_id.clone()),
                    tx_id: vote.tx_id.clone(),
                    violation_type: ByzantineViolationType::DoubleVoting,
                    evidence: serde_json::json!({
                        "old_vote": {
                            "value": existing_vote.value,
                            "timestamp": existing_vote.timestamp,
                        },
                        "new_vote": {
                            "value": vote.value,
                            "timestamp": vote.timestamp,
                        },
                    }),
                    detected_at: Utc::now(),
                };

                self.handle_byzantine_violation(&violation).await?;

                self.etcd
                    .set_transaction_state(&vote.tx_id, TransactionState::AbortedByzantine)
                    .await?;

                return Ok(ByzantineCheckResult::Rejected(
                    ByzantineViolationType::DoubleVoting,
                ));
            } else {
                return Ok(ByzantineCheckResult::Idempotent);
            }
        }

        let new_count = self.etcd.increment_vote_count(&vote.tx_id, vote.value).await?;

        let all_counts = self.etcd.get_all_vote_counts(&vote.tx_id).await?;

        let (max_value, max_count) = all_counts
            .iter()
            .max_by_key(|(_, &count)| count)
            .map(|(&value, &count)| (value, count))
            .unwrap_or((vote.value, new_count));

        let threshold = self.etcd.get_config_threshold().await?;

        if max_count >= threshold as u64 && vote.value != max_value {
            warn!(
                "Minority vote attack detected: peer_id={} voted {} but majority voted {}",
                vote.peer_id, vote.value, max_value
            );

            let violation = ByzantineViolation {
                peer_id: vote.peer_id.clone(),
                node_id: Some(vote.node_id.clone()),
                tx_id: vote.tx_id.clone(),
                violation_type: ByzantineViolationType::MinorityVote,
                evidence: serde_json::json!({
                    "voted_value": vote.value,
                    "majority_value": max_value,
                    "majority_count": max_count,
                    "threshold": threshold,
                    "all_counts": all_counts,
                }),
                detected_at: Utc::now(),
            };

            self.handle_byzantine_violation(&violation).await?;

            self.etcd
                .set_transaction_state(&vote.tx_id, TransactionState::AbortedByzantine)
                .await?;

            return Ok(ByzantineCheckResult::Rejected(
                ByzantineViolationType::MinorityVote,
            ));
        }

        self.postgres.record_vote(vote).await?;
        self.postgres.update_node_last_seen(&vote.peer_id).await?;

        if new_count >= threshold as u64 {
            info!(
                "Threshold reached for tx_id={} value={} count={}",
                vote.tx_id, vote.value, new_count
            );

            self.etcd
                .set_transaction_state(&vote.tx_id, TransactionState::ThresholdReached)
                .await?;

            return Ok(ByzantineCheckResult::ThresholdReached {
                value: vote.value,
                count: new_count,
            });
        }

        Ok(ByzantineCheckResult::Accepted { count: new_count })
    }

    async fn handle_byzantine_violation(&mut self, violation: &ByzantineViolation) -> Result<()> {
        self.etcd.ban_node(violation).await?;

        self.postgres.record_byzantine_violation(violation).await?;

        error!(
            "Byzantine violation detected and handled: peer_id={} type={:?}",
            violation.peer_id, violation.violation_type
        );

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum ByzantineCheckResult {
    Accepted {
        count: u64,
    },
    ThresholdReached {
        value: u64,
        count: u64,
    },
    Rejected(ByzantineViolationType),
    Idempotent,
}

mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use threshold_crypto::KeyPair;
    use threshold_types::{NodeId, PeerId, TransactionId};

    #[tokio::test]
    #[ignore]
    async fn test_double_voting_detection() {
        let etcd = EtcdStorage::new(vec!["127.0.0.1:2379".to_string()])
            .await
            .unwrap();
        let postgres = PostgresStorage::new("postgresql://localhost/threshold_voting")
            .await
            .unwrap();

        let mut detector = ByzantineDetector::new(etcd, postgres);

        let keypair = KeyPair::generate();
        let tx_id = TransactionId::from("test_tx_001");
        let node_id = NodeId::from("node_1");
        let peer_id = PeerId::from("peer_1");

        let message1 = format!("{}||{}", tx_id, 42u64);
        let signature1 = keypair.sign(message1.as_bytes());
        let vote1 = Vote::new(
            tx_id.clone(),
            node_id.clone(),
            peer_id.clone(),
            42,
            signature1,
            keypair.public_key(),
        );

        let result1 = detector.check_vote(&vote1).await;
        assert!(matches!(result1, Ok(ByzantineCheckResult::Accepted { .. })));

        let message2 = format!("{}||{}", tx_id, 99u64);
        let signature2 = keypair.sign(message2.as_bytes());
        let vote2 = Vote::new(
            tx_id.clone(),
            node_id.clone(),
            peer_id.clone(),
            99,
            signature2,
            keypair.public_key(),
        );

        let result2 = detector.check_vote(&vote2).await;
        assert!(matches!(
            result2,
            Ok(ByzantineCheckResult::Rejected(
                ByzantineViolationType::DoubleVoting
            ))
        ));
    }
}
