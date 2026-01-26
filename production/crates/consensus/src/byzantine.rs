use chrono::Utc;
use threshold_crypto::verify_vote;
use threshold_storage::{EtcdStorage, PostgresStorage};
use threshold_types::{
    ByzantineViolation, ByzantineViolationType, Result,
    TransactionState, Vote, VotingError,
};
use tracing::{error, info, warn};

/// Byzantine fault detector that validates votes and detects malicious behavior
pub struct ByzantineDetector {
    etcd: EtcdStorage,
    postgres: PostgresStorage,
}

impl ByzantineDetector {
    pub fn new(etcd: EtcdStorage, postgres: PostgresStorage) -> Self {
        Self { etcd, postgres }
    }

    /// Check a vote for Byzantine violations
    ///
    /// This method performs four types of Byzantine violation detection:
    /// 1. DoubleVote: Same node votes differently on same TX
    /// 2. InvalidSignature: Vote signature verification fails
    /// 3. MinorityVote: Node votes against consensus after threshold reached
    /// 4. Timeout: Node doesn't respond within timeout (handled elsewhere)
    pub async fn check_vote(&mut self, vote: &Vote) -> Result<ByzantineCheckResult> {
        // Check if node is already banned
        if self.etcd.is_peer_banned(&vote.peer_id).await? {
            return Err(VotingError::NodeBanned {
                peer_id: vote.peer_id.0.clone(),
            });
        }

        // Violation Type 1: Invalid Signature
        if let Err(e) = verify_vote(vote) {
            warn!(
                "Invalid signature from peer_id={} for tx_id={}",
                vote.peer_id, vote.tx_id
            );

            let violation = ByzantineViolation {
                id: None,
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

        // Violation Type 2: Double Voting
        // Store vote and check if a different vote already exists
        if let Some(existing_vote) = self.etcd.store_vote(vote).await? {
            if existing_vote.value != vote.value {
                warn!(
                    "Double voting detected: peer_id={} node_id={} old_value={} new_value={}",
                    vote.peer_id, vote.node_id, existing_vote.value, vote.value
                );

                let violation = ByzantineViolation {
                    id: None,
                    peer_id: vote.peer_id.clone(),
                    node_id: Some(vote.node_id.clone()),
                    tx_id: vote.tx_id.clone(),
                    violation_type: ByzantineViolationType::DoubleVote,
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
                    ByzantineViolationType::DoubleVote,
                ));
            } else {
                // Same vote received again - idempotent
                return Ok(ByzantineCheckResult::Idempotent);
            }
        }

        // Increment vote count for this value
        let new_count = self.etcd.increment_vote_count(&vote.tx_id, vote.value).await?;

        // Get all vote counts to detect minority voting attacks
        let all_counts = self.etcd.get_all_vote_counts(&vote.tx_id).await?;

        let (max_value, max_count) = all_counts
            .iter()
            .max_by_key(|(_, &count)| count)
            .map(|(&value, &count)| (value, count))
            .unwrap_or((vote.value, new_count));

        let threshold = self.etcd.get_config_threshold().await?;

        // Violation Type 3: Minority Vote Attack
        // If threshold already reached for a different value, this is suspicious
        if max_count >= threshold as u64 && vote.value != max_value {
            warn!(
                "Minority vote attack detected: peer_id={} voted {} but majority voted {}",
                vote.peer_id, vote.value, max_value
            );

            let violation = ByzantineViolation {
                id: None,
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

        // Record valid vote to PostgreSQL
        self.postgres.record_vote(vote).await?;
        self.postgres.update_node_last_seen(&vote.node_id).await?;

        // Check if threshold reached
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

    /// Handle a Byzantine violation by banning the node and recording the violation
    async fn handle_byzantine_violation(&mut self, violation: &ByzantineViolation) -> Result<()> {
        // Ban node in etcd
        self.etcd.ban_node(violation).await?;

        // Record violation in PostgreSQL for audit trail
        self.postgres.record_byzantine_violation(violation).await?;

        error!(
            "Byzantine violation detected and handled: peer_id={} type={:?}",
            violation.peer_id, violation.violation_type
        );

        Ok(())
    }
}

/// Result of Byzantine validation check
#[derive(Debug, Clone)]
pub enum ByzantineCheckResult {
    /// Vote accepted and counted
    Accepted {
        count: u64,
    },
    /// Threshold reached for consensus
    ThresholdReached {
        value: u64,
        count: u64,
    },
    /// Vote rejected due to Byzantine violation
    Rejected(ByzantineViolationType),
    /// Idempotent vote (same vote received again)
    Idempotent,
}

/// Helper module for hex encoding
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
        let postgres = PostgresStorage::new(&threshold_types::PostgresConfig {
            url: "postgresql://localhost/threshold_voting".to_string(),
            max_connections: 10,
            connect_timeout_secs: 5,
        })
        .await
        .unwrap();

        let mut detector = ByzantineDetector::new(etcd, postgres);

        let keypair = KeyPair::generate();
        let tx_id = TransactionId::from("test_tx_001");
        let node_id = NodeId::from(1);
        let peer_id = PeerId::from("peer_1");

        let message1 = format!("{}||{}", tx_id, 42u64);
        let signature1 = keypair.sign(message1.as_bytes());
        let vote1 = Vote::new(node_id, tx_id.clone(), 1, true, Some(42))
            .with_signature(signature1)
            .with_public_key(keypair.public_key());

        let result1 = detector.check_vote(&vote1).await;
        assert!(matches!(result1, Ok(ByzantineCheckResult::Accepted { .. })));

        let message2 = format!("{}||{}", tx_id, 99u64);
        let signature2 = keypair.sign(message2.as_bytes());
        let vote2 = Vote::new(node_id, tx_id.clone(), 1, true, Some(99))
            .with_signature(signature2)
            .with_public_key(keypair.public_key());

        let result2 = detector.check_vote(&vote2).await;
        assert!(matches!(
            result2,
            Ok(ByzantineCheckResult::Rejected(
                ByzantineViolationType::DoubleVote
            ))
        ));
    }
}
