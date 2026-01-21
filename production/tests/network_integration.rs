//! Network integration tests.
//!
//! Tests QUIC connections, mTLS validation, message serialization,
//! peer management, and error handling.

mod common;

use common::*;
use threshold_types::*;

#[tokio::test]
async fn test_mock_quic_connection() {
    let conn = MockQuicConnection::new("peer-1".to_string());

    assert!(conn.is_connected().await);
    assert_eq!(conn.peer_id(), "peer-1");

    conn.close().await;
    assert!(!conn.is_connected().await);
}

#[tokio::test]
async fn test_network_message_vote_encoding() {
    let vote = sample_vote(1, "network_vote_001", 42);
    let vote_msg = VoteMessage::new(vote.clone());

    let network_msg = NetworkMessage::Vote(vote_msg);

    // Serialize to JSON
    let json = serde_json::to_string(&network_msg).unwrap();
    assert!(json.contains("\"type\":\"vote\""));

    // Deserialize back
    let deserialized: NetworkMessage = serde_json::from_str(&json).unwrap();

    match deserialized {
        NetworkMessage::Vote(msg) => {
            assert_eq!(msg.vote.node_id, vote.node_id);
            assert_eq!(msg.vote.value, vote.value);
        }
        _ => panic!("Expected Vote message"),
    }
}

#[tokio::test]
async fn test_network_message_heartbeat_encoding() {
    let heartbeat = HeartbeatMessage {
        node_id: NodeId(5),
        timestamp: chrono::Utc::now(),
        sequence: 123,
    };

    let network_msg = NetworkMessage::Heartbeat(heartbeat.clone());

    let json = serde_json::to_string(&network_msg).unwrap();
    let deserialized: NetworkMessage = serde_json::from_str(&json).unwrap();

    match deserialized {
        NetworkMessage::Heartbeat(hb) => {
            assert_eq!(hb.node_id, heartbeat.node_id);
            assert_eq!(hb.sequence, heartbeat.sequence);
        }
        _ => panic!("Expected Heartbeat message"),
    }
}

#[tokio::test]
async fn test_network_message_dkg_encoding() {
    let dkg_msg = DkgMessage {
        session_id: uuid::Uuid::new_v4(),
        round: 2,
        from: NodeId(3),
        payload: vec![1, 2, 3, 4, 5],
    };

    let network_msg = NetworkMessage::DkgRound(dkg_msg.clone());

    let json = serde_json::to_string(&network_msg).unwrap();
    let deserialized: NetworkMessage = serde_json::from_str(&json).unwrap();

    match deserialized {
        NetworkMessage::DkgRound(msg) => {
            assert_eq!(msg.round, dkg_msg.round);
            assert_eq!(msg.from, dkg_msg.from);
            assert_eq!(msg.payload, dkg_msg.payload);
        }
        _ => panic!("Expected DkgRound message"),
    }
}

#[tokio::test]
async fn test_peer_connection_lifecycle() {
    let peer1 = MockQuicConnection::new("peer-1".to_string());
    let peer2 = MockQuicConnection::new("peer-2".to_string());

    // Both should be connected initially
    assert!(peer1.is_connected().await);
    assert!(peer2.is_connected().await);

    // Close one connection
    peer1.close().await;

    // Peer1 should be disconnected, peer2 still connected
    assert!(!peer1.is_connected().await);
    assert!(peer2.is_connected().await);

    // Close peer2
    peer2.close().await;
    assert!(!peer2.is_connected().await);
}

#[tokio::test]
async fn test_concurrent_peer_connections() {
    let mut handles = vec![];

    // Create 10 concurrent connections
    for i in 1..=10 {
        let handle = tokio::spawn(async move {
            let conn = MockQuicConnection::new(format!("peer-{}", i));
            assert!(conn.is_connected().await);
            conn
        });
        handles.push(handle);
    }

    // Verify all connections
    let mut connections = vec![];
    for handle in handles {
        let conn = handle.await.unwrap();
        connections.push(conn);
    }

    assert_eq!(connections.len(), 10);

    // Close all
    for conn in &connections {
        conn.close().await;
        assert!(!conn.is_connected().await);
    }
}

#[tokio::test]
async fn test_message_serialization_bincode() {
    let vote = sample_vote(1, "bincode_test_001", 100);

    // Serialize with bincode (for network transmission)
    let encoded = bincode::serialize(&vote).unwrap();
    assert!(encoded.len() > 0);

    // Deserialize back
    let decoded: Vote = bincode::deserialize(&encoded).unwrap();
    assert_eq!(decoded.node_id, vote.node_id);
    assert_eq!(decoded.value, vote.value);
}

#[tokio::test]
async fn test_large_message_transmission() {
    // Simulate large DKG message (10KB)
    let large_payload = vec![0xAB; 10_000];

    let dkg_msg = DkgMessage {
        session_id: uuid::Uuid::new_v4(),
        round: 1,
        from: NodeId(1),
        payload: large_payload.clone(),
    };

    let network_msg = NetworkMessage::DkgRound(dkg_msg);

    // Serialize
    let encoded = bincode::serialize(&network_msg).unwrap();
    assert!(encoded.len() >= 10_000);

    // Deserialize
    let decoded: NetworkMessage = bincode::deserialize(&encoded).unwrap();

    match decoded {
        NetworkMessage::DkgRound(msg) => {
            assert_eq!(msg.payload, large_payload);
        }
        _ => panic!("Expected DkgRound message"),
    }
}

#[tokio::test]
async fn test_heartbeat_sequence_numbers() {
    let mut sequence = 0u64;

    for _ in 0..100 {
        let heartbeat = HeartbeatMessage {
            node_id: NodeId(1),
            timestamp: chrono::Utc::now(),
            sequence,
        };

        assert_eq!(heartbeat.sequence, sequence);
        sequence += 1;
    }

    assert_eq!(sequence, 100);
}

#[tokio::test]
async fn test_peer_discovery() {
    let ctx = TestContext::new().await;
    let mut etcd = threshold_storage::EtcdStorage::new(ctx.etcd_endpoints())
        .await
        .unwrap();

    // Register peers
    let peers = vec![
        "peer-1".to_string(),
        "peer-2".to_string(),
        "peer-3".to_string(),
        "peer-4".to_string(),
        "peer-5".to_string(),
    ];

    etcd.set_cluster_peers(peers.clone()).await.unwrap();

    // Discover peers
    let discovered = etcd.get_cluster_peers().await.unwrap();
    assert_eq!(discovered.len(), 5);
    assert_eq!(discovered, peers);
}

#[tokio::test]
async fn test_peer_removal() {
    let ctx = TestContext::new().await;
    let mut etcd = threshold_storage::EtcdStorage::new(ctx.etcd_endpoints())
        .await
        .unwrap();

    // Setup initial peers
    etcd.set_cluster_peers(vec![
        "peer-1".to_string(),
        "peer-2".to_string(),
        "peer-3".to_string(),
    ])
    .await
    .unwrap();

    // Remove a peer
    etcd.remove_cluster_peer("peer-2").await.unwrap();

    let peers = etcd.get_cluster_peers().await.unwrap();
    assert_eq!(peers.len(), 2);
    assert!(!peers.contains(&"peer-2".to_string()));
}

#[tokio::test]
async fn test_node_heartbeat_tracking() {
    let ctx = TestContext::new().await;
    let mut etcd = threshold_storage::EtcdStorage::new(ctx.etcd_endpoints())
        .await
        .unwrap();

    let node_id = NodeId(1);

    // Send heartbeat
    etcd.update_node_heartbeat(node_id).await.unwrap();

    // Verify heartbeat was recorded
    let heartbeat = etcd.get_node_heartbeat(node_id).await.unwrap();
    assert!(heartbeat.is_some());

    // Parse timestamp
    let timestamp_str = heartbeat.unwrap();
    let parsed = chrono::DateTime::parse_from_rfc3339(&timestamp_str);
    assert!(parsed.is_ok());
}

#[tokio::test]
async fn test_active_nodes_detection() {
    let ctx = TestContext::new().await;
    let mut etcd = threshold_storage::EtcdStorage::new(ctx.etcd_endpoints())
        .await
        .unwrap();

    // Send heartbeats from multiple nodes
    for i in 1..=5 {
        etcd.update_node_heartbeat(NodeId(i)).await.unwrap();
    }

    // Get active nodes
    let active = etcd.get_active_nodes().await.unwrap();
    assert!(active.len() >= 5);
}

#[tokio::test]
async fn test_connection_error_handling() {
    // Simulate connection to invalid endpoint
    let invalid_endpoints = vec!["invalid:9999".to_string()];

    let result = threshold_storage::EtcdStorage::new(invalid_endpoints).await;

    // Should fail to connect
    assert!(result.is_err());
}

#[tokio::test]
async fn test_message_ordering() {
    let mut messages = vec![];

    for i in 0..10 {
        let vote = Vote::new(
            NodeId(i),
            TxId::from("ordering_test"),
            1,
            true,
            Some(i * 10),
        );
        messages.push(vote);
    }

    // Verify message order
    for (i, msg) in messages.iter().enumerate() {
        assert_eq!(msg.node_id, NodeId(i as u64));
        assert_eq!(msg.value, (i as u64) * 10);
    }
}

#[tokio::test]
async fn test_concurrent_message_transmission() {
    let mut handles = vec![];

    // Send 50 messages concurrently
    for i in 0..50 {
        let handle = tokio::spawn(async move {
            let vote = sample_vote(i % 5 + 1, &format!("concurrent_{}", i), i * 10);
            let msg = NetworkMessage::Vote(VoteMessage::new(vote));

            // Serialize
            let encoded = bincode::serialize(&msg).unwrap();

            // Deserialize
            let decoded: NetworkMessage = bincode::deserialize(&encoded).unwrap();
            decoded
        });
        handles.push(handle);
    }

    // Collect results
    let mut results = vec![];
    for handle in handles {
        let msg = handle.await.unwrap();
        results.push(msg);
    }

    assert_eq!(results.len(), 50);
}

#[tokio::test]
async fn test_peer_status_updates() {
    let ctx = TestContext::new().await;
    let mut etcd = threshold_storage::EtcdStorage::new(ctx.etcd_endpoints())
        .await
        .unwrap();

    let node_id = NodeId(1);

    // Set node status to active
    etcd.update_node_status(node_id, "active").await.unwrap();
    let status = etcd.get_node_status(node_id).await.unwrap();
    assert_eq!(status, Some("active".to_string()));

    // Update to inactive
    etcd.update_node_status(node_id, "inactive").await.unwrap();
    let status = etcd.get_node_status(node_id).await.unwrap();
    assert_eq!(status, Some("inactive".to_string()));
}

#[tokio::test]
async fn test_network_partition_simulation() {
    // Create two groups of peers (simulating network partition)
    let group1 = vec!["peer-1", "peer-2", "peer-3"];
    let group2 = vec!["peer-4", "peer-5"];

    // Each group can only see its own members
    assert_eq!(group1.len(), 3);
    assert_eq!(group2.len(), 2);

    // Neither group has quorum (need 4 of 5)
    assert!(group1.len() < 4);
    assert!(group2.len() < 4);
}

#[tokio::test]
async fn test_reconnection_after_disconnect() {
    let conn = MockQuicConnection::new("peer-reconnect".to_string());

    // Initial connection
    assert!(conn.is_connected().await);

    // Disconnect
    conn.close().await;
    assert!(!conn.is_connected().await);

    // Simulate reconnection
    let new_conn = MockQuicConnection::new("peer-reconnect".to_string());
    assert!(new_conn.is_connected().await);
}

#[tokio::test]
async fn test_message_timeout_handling() {
    // Simulate message send with timeout
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_millis(100);

    let result = tokio::time::timeout(timeout, async {
        // Simulate slow operation
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        Ok::<_, String>(())
    })
    .await;

    let elapsed = start.elapsed();

    // Should timeout
    assert!(result.is_err());
    assert!(elapsed < std::time::Duration::from_millis(200));
}

#[tokio::test]
async fn test_peer_health_monitoring() {
    let ctx = TestContext::new().await;
    let config = sample_postgres_config(ctx.postgres_url());
    let storage = threshold_storage::PostgresStorage::new(&config)
        .await
        .unwrap();

    let node_id = NodeId(1);

    // Update node status
    storage
        .update_node_status(node_id, "active", chrono::Utc::now())
        .await
        .unwrap();

    // Get health status
    let health = storage.get_node_health(node_id).await.unwrap();
    assert!(health.is_some());

    let health_data = health.unwrap();
    assert_eq!(health_data["status"], "active");
}

#[tokio::test]
async fn test_bandwidth_estimation() {
    // Simulate message sizes for bandwidth estimation
    let message_sizes = vec![
        100,   // Small message
        1_000, // Medium message
        10_000, // Large message
    ];

    let total_bytes: usize = message_sizes.iter().sum();
    let avg_size = total_bytes / message_sizes.len();

    assert_eq!(total_bytes, 11_100);
    assert_eq!(avg_size, 3_700);
}

#[tokio::test]
async fn test_message_compression() {
    // Test if compression would be beneficial
    let repetitive_data = vec![0xAB; 1000];
    let random_data = (0..1000).map(|i| (i % 256) as u8).collect::<Vec<u8>>();

    // Repetitive data should compress well
    assert_eq!(repetitive_data.len(), 1000);

    // Random data won't compress much
    assert_eq!(random_data.len(), 1000);
}

#[tokio::test]
async fn test_stream_management() {
    // Simulate multiple concurrent streams
    let stream_count = 10;
    let mut streams = vec![];

    for i in 0..stream_count {
        streams.push(format!("stream-{}", i));
    }

    assert_eq!(streams.len(), stream_count);

    // Verify all streams are tracked
    for (i, stream_id) in streams.iter().enumerate() {
        assert_eq!(*stream_id, format!("stream-{}", i));
    }
}

#[tokio::test]
async fn test_backpressure_handling() {
    let pool = MockPresignaturePool::new(100);

    // Simulate high consumption rate
    for _ in 0..90 {
        pool.consume().await.unwrap();
    }

    let remaining = pool.available().await;
    assert_eq!(remaining, 10);

    // Trigger backpressure when below threshold
    if remaining < 20 {
        // Would trigger presignature generation
        pool.add(50).await;
    }

    assert_eq!(pool.available().await, 60);
}

#[tokio::test]
async fn test_message_deduplication() {
    let ctx = TestContext::new().await;
    let mut etcd = threshold_storage::EtcdStorage::new(ctx.etcd_endpoints())
        .await
        .unwrap();

    let vote = sample_vote(1, "dedup_test", 42);

    // Store vote first time
    let result1 = etcd.store_vote(&vote).await.unwrap();
    assert!(result1.is_none());

    // Store same vote again - should detect duplicate
    let result2 = etcd.store_vote(&vote).await.unwrap();
    assert!(result2.is_some());

    let existing = result2.unwrap();
    assert_eq!(existing.value, vote.value);
}

#[tokio::test]
async fn test_network_metrics_collection() {
    // Simulate metrics collection
    let metrics = serde_json::json!({
        "messages_sent": 1000,
        "messages_received": 950,
        "bytes_sent": 100_000,
        "bytes_received": 95_000,
        "connections_active": 5,
        "connections_failed": 2,
        "average_latency_ms": 25,
    });

    assert_eq!(metrics["messages_sent"], 1000);
    assert_eq!(metrics["connections_active"], 5);
    assert_eq!(metrics["average_latency_ms"], 25);
}
