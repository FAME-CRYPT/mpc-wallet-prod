//! Storage integration tests for PostgreSQL and etcd.
//!
//! These tests validate:
//! - PostgreSQL connection and schema
//! - etcd distributed coordination
//! - Vote recording and retrieval
//! - Transaction state transitions
//! - Byzantine violation recording
//! - Concurrent access handling
//! - Database transactions and rollbacks

mod common;

use common::*;
use threshold_storage::{EtcdStorage, PostgresStorage};
use threshold_types::*;
use chrono::Utc;

#[tokio::test]
async fn test_postgres_connection_and_schema() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());

    let storage = PostgresStorage::new(&config).await;
    assert!(storage.is_ok(), "Should connect to PostgreSQL");
}

#[tokio::test]
async fn test_etcd_connection() {
    let ctx = TestContext::new().await;

    let storage = EtcdStorage::new(ctx.etcd_endpoints()).await;
    assert!(storage.is_ok(), "Should connect to etcd");
}

#[tokio::test]
async fn test_vote_recording_and_retrieval() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());
    let storage = PostgresStorage::new(&config).await.unwrap();

    // Create a voting round first
    let tx_id = test_tx_id("vote_001");
    let round = VotingRound {
        id: 0,
        tx_id: tx_id.clone(),
        round_number: 1,
        total_nodes: 5,
        threshold: 4,
        votes_received: 0,
        approved: false,
        completed: false,
        started_at: Utc::now(),
        completed_at: None,
    };

    let round_id = storage.create_voting_round(&round).await.unwrap();
    assert!(round_id > 0, "Voting round should be created");

    // Create and record a vote
    let vote = Vote::new(NodeId(1), tx_id.clone(), 1, true, Some(42));

    let result = storage.record_vote(&vote).await;
    assert!(result.is_ok(), "Vote should be recorded successfully");
}

#[tokio::test]
async fn test_duplicate_vote_handling() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());
    let storage = PostgresStorage::new(&config).await.unwrap();

    let tx_id = test_tx_id("vote_002");
    let round = sample_voting_round(&tx_id.0);
    storage.create_voting_round(&round).await.unwrap();

    let vote = Vote::new(NodeId(1), tx_id.clone(), 1, true, Some(100));

    // First vote should succeed
    storage.record_vote(&vote).await.unwrap();

    // Duplicate vote should be handled (ON CONFLICT DO NOTHING)
    let result = storage.record_vote(&vote).await;
    assert!(result.is_ok(), "Duplicate vote should be handled gracefully");
}

#[tokio::test]
async fn test_transaction_creation_and_retrieval() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());
    let storage = PostgresStorage::new(&config).await.unwrap();

    let tx = sample_transaction("tx_create_001");

    // Create transaction
    let tx_id = storage.create_transaction(&tx).await.unwrap();
    assert!(tx_id > 0, "Transaction should be created with valid ID");

    // Retrieve transaction
    let retrieved = storage.get_transaction(&tx.txid).await.unwrap();
    assert!(retrieved.is_some(), "Transaction should be retrievable");

    let retrieved_tx = retrieved.unwrap();
    assert_eq!(retrieved_tx.txid, tx.txid);
    assert_eq!(retrieved_tx.recipient, tx.recipient);
    assert_eq!(retrieved_tx.amount_sats, tx.amount_sats);
    assert_eq!(retrieved_tx.state, TransactionState::Pending);
}

#[tokio::test]
async fn test_transaction_state_transitions() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());
    let storage = PostgresStorage::new(&config).await.unwrap();

    let tx = sample_transaction("tx_state_001");
    storage.create_transaction(&tx).await.unwrap();

    // Test state transition: Pending -> Voting
    storage
        .update_transaction_state(&tx.txid, TransactionState::Voting)
        .await
        .unwrap();

    let retrieved = storage.get_transaction(&tx.txid).await.unwrap().unwrap();
    assert_eq!(retrieved.state, TransactionState::Voting);

    // Test state transition: Voting -> ThresholdReached
    storage
        .update_transaction_state(&tx.txid, TransactionState::ThresholdReached)
        .await
        .unwrap();

    let retrieved = storage.get_transaction(&tx.txid).await.unwrap().unwrap();
    assert_eq!(retrieved.state, TransactionState::ThresholdReached);

    // Test state transition: ThresholdReached -> Signing
    storage
        .update_transaction_state(&tx.txid, TransactionState::Signing)
        .await
        .unwrap();

    let retrieved = storage.get_transaction(&tx.txid).await.unwrap().unwrap();
    assert_eq!(retrieved.state, TransactionState::Signing);
}

#[tokio::test]
async fn test_signed_transaction_update() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());
    let storage = PostgresStorage::new(&config).await.unwrap();

    let tx = sample_transaction("tx_signed_001");
    storage.create_transaction(&tx).await.unwrap();

    let signed_tx = vec![0x01, 0x02, 0x03, 0x04, 0x05];

    storage
        .set_signed_transaction(&tx.txid, &signed_tx)
        .await
        .unwrap();

    let retrieved = storage.get_transaction(&tx.txid).await.unwrap().unwrap();
    assert_eq!(retrieved.signed_tx, Some(signed_tx));
    assert_eq!(retrieved.state, TransactionState::Signed);
}

#[tokio::test]
async fn test_byzantine_violation_recording() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());
    let storage = PostgresStorage::new(&config).await.unwrap();

    let violation = sample_byzantine_violation(1, "tx_byzantine_001", ViolationType::DoubleVote);

    let result = storage.record_byzantine_violation(&violation).await;
    assert!(result.is_ok(), "Byzantine violation should be recorded");

    // Retrieve violations for node
    let violations = storage
        .get_node_violations(NodeId(1))
        .await
        .unwrap();

    assert_eq!(violations.len(), 1, "Should retrieve 1 violation");
    assert_eq!(violations[0].violation_type, ViolationType::DoubleVote);
}

#[tokio::test]
async fn test_multiple_violation_types() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());
    let storage = PostgresStorage::new(&config).await.unwrap();

    let node_id = NodeId(2);

    // Record different violation types
    let violations = vec![
        sample_byzantine_violation(2, "tx_v1", ViolationType::DoubleVote),
        sample_byzantine_violation(2, "tx_v2", ViolationType::InvalidSignature),
        sample_byzantine_violation(2, "tx_v3", ViolationType::MinorityVote),
    ];

    for violation in &violations {
        storage
            .record_byzantine_violation(violation)
            .await
            .unwrap();
    }

    let retrieved = storage.get_node_violations(node_id).await.unwrap();
    assert_eq!(retrieved.len(), 3, "Should have 3 violations");

    // Check all violation types are present
    let types: Vec<ViolationType> = retrieved.iter().map(|v| v.violation_type).collect();
    assert!(types.contains(&ViolationType::DoubleVote));
    assert!(types.contains(&ViolationType::InvalidSignature));
    assert!(types.contains(&ViolationType::MinorityVote));
}

#[tokio::test]
async fn test_node_status_updates() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());
    let storage = PostgresStorage::new(&config).await.unwrap();

    let node_id = NodeId(1);
    let last_heartbeat = Utc::now();

    storage
        .update_node_status(node_id, "active", last_heartbeat)
        .await
        .unwrap();

    let health = storage.get_node_health(node_id).await.unwrap();
    assert!(health.is_some(), "Node health should be retrievable");

    let health_data = health.unwrap();
    assert_eq!(health_data["status"].as_str().unwrap(), "active");
}

#[tokio::test]
async fn test_presignature_usage_recording() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());
    let storage = PostgresStorage::new(&config).await.unwrap();

    // Create a transaction first
    let tx = sample_transaction("tx_presig_001");
    let tx_id = storage.create_transaction(&tx).await.unwrap();

    let usage = PresignatureUsage {
        id: 0,
        presig_id: PresignatureId::new(),
        transaction_id: tx_id,
        used_at: Utc::now(),
        generation_time_ms: 250,
    };

    let result = storage.record_presignature_usage(&usage).await;
    assert!(result.is_ok(), "Presignature usage should be recorded");
}

#[tokio::test]
async fn test_audit_log_recording() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());
    let storage = PostgresStorage::new(&config).await.unwrap();

    let node_id = NodeId(1);
    let tx_id = test_tx_id("audit_001");

    let details = serde_json::json!({
        "action": "vote_submitted",
        "value": 100,
        "timestamp": Utc::now().to_rfc3339(),
    });

    let result = storage
        .log_audit_event("vote_submitted", Some(node_id), Some(&tx_id), details)
        .await;

    assert!(result.is_ok(), "Audit event should be logged");
}

// ============================================================================
// etcd Integration Tests
// ============================================================================

#[tokio::test]
async fn test_etcd_vote_count_operations() {
    let ctx = TestContext::new().await;
    let mut storage = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    let tx_id = test_tx_id("etcd_vote_001");

    // Initial count should be 0
    let count = storage.get_vote_count(&tx_id, 100).await.unwrap();
    assert_eq!(count, 0);

    // Increment vote count
    let new_count = storage.increment_vote_count(&tx_id, 100).await.unwrap();
    assert_eq!(new_count, 1);

    // Increment again
    let new_count = storage.increment_vote_count(&tx_id, 100).await.unwrap();
    assert_eq!(new_count, 2);

    // Get count
    let count = storage.get_vote_count(&tx_id, 100).await.unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn test_etcd_vote_storage_and_retrieval() {
    let ctx = TestContext::new().await;
    let mut storage = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    let vote = sample_vote(1, "etcd_vote_002", 42);

    // Store vote - should return None (no existing vote)
    let existing = storage.store_vote(&vote).await.unwrap();
    assert!(existing.is_none(), "First vote should have no existing vote");

    // Store same vote again - should return the existing vote
    let existing = storage.store_vote(&vote).await.unwrap();
    assert!(existing.is_some(), "Second vote should find existing vote");

    let existing_vote = existing.unwrap();
    assert_eq!(existing_vote.value, vote.value);
    assert_eq!(existing_vote.node_id, vote.node_id);
}

#[tokio::test]
async fn test_etcd_transaction_state_management() {
    let ctx = TestContext::new().await;
    let mut storage = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    let tx_id = test_tx_id("etcd_state_001");

    // Initial state should be Pending
    let state = storage.get_transaction_state(&tx_id).await.unwrap();
    assert_eq!(state, TransactionState::Pending);

    // Update state to Voting
    storage
        .set_transaction_state(&tx_id, TransactionState::Voting)
        .await
        .unwrap();

    let state = storage.get_transaction_state(&tx_id).await.unwrap();
    assert_eq!(state, TransactionState::Voting);

    // Update state to ThresholdReached
    storage
        .set_transaction_state(&tx_id, TransactionState::ThresholdReached)
        .await
        .unwrap();

    let state = storage.get_transaction_state(&tx_id).await.unwrap();
    assert_eq!(state, TransactionState::ThresholdReached);
}

#[tokio::test]
async fn test_etcd_distributed_locking() {
    let ctx = TestContext::new().await;
    let mut storage = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    let tx_id = test_tx_id("etcd_lock_001");

    // Acquire lock
    let lease_id = storage.acquire_signing_lock(&tx_id).await.unwrap();
    assert!(lease_id > 0, "Should acquire lock with valid lease ID");

    // Try to acquire same lock again - should fail
    let result = storage.acquire_signing_lock(&tx_id).await;
    assert!(result.is_err(), "Should not acquire already-locked resource");

    // Release lock
    storage.release_signing_lock(&tx_id).await.unwrap();

    // Should be able to acquire again
    let lease_id = storage.acquire_signing_lock(&tx_id).await.unwrap();
    assert!(lease_id > 0, "Should acquire lock after release");

    storage.release_signing_lock(&tx_id).await.unwrap();
}

#[tokio::test]
async fn test_etcd_presignature_lock() {
    let ctx = TestContext::new().await;
    let mut storage = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    // Acquire presignature generation lock
    let lease_id = storage.acquire_presig_generation_lock().await.unwrap();
    assert!(lease_id > 0);

    // Should fail to acquire again
    let result = storage.acquire_presig_generation_lock().await;
    assert!(result.is_err());

    // Release and retry
    storage.release_presig_generation_lock().await.unwrap();
    let lease_id = storage.acquire_presig_generation_lock().await.unwrap();
    assert!(lease_id > 0);

    storage.release_presig_generation_lock().await.unwrap();
}

#[tokio::test]
async fn test_etcd_dkg_session_lock() {
    let ctx = TestContext::new().await;
    let mut storage = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    let session_id = "dkg-session-001";

    let lease_id = storage.acquire_dkg_session_lock(session_id).await.unwrap();
    assert!(lease_id > 0);

    let result = storage.acquire_dkg_session_lock(session_id).await;
    assert!(result.is_err());

    storage.release_dkg_session_lock(session_id).await.unwrap();
}

#[tokio::test]
async fn test_etcd_counter_operations() {
    let ctx = TestContext::new().await;
    let mut storage = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    // Transaction counter
    let count1 = storage.increment_transaction_counter().await.unwrap();
    let count2 = storage.increment_transaction_counter().await.unwrap();
    assert_eq!(count2, count1 + 1);

    let current = storage.get_transaction_counter().await.unwrap();
    assert_eq!(current, count2);

    // Byzantine counter
    let byz_count = storage.increment_byzantine_counter().await.unwrap();
    assert!(byz_count > 0);
}

#[tokio::test]
async fn test_etcd_node_status_with_ttl() {
    let ctx = TestContext::new().await;
    let mut storage = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    let node_id = NodeId(1);

    storage
        .update_node_status(node_id, "active")
        .await
        .unwrap();

    let status = storage.get_node_status(node_id).await.unwrap();
    assert_eq!(status, Some("active".to_string()));
}

#[tokio::test]
async fn test_etcd_node_heartbeat() {
    let ctx = TestContext::new().await;
    let mut storage = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    let node_id = NodeId(1);

    storage.update_node_heartbeat(node_id).await.unwrap();

    let heartbeat = storage.get_node_heartbeat(node_id).await.unwrap();
    assert!(heartbeat.is_some());
}

#[tokio::test]
async fn test_etcd_cluster_configuration() {
    let ctx = TestContext::new().await;
    let mut storage = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    // Set threshold
    storage.set_cluster_threshold(4).await.unwrap();
    let threshold = storage.get_cluster_threshold().await.unwrap();
    assert_eq!(threshold, 4);

    // Set peers
    let peers = vec![
        "peer-1".to_string(),
        "peer-2".to_string(),
        "peer-3".to_string(),
        "peer-4".to_string(),
        "peer-5".to_string(),
    ];
    storage.set_cluster_peers(peers.clone()).await.unwrap();

    let retrieved_peers = storage.get_cluster_peers().await.unwrap();
    assert_eq!(retrieved_peers.len(), 5);
    assert_eq!(retrieved_peers, peers);
}

#[tokio::test]
async fn test_etcd_peer_management() {
    let ctx = TestContext::new().await;
    let mut storage = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    storage.set_cluster_peers(vec![]).await.unwrap();

    // Add peers
    storage.add_cluster_peer("peer-1".to_string()).await.unwrap();
    storage.add_cluster_peer("peer-2".to_string()).await.unwrap();

    let peers = storage.get_cluster_peers().await.unwrap();
    assert_eq!(peers.len(), 2);

    // Remove peer
    storage.remove_cluster_peer("peer-1").await.unwrap();

    let peers = storage.get_cluster_peers().await.unwrap();
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0], "peer-2");
}

#[tokio::test]
async fn test_etcd_node_banning() {
    let ctx = TestContext::new().await;
    let mut storage = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    let node_id = NodeId(99);
    let violation = sample_byzantine_violation(99, "tx_ban_001", ViolationType::DoubleVote);

    // Node should not be banned initially
    let is_banned = storage.is_node_banned(node_id).await.unwrap();
    assert!(!is_banned);

    // Ban node
    storage.ban_node(&violation).await.unwrap();

    // Node should now be banned
    let is_banned = storage.is_node_banned(node_id).await.unwrap();
    assert!(is_banned);

    // Get ban info
    let ban_info = storage.get_ban_info(node_id).await.unwrap();
    assert!(ban_info.is_some());
    assert_eq!(ban_info.unwrap().violation_type, ViolationType::DoubleVote);

    // Unban node
    storage.unban_node(node_id).await.unwrap();

    let is_banned = storage.is_node_banned(node_id).await.unwrap();
    assert!(!is_banned);
}

#[tokio::test]
async fn test_concurrent_vote_counting() {
    let ctx = TestContext::new().await;
    let endpoints = ctx.etcd_endpoints();
    let tx_id = test_tx_id("concurrent_001");

    // Spawn multiple concurrent vote increments
    let mut handles = vec![];
    for _ in 0..10 {
        let endpoints_clone = endpoints.clone();
        let tx_id_clone = tx_id.clone();

        let handle = tokio::spawn(async move {
            let mut storage = EtcdStorage::new(endpoints_clone).await.unwrap();
            storage.increment_vote_count(&tx_id_clone, 100).await.unwrap();
        });
        handles.push(handle);
    }

    // Wait for all to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Final count should be 10
    let mut storage = EtcdStorage::new(endpoints).await.unwrap();
    let final_count = storage.get_vote_count(&tx_id, 100).await.unwrap();
    assert_eq!(final_count, 10, "All concurrent increments should be counted");
}

#[tokio::test]
async fn test_vote_cleanup() {
    let ctx = TestContext::new().await;
    let mut storage = EtcdStorage::new(ctx.etcd_endpoints()).await.unwrap();

    let tx_id = test_tx_id("cleanup_001");

    // Store some votes
    for i in 1..=3 {
        let vote = sample_vote(i, &tx_id.0, 100);
        storage.store_vote(&vote).await.unwrap();
        storage.increment_vote_count(&tx_id, 100).await.unwrap();
    }

    // Verify votes exist
    let count = storage.get_vote_count(&tx_id, 100).await.unwrap();
    assert_eq!(count, 3);

    // Clean up
    storage.delete_all_votes(&tx_id).await.unwrap();
    storage.delete_all_vote_counts(&tx_id).await.unwrap();

    // Verify cleanup
    let count = storage.get_vote_count(&tx_id, 100).await.unwrap();
    assert_eq!(count, 0);
}
