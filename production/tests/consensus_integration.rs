//! Consensus integration tests.
//!
//! Tests Byzantine fault detection, vote processing, and consensus FSM.

mod common;

use common::*;
use threshold_consensus::{ByzantineCheckResult, ByzantineDetector};
use threshold_storage::{EtcdStorage, PostgresStorage};
use threshold_types::*;
use chrono::Utc;

#[tokio::test]
async fn test_valid_vote_acceptance() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());

    let postgres = PostgresStorage::new(&config).await.unwrap();
    let etcd = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    let mut detector = ByzantineDetector::new(etcd, postgres);

    // Setup configuration
    detector.etcd.set_cluster_threshold(4).await.unwrap();

    let tx_id = test_tx_id("consensus_001");
    let (vote, _key) = create_signed_vote(NodeId(1), tx_id.clone(), 1, true, 100);

    let result = detector.check_vote(&vote).await.unwrap();

    match result {
        ByzantineCheckResult::Accepted { count } => {
            assert_eq!(count, 1, "First vote should have count 1");
        }
        _ => panic!("Expected Accepted result"),
    }
}

#[tokio::test]
async fn test_double_vote_detection() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());

    let postgres = PostgresStorage::new(&config).await.unwrap();
    let etcd = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    let mut detector = ByzantineDetector::new(etcd, postgres);
    detector.etcd.set_cluster_threshold(4).await.unwrap();

    let tx_id = test_tx_id("double_vote_001");
    let node_id = NodeId(1);

    // Submit first vote
    let (vote1, _key) = create_signed_vote(node_id, tx_id.clone(), 1, true, 100);
    let result1 = detector.check_vote(&vote1).await.unwrap();
    assert!(matches!(result1, ByzantineCheckResult::Accepted { .. }));

    // Submit conflicting vote from same node
    let (vote2, _key) = create_signed_vote(node_id, tx_id.clone(), 1, false, 200);
    let result2 = detector.check_vote(&vote2).await.unwrap();

    match result2 {
        ByzantineCheckResult::Rejected(ViolationType::DoubleVote) => {
            // Expected
        }
        _ => panic!("Expected DoubleVote violation"),
    }

    // Verify node is banned
    let is_banned = detector.etcd.is_node_banned(node_id).await.unwrap();
    assert!(is_banned, "Node should be banned for double voting");

    // Verify transaction state is AbortedByzantine
    let state = detector.etcd.get_transaction_state(&tx_id).await.unwrap();
    assert_eq!(state, TransactionState::AbortedByzantine);
}

#[tokio::test]
async fn test_invalid_signature_detection() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());

    let postgres = PostgresStorage::new(&config).await.unwrap();
    let etcd = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    let mut detector = ByzantineDetector::new(etcd, postgres);
    detector.etcd.set_cluster_threshold(4).await.unwrap();

    let tx_id = test_tx_id("invalid_sig_001");
    let vote = create_invalid_signature_vote(NodeId(2), tx_id.clone(), 1, true, 100);

    let result = detector.check_vote(&vote).await.unwrap();

    match result {
        ByzantineCheckResult::Rejected(ViolationType::InvalidSignature) => {
            // Expected
        }
        _ => panic!("Expected InvalidSignature violation"),
    }

    // Verify violation was recorded in PostgreSQL
    let violations = detector.postgres.get_node_violations(NodeId(2)).await.unwrap();
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].violation_type, ViolationType::InvalidSignature);
}

#[tokio::test]
async fn test_threshold_consensus_4_of_5() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());

    let postgres = PostgresStorage::new(&config).await.unwrap();
    let etcd = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    let mut detector = ByzantineDetector::new(etcd, postgres);
    detector.etcd.set_cluster_threshold(4).await.unwrap();

    let tx_id = test_tx_id("threshold_001");

    // Submit 3 votes - should not reach threshold
    for i in 1..=3 {
        let (vote, _key) = create_signed_vote(NodeId(i), tx_id.clone(), 1, true, 100);
        let result = detector.check_vote(&vote).await.unwrap();
        assert!(matches!(result, ByzantineCheckResult::Accepted { .. }));
    }

    // Submit 4th vote - should reach threshold
    let (vote4, _key) = create_signed_vote(NodeId(4), tx_id.clone(), 1, true, 100);
    let result = detector.check_vote(&vote4).await.unwrap();

    match result {
        ByzantineCheckResult::ThresholdReached { value, count } => {
            assert_eq!(value, 100);
            assert_eq!(count, 4);
        }
        _ => panic!("Expected ThresholdReached result"),
    }

    // Verify transaction state
    let state = detector.etcd.get_transaction_state(&tx_id).await.unwrap();
    assert_eq!(state, TransactionState::ThresholdReached);
}

#[tokio::test]
async fn test_minority_vote_detection() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());

    let postgres = PostgresStorage::new(&config).await.unwrap();
    let etcd = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    let mut detector = ByzantineDetector::new(etcd, postgres);
    detector.etcd.set_cluster_threshold(4).await.unwrap();

    let tx_id = test_tx_id("minority_001");

    // 4 nodes vote for value 100 (reaches threshold)
    for i in 1..=4 {
        let (vote, _key) = create_signed_vote(NodeId(i), tx_id.clone(), 1, true, 100);
        detector.check_vote(&vote).await.unwrap();
    }

    // 5th node votes for different value (minority attack)
    let (vote5, _key) = create_signed_vote(NodeId(5), tx_id.clone(), 1, true, 200);
    let result = detector.check_vote(&vote5).await.unwrap();

    match result {
        ByzantineCheckResult::Rejected(ViolationType::MinorityVote) => {
            // Expected
        }
        _ => panic!("Expected MinorityVote violation"),
    }

    // Verify node is banned
    let is_banned = detector.etcd.is_node_banned(NodeId(5)).await.unwrap();
    assert!(is_banned);
}

#[tokio::test]
async fn test_idempotent_vote_submission() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());

    let postgres = PostgresStorage::new(&config).await.unwrap();
    let etcd = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    let mut detector = ByzantineDetector::new(etcd, postgres);
    detector.etcd.set_cluster_threshold(4).await.unwrap();

    let tx_id = test_tx_id("idempotent_001");
    let (vote, _key) = create_signed_vote(NodeId(1), tx_id.clone(), 1, true, 100);

    // First submission
    let result1 = detector.check_vote(&vote).await.unwrap();
    assert!(matches!(result1, ByzantineCheckResult::Accepted { .. }));

    // Second submission (identical vote)
    let result2 = detector.check_vote(&vote).await.unwrap();
    assert!(matches!(result2, ByzantineCheckResult::Idempotent));

    // Verify vote count is still 1
    let count = detector.etcd.get_vote_count(&tx_id, 100).await.unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn test_banned_node_rejection() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());

    let postgres = PostgresStorage::new(&config).await.unwrap();
    let etcd = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    let mut detector = ByzantineDetector::new(etcd, postgres);

    let node_id = NodeId(99);
    let tx_id = test_tx_id("banned_001");

    // Ban the node first
    let violation = sample_byzantine_violation(99, &tx_id.0, ViolationType::DoubleVote);
    detector.etcd.ban_node(&violation).await.unwrap();

    // Try to submit vote from banned node
    let (vote, _key) = create_signed_vote(node_id, tx_id.clone(), 1, true, 100);
    let result = detector.check_vote(&vote).await;

    assert!(result.is_err(), "Banned node should be rejected");
    match result.unwrap_err() {
        Error::NodeBanned { peer_id } => {
            assert_eq!(peer_id, "peer-99");
        }
        _ => panic!("Expected NodeBanned error"),
    }
}

#[tokio::test]
async fn test_concurrent_vote_submission() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());
    let endpoints = ctx.etcd_endpoints();

    // Setup threshold
    {
        let mut etcd = EtcdStorage::new(endpoints.clone()).await.unwrap();
        etcd.set_cluster_threshold(4).await.unwrap();
    }

    let tx_id = test_tx_id("concurrent_votes_001");

    // Submit 5 votes concurrently
    let mut handles = vec![];
    for i in 1..=5 {
        let config_clone = config.clone();
        let endpoints_clone = endpoints.clone();
        let tx_id_clone = tx_id.clone();

        let handle = tokio::spawn(async move {
            let postgres = PostgresStorage::new(&config_clone).await.unwrap();
            let etcd = EtcdStorage::new(endpoints_clone).await.unwrap();
            let mut detector = ByzantineDetector::new(etcd, postgres);

            let (vote, _key) = create_signed_vote(NodeId(i), tx_id_clone, 1, true, 100);
            detector.check_vote(&vote).await.unwrap()
        });
        handles.push(handle);
    }

    // Wait for all votes
    let mut threshold_reached = false;
    for handle in handles {
        let result = handle.await.unwrap();
        if matches!(result, ByzantineCheckResult::ThresholdReached { .. }) {
            threshold_reached = true;
        }
    }

    assert!(threshold_reached, "Threshold should be reached");

    // Verify final count
    let mut etcd = EtcdStorage::new(endpoints).await.unwrap();
    let count = etcd.get_vote_count(&tx_id, 100).await.unwrap();
    assert_eq!(count, 5);
}

#[tokio::test]
async fn test_exactly_threshold_votes() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());

    let postgres = PostgresStorage::new(&config).await.unwrap();
    let etcd = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    let mut detector = ByzantineDetector::new(etcd, postgres);
    detector.etcd.set_cluster_threshold(4).await.unwrap();

    let tx_id = test_tx_id("exact_threshold_001");

    // Submit exactly 4 votes (threshold)
    for i in 1..=4 {
        let (vote, _key) = create_signed_vote(NodeId(i), tx_id.clone(), 1, true, 100);
        let result = detector.check_vote(&vote).await.unwrap();

        if i < 4 {
            assert!(matches!(result, ByzantineCheckResult::Accepted { .. }));
        } else {
            // 4th vote should trigger threshold
            assert!(matches!(result, ByzantineCheckResult::ThresholdReached { .. }));
        }
    }
}

#[tokio::test]
async fn test_tie_vote_scenario() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());

    let postgres = PostgresStorage::new(&config).await.unwrap();
    let etcd = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    let mut detector = ByzantineDetector::new(etcd, postgres);
    detector.etcd.set_cluster_threshold(4).await.unwrap();

    let tx_id = test_tx_id("tie_vote_001");

    // 2 votes for value 100
    for i in 1..=2 {
        let (vote, _key) = create_signed_vote(NodeId(i), tx_id.clone(), 1, true, 100);
        detector.check_vote(&vote).await.unwrap();
    }

    // 2 votes for value 200
    for i in 3..=4 {
        let (vote, _key) = create_signed_vote(NodeId(i), tx_id.clone(), 1, true, 200);
        detector.check_vote(&vote).await.unwrap();
    }

    // Verify both values have count 2
    let count_100 = detector.etcd.get_vote_count(&tx_id, 100).await.unwrap();
    let count_200 = detector.etcd.get_vote_count(&tx_id, 200).await.unwrap();

    assert_eq!(count_100, 2);
    assert_eq!(count_200, 2);

    // 5th vote breaks the tie
    let (vote5, _key) = create_signed_vote(NodeId(5), tx_id.clone(), 1, true, 100);
    let result = detector.check_vote(&vote5).await.unwrap();

    // Still only 3 votes for 100, threshold not reached
    assert!(matches!(result, ByzantineCheckResult::Accepted { count: 3 }));
}

#[tokio::test]
async fn test_vote_with_zero_value() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());

    let postgres = PostgresStorage::new(&config).await.unwrap();
    let etcd = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    let mut detector = ByzantineDetector::new(etcd, postgres);
    detector.etcd.set_cluster_threshold(4).await.unwrap();

    let tx_id = test_tx_id("zero_value_001");
    let (vote, _key) = create_signed_vote(NodeId(1), tx_id.clone(), 1, true, 0);

    let result = detector.check_vote(&vote).await.unwrap();
    assert!(matches!(result, ByzantineCheckResult::Accepted { .. }));

    let count = detector.etcd.get_vote_count(&tx_id, 0).await.unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn test_multiple_transactions_concurrent() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());
    let endpoints = ctx.etcd_endpoints();

    // Setup threshold
    {
        let mut etcd = EtcdStorage::new(endpoints.clone()).await.unwrap();
        etcd.set_cluster_threshold(4).await.unwrap();
    }

    // Process votes for 3 different transactions concurrently
    let mut handles = vec![];

    for tx_num in 1..=3 {
        for node_id in 1..=5 {
            let config_clone = config.clone();
            let endpoints_clone = endpoints.clone();
            let tx_id = test_tx_id(&format!("multi_tx_{:03}", tx_num));

            let handle = tokio::spawn(async move {
                let postgres = PostgresStorage::new(&config_clone).await.unwrap();
                let etcd = EtcdStorage::new(endpoints_clone).await.unwrap();
                let mut detector = ByzantineDetector::new(etcd, postgres);

                let (vote, _key) = create_signed_vote(NodeId(node_id), tx_id, 1, true, 100);
                detector.check_vote(&vote).await.unwrap()
            });
            handles.push(handle);
        }
    }

    // Wait for all votes
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify all 3 transactions reached threshold
    let mut etcd = EtcdStorage::new(endpoints).await.unwrap();
    for tx_num in 1..=3 {
        let tx_id = test_tx_id(&format!("multi_tx_{:03}", tx_num));
        let count = etcd.get_vote_count(&tx_id, 100).await.unwrap();
        assert_eq!(count, 5, "Transaction {} should have 5 votes", tx_num);

        let state = etcd.get_transaction_state(&tx_id).await.unwrap();
        assert_eq!(state, TransactionState::ThresholdReached);
    }
}

#[tokio::test]
async fn test_vote_evidence_recording() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());

    let postgres = PostgresStorage::new(&config).await.unwrap();
    let etcd = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    let mut detector = ByzantineDetector::new(etcd, postgres);
    detector.etcd.set_cluster_threshold(4).await.unwrap();

    let tx_id = test_tx_id("evidence_001");
    let node_id = NodeId(1);

    // Submit first vote
    let (vote1, _key) = create_signed_vote(node_id, tx_id.clone(), 1, true, 100);
    detector.check_vote(&vote1).await.unwrap();

    // Submit conflicting vote (triggers double-vote)
    let (vote2, _key) = create_signed_vote(node_id, tx_id.clone(), 1, false, 200);
    detector.check_vote(&vote2).await.unwrap();

    // Check that evidence was recorded
    let violations = detector.postgres.get_node_violations(node_id).await.unwrap();
    assert_eq!(violations.len(), 1);

    let violation = &violations[0];
    assert_eq!(violation.violation_type, ViolationType::DoubleVote);

    // Verify evidence contains both votes
    let evidence = &violation.evidence;
    assert!(evidence.get("old_vote").is_some());
    assert!(evidence.get("new_vote").is_some());
}

#[tokio::test]
async fn test_high_value_consensus() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());

    let postgres = PostgresStorage::new(&config).await.unwrap();
    let etcd = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    let mut detector = ByzantineDetector::new(etcd, postgres);
    detector.etcd.set_cluster_threshold(4).await.unwrap();

    let tx_id = test_tx_id("high_value_001");
    let high_value = u64::MAX - 1000;

    // Submit votes with very high value
    for i in 1..=4 {
        let (vote, _key) = create_signed_vote(NodeId(i), tx_id.clone(), 1, true, high_value);
        let result = detector.check_vote(&vote).await.unwrap();

        if i == 4 {
            match result {
                ByzantineCheckResult::ThresholdReached { value, count } => {
                    assert_eq!(value, high_value);
                    assert_eq!(count, 4);
                }
                _ => panic!("Expected threshold reached"),
            }
        }
    }
}

#[tokio::test]
async fn test_sequential_voting_rounds() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());

    let postgres = PostgresStorage::new(&config).await.unwrap();
    let etcd = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    let mut detector = ByzantineDetector::new(etcd, postgres);
    detector.etcd.set_cluster_threshold(4).await.unwrap();

    let tx_id = test_tx_id("sequential_001");

    // Round 1
    for i in 1..=4 {
        let (vote, _key) = create_signed_vote(NodeId(i), tx_id.clone(), 1, true, 100);
        detector.check_vote(&vote).await.unwrap();
    }

    // Verify round 1 consensus
    let state = detector.etcd.get_transaction_state(&tx_id).await.unwrap();
    assert_eq!(state, TransactionState::ThresholdReached);

    // Clean up for round 2
    detector.etcd.delete_all_votes(&tx_id).await.unwrap();
    detector.etcd.delete_all_vote_counts(&tx_id).await.unwrap();
    detector
        .etcd
        .set_transaction_state(&tx_id, TransactionState::Pending)
        .await
        .unwrap();

    // Round 2 with different value
    for i in 1..=4 {
        let (vote, _key) = create_signed_vote(NodeId(i), tx_id.clone(), 2, true, 200);
        detector.check_vote(&vote).await.unwrap();
    }

    let count = detector.etcd.get_vote_count(&tx_id, 200).await.unwrap();
    assert_eq!(count, 4);
}
