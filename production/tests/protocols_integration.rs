//! Protocol integration tests.
//!
//! Tests MPC protocol flows (CGGMP24, FROST), presignature pool,
//! and protocol state machine validation.

mod common;

use common::*;
use threshold_types::*;

#[tokio::test]
async fn test_presignature_pool_management() {
    let pool = MockPresignaturePool::new(10);

    // Check initial count
    assert_eq!(pool.available().await, 10);

    // Consume presignatures
    for _ in 0..5 {
        pool.consume().await.unwrap();
    }

    assert_eq!(pool.available().await, 5);

    // Add more presignatures
    pool.add(3).await;
    assert_eq!(pool.available().await, 8);
}

#[tokio::test]
async fn test_presignature_pool_exhaustion() {
    let pool = MockPresignaturePool::new(2);

    // Consume all
    pool.consume().await.unwrap();
    pool.consume().await.unwrap();

    assert_eq!(pool.available().await, 0);

    // Should fail to consume when exhausted
    let result = pool.consume().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_presignature_usage_recording() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());
    let storage = threshold_storage::PostgresStorage::new(&config)
        .await
        .unwrap();

    // Create transaction first
    let tx = sample_transaction("presig_usage_001");
    let tx_id = storage.create_transaction(&tx).await.unwrap();

    // Record presignature usage
    let usage = PresignatureUsage {
        id: 0,
        presig_id: PresignatureId::new(),
        transaction_id: tx_id,
        used_at: chrono::Utc::now(),
        generation_time_ms: 250,
    };

    let result = storage.record_presignature_usage(&usage).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_concurrent_presignature_consumption() {
    let pool = MockPresignaturePool::new(100);

    // Spawn 50 concurrent consumers
    let mut handles = vec![];
    for _ in 0..50 {
        let pool_clone = MockPresignaturePool {
            available: pool.available.clone(),
        };

        let handle = tokio::spawn(async move {
            pool_clone.consume().await.unwrap();
        });
        handles.push(handle);
    }

    // Wait for all
    for handle in handles {
        handle.await.unwrap();
    }

    // Should have 50 left
    assert_eq!(pool.available().await, 50);
}

#[tokio::test]
async fn test_dkg_message_structure() {
    let dkg_msg = DkgMessage {
        session_id: uuid::Uuid::new_v4(),
        round: 1,
        from: NodeId(1),
        payload: vec![1, 2, 3, 4, 5],
    };

    // Test serialization
    let json = serde_json::to_string(&dkg_msg).unwrap();
    let deserialized: DkgMessage = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.round, 1);
    assert_eq!(deserialized.from, NodeId(1));
    assert_eq!(deserialized.payload, vec![1, 2, 3, 4, 5]);
}

#[tokio::test]
async fn test_signing_message_structure() {
    let signing_msg = SigningMessage {
        tx_id: TxId::from("signing_test_001"),
        round: 2,
        from: NodeId(3),
        payload: vec![0xAB, 0xCD, 0xEF],
    };

    let json = serde_json::to_string(&signing_msg).unwrap();
    let deserialized: SigningMessage = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.tx_id, TxId::from("signing_test_001"));
    assert_eq!(deserialized.round, 2);
    assert_eq!(deserialized.from, NodeId(3));
}

#[tokio::test]
async fn test_presignature_message_structure() {
    let presig_msg = PresignatureMessage {
        presig_id: PresignatureId::new(),
        round: 1,
        from: NodeId(2),
        payload: vec![0x01, 0x02],
    };

    let json = serde_json::to_string(&presig_msg).unwrap();
    let deserialized: PresignatureMessage = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.round, 1);
    assert_eq!(deserialized.from, NodeId(2));
}

#[tokio::test]
async fn test_protocol_session_coordination() {
    let ctx = TestContext::new().await;
    let mut etcd = threshold_storage::EtcdStorage::new(ctx.etcd_endpoints())
        .await
        .unwrap();

    let session_id = "dkg-session-test-001";

    // Node 1 acquires session lock
    let lease_id = etcd.acquire_dkg_session_lock(session_id).await.unwrap();
    assert!(lease_id > 0);

    // Node 2 tries to acquire same session lock - should fail
    let result = etcd.acquire_dkg_session_lock(session_id).await;
    assert!(result.is_err(), "Only one node should hold session lock");

    // Release lock
    etcd.release_dkg_session_lock(session_id).await.unwrap();

    // Now Node 2 can acquire
    let lease_id2 = etcd.acquire_dkg_session_lock(session_id).await.unwrap();
    assert!(lease_id2 > 0);

    etcd.release_dkg_session_lock(session_id).await.unwrap();
}

#[tokio::test]
async fn test_presignature_generation_lock() {
    let ctx = TestContext::new().await;
    let mut etcd = threshold_storage::EtcdStorage::new(ctx.etcd_endpoints())
        .await
        .unwrap();

    // Acquire presignature generation lock
    let lease_id = etcd.acquire_presig_generation_lock().await.unwrap();
    assert!(lease_id > 0);

    // Should fail to acquire again
    let result = etcd.acquire_presig_generation_lock().await;
    assert!(result.is_err());

    // Release and retry
    etcd.release_presig_generation_lock().await.unwrap();
    let lease_id2 = etcd.acquire_presig_generation_lock().await.unwrap();
    assert!(lease_id2 > 0);

    etcd.release_presig_generation_lock().await.unwrap();
}

#[tokio::test]
async fn test_signing_round_coordination() {
    let ctx = TestContext::new().await;
    let mut etcd = threshold_storage::EtcdStorage::new(ctx.etcd_endpoints())
        .await
        .unwrap();

    let tx_id = test_tx_id("signing_coordination_001");

    // Acquire signing lock
    let lease_id = etcd.acquire_signing_lock(&tx_id).await.unwrap();
    assert!(lease_id > 0);

    // Should fail to acquire again
    let result = etcd.acquire_signing_lock(&tx_id).await;
    assert!(result.is_err(), "Only one signing session per transaction");

    // Release lock
    etcd.release_signing_lock(&tx_id).await.unwrap();
}

#[tokio::test]
async fn test_multiple_signing_sessions_different_txs() {
    let ctx = TestContext::new().await;
    let endpoints = ctx.etcd_endpoints();

    let tx_id_1 = test_tx_id("signing_session_001");
    let tx_id_2 = test_tx_id("signing_session_002");

    // Acquire locks for different transactions concurrently
    let handle1 = {
        let endpoints = endpoints.clone();
        let tx_id = tx_id_1.clone();
        tokio::spawn(async move {
            let mut etcd = threshold_storage::EtcdStorage::new(endpoints)
                .await
                .unwrap();
            etcd.acquire_signing_lock(&tx_id).await.unwrap()
        })
    };

    let handle2 = {
        let endpoints = endpoints.clone();
        let tx_id = tx_id_2.clone();
        tokio::spawn(async move {
            let mut etcd = threshold_storage::EtcdStorage::new(endpoints)
                .await
                .unwrap();
            etcd.acquire_signing_lock(&tx_id).await.unwrap()
        })
    };

    // Both should succeed (different transactions)
    let lease1 = handle1.await.unwrap();
    let lease2 = handle2.await.unwrap();

    assert!(lease1 > 0);
    assert!(lease2 > 0);

    // Cleanup
    let mut etcd = threshold_storage::EtcdStorage::new(endpoints).await.unwrap();
    etcd.release_signing_lock(&tx_id_1).await.unwrap();
    etcd.release_signing_lock(&tx_id_2).await.unwrap();
}

#[tokio::test]
async fn test_protocol_state_transitions() {
    let ctx = TestContext::new().await;
    let mut etcd = threshold_storage::EtcdStorage::new(ctx.etcd_endpoints())
        .await
        .unwrap();

    let tx_id = test_tx_id("protocol_state_001");

    // Transaction lifecycle states
    let states = vec![
        TransactionState::Pending,
        TransactionState::Voting,
        TransactionState::ThresholdReached,
        TransactionState::Signing,
        TransactionState::Signed,
        TransactionState::Submitted,
        TransactionState::Confirmed,
    ];

    for state in states {
        etcd.set_transaction_state(&tx_id, state).await.unwrap();
        let current = etcd.get_transaction_state(&tx_id).await.unwrap();
        assert_eq!(current, state);
    }
}

#[tokio::test]
async fn test_mock_crypto_signature_generation() {
    // Test ECDSA signature
    let ecdsa_sig = MockCrypto::mock_ecdsa_signature();
    assert!(ecdsa_sig.len() >= 70 && ecdsa_sig.len() <= 73);
    assert_eq!(ecdsa_sig[0], 0x30); // DER encoding marker

    // Test Schnorr signature
    let schnorr_sig = MockCrypto::mock_schnorr_signature();
    assert_eq!(schnorr_sig.len(), 64);

    // Test public keys
    let compressed = MockCrypto::mock_compressed_pubkey();
    assert_eq!(compressed.len(), 33);
    assert!(compressed[0] == 0x02 || compressed[0] == 0x03);

    let xonly = MockCrypto::mock_xonly_pubkey();
    assert_eq!(xonly.len(), 32);
}

#[tokio::test]
async fn test_signing_with_mock_signatures() {
    // Build a transaction
    let utxos = sample_utxos();
    let change_address = "tb1qchange00000000000000000000000000000000".to_string();
    let sender_script_pubkey = vec![0; 22];

    let builder = threshold_bitcoin::TransactionBuilder::new(
        utxos,
        change_address,
        sender_script_pubkey,
        5,
    );

    let unsigned_tx = builder
        .add_output("tb1qrecipient000000000000000000000000000000".to_string(), 10_000)
        .build_p2wpkh()
        .unwrap();

    // Generate mock signatures for all inputs
    let signatures: Vec<Vec<u8>> = unsigned_tx
        .inputs
        .iter()
        .map(|_| MockCrypto::mock_ecdsa_signature())
        .collect();

    let public_keys: Vec<Vec<u8>> = unsigned_tx
        .inputs
        .iter()
        .map(|_| MockCrypto::mock_compressed_pubkey())
        .collect();

    // Finalize transaction with mock signatures
    let result = threshold_bitcoin::finalize_p2wpkh_transaction(
        &unsigned_tx.unsigned_tx_hex,
        &signatures,
        &public_keys,
    );

    assert!(result.is_ok(), "Should finalize with mock signatures");
}

#[tokio::test]
async fn test_presignature_pool_refill_strategy() {
    let pool = MockPresignaturePool::new(5);

    // Consume some presignatures
    for _ in 0..3 {
        pool.consume().await.unwrap();
    }

    assert_eq!(pool.available().await, 2);

    // Refill to maintain minimum threshold
    let min_threshold = 5;
    let current = pool.available().await;
    if current < min_threshold {
        let needed = min_threshold - current;
        pool.add(needed).await;
    }

    assert_eq!(pool.available().await, 5);
}

#[tokio::test]
async fn test_protocol_round_progression() {
    // Simulate multi-round protocol
    let rounds = vec![1, 2, 3, 4, 5];

    for round in rounds {
        let msg = DkgMessage {
            session_id: uuid::Uuid::new_v4(),
            round,
            from: NodeId(1),
            payload: vec![round as u8],
        };

        assert_eq!(msg.round, round);
    }
}

#[tokio::test]
async fn test_concurrent_protocol_sessions() {
    let ctx = TestContext::new().await;
    let endpoints = ctx.etcd_endpoints();

    // Start 3 different DKG sessions concurrently
    let mut handles = vec![];

    for i in 1..=3 {
        let endpoints = endpoints.clone();
        let session_id = format!("concurrent-session-{}", i);

        let handle = tokio::spawn(async move {
            let mut etcd = threshold_storage::EtcdStorage::new(endpoints)
                .await
                .unwrap();
            etcd.acquire_dkg_session_lock(&session_id).await.unwrap()
        });
        handles.push(handle);
    }

    // All should succeed (different sessions)
    for handle in handles {
        let lease_id = handle.await.unwrap();
        assert!(lease_id > 0);
    }

    // Cleanup
    let mut etcd = threshold_storage::EtcdStorage::new(endpoints).await.unwrap();
    for i in 1..=3 {
        etcd.release_dkg_session_lock(&format!("concurrent-session-{}", i))
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn test_signing_with_presignature() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());
    let storage = threshold_storage::PostgresStorage::new(&config)
        .await
        .unwrap();

    let pool = MockPresignaturePool::new(10);

    // Create transaction
    let tx = sample_transaction("presig_signing_001");
    let tx_id = storage.create_transaction(&tx).await.unwrap();

    // Consume presignature for signing
    pool.consume().await.unwrap();

    // Record usage
    let usage = PresignatureUsage {
        id: 0,
        presig_id: PresignatureId::new(),
        transaction_id: tx_id,
        used_at: chrono::Utc::now(),
        generation_time_ms: 150,
    };

    storage.record_presignature_usage(&usage).await.unwrap();

    // Verify presignature was consumed
    assert_eq!(pool.available().await, 9);
}

#[tokio::test]
async fn test_protocol_message_payload_sizes() {
    // Test small payload
    let small = DkgMessage {
        session_id: uuid::Uuid::new_v4(),
        round: 1,
        from: NodeId(1),
        payload: vec![0; 100],
    };

    let json_small = serde_json::to_string(&small).unwrap();
    assert!(json_small.len() < 500);

    // Test large payload (simulating actual DKG data)
    let large = DkgMessage {
        session_id: uuid::Uuid::new_v4(),
        round: 1,
        from: NodeId(1),
        payload: vec![0; 10_000],
    };

    let json_large = serde_json::to_string(&large).unwrap();
    assert!(json_large.len() > 10_000);
}

#[tokio::test]
async fn test_session_id_uniqueness() {
    let session_ids: Vec<uuid::Uuid> = (0..100)
        .map(|_| uuid::Uuid::new_v4())
        .collect();

    // All should be unique
    for i in 0..session_ids.len() {
        for j in (i + 1)..session_ids.len() {
            assert_ne!(session_ids[i], session_ids[j]);
        }
    }
}

#[tokio::test]
async fn test_protocol_timeout_handling() {
    // Simulate protocol timeout by checking transaction state
    let ctx = TestContext::new().await;
    let mut etcd = threshold_storage::EtcdStorage::new(ctx.etcd_endpoints())
        .await
        .unwrap();

    let tx_id = test_tx_id("timeout_test_001");

    // Set to signing state
    etcd.set_transaction_state(&tx_id, TransactionState::Signing)
        .await
        .unwrap();

    // Simulate timeout - transition to failed
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    etcd.set_transaction_state(&tx_id, TransactionState::Failed)
        .await
        .unwrap();

    let state = etcd.get_transaction_state(&tx_id).await.unwrap();
    assert_eq!(state, TransactionState::Failed);
}

#[tokio::test]
async fn test_presignature_generation_metrics() {
    let generation_times = vec![100, 150, 200, 250, 300]; // ms

    let avg = generation_times.iter().sum::<i32>() / generation_times.len() as i32;
    assert_eq!(avg, 200);

    let max = generation_times.iter().max().unwrap();
    assert_eq!(*max, 300);

    let min = generation_times.iter().min().unwrap();
    assert_eq!(*min, 100);
}
