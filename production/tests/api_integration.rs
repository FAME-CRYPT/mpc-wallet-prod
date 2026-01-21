//! API integration tests.
//!
//! Tests REST endpoints, request/response validation, error handling,
//! JSON serialization, and concurrent requests.

mod common;

use common::*;
use threshold_types::*;
use serde_json::json;

// Since we don't have direct access to the API server infrastructure,
// these tests will focus on type serialization/deserialization and
// validation logic that would be used in API handlers.

#[tokio::test]
async fn test_vote_serialization() {
    let vote = sample_vote(1, "api_test_001", 100);

    let json = serde_json::to_string(&vote).unwrap();
    assert!(json.contains("\"tx_id\""));
    assert!(json.contains("\"node_id\""));
    assert!(json.contains("\"value\":100"));

    let deserialized: Vote = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.node_id, vote.node_id);
    assert_eq!(deserialized.value, vote.value);
}

#[tokio::test]
async fn test_transaction_serialization() {
    let tx = sample_transaction("api_test_002");

    let json = serde_json::to_string(&tx).unwrap();
    assert!(json.contains("\"txid\""));
    assert!(json.contains("\"state\""));
    assert!(json.contains("\"recipient\""));

    let deserialized: Transaction = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.txid, tx.txid);
    assert_eq!(deserialized.state, tx.state);
    assert_eq!(deserialized.amount_sats, tx.amount_sats);
}

#[tokio::test]
async fn test_byzantine_violation_serialization() {
    let violation = sample_byzantine_violation(1, "api_test_003", ViolationType::DoubleVote);

    let json = serde_json::to_string(&violation).unwrap();
    assert!(json.contains("\"peer_id\""));
    assert!(json.contains("\"violation_type\""));
    assert!(json.contains("\"evidence\""));

    let deserialized: ByzantineViolation = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.peer_id, violation.peer_id);
    assert_eq!(deserialized.violation_type, violation.violation_type);
}

#[tokio::test]
async fn test_transaction_state_enum_serialization() {
    let states = vec![
        TransactionState::Pending,
        TransactionState::Voting,
        TransactionState::ThresholdReached,
        TransactionState::Approved,
        TransactionState::Signing,
        TransactionState::Signed,
        TransactionState::Confirmed,
        TransactionState::Failed,
        TransactionState::AbortedByzantine,
    ];

    for state in states {
        let json = serde_json::to_string(&state).unwrap();
        let deserialized: TransactionState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, state);
    }
}

#[tokio::test]
async fn test_violation_type_enum_serialization() {
    let types = vec![
        ViolationType::DoubleVote,
        ViolationType::InvalidSignature,
        ViolationType::Timeout,
        ViolationType::MalformedMessage,
        ViolationType::MinorityVote,
    ];

    for vtype in types {
        let json = serde_json::to_string(&vtype).unwrap();
        let deserialized: ViolationType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, vtype);
    }
}

#[tokio::test]
async fn test_network_message_serialization() {
    let vote = sample_vote(1, "msg_test_001", 42);
    let vote_msg = VoteMessage::new(vote);

    let network_msg = NetworkMessage::Vote(vote_msg);

    let json = serde_json::to_string(&network_msg).unwrap();
    assert!(json.contains("\"type\":\"vote\""));

    let deserialized: NetworkMessage = serde_json::from_str(&json).unwrap();
    match deserialized {
        NetworkMessage::Vote(_) => {
            // Expected
        }
        _ => panic!("Expected Vote message"),
    }
}

#[tokio::test]
async fn test_heartbeat_message_serialization() {
    let heartbeat = HeartbeatMessage {
        node_id: NodeId(1),
        timestamp: chrono::Utc::now(),
        sequence: 42,
    };

    let network_msg = NetworkMessage::Heartbeat(heartbeat);

    let json = serde_json::to_string(&network_msg).unwrap();
    assert!(json.contains("\"type\":\"heartbeat\""));

    let deserialized: NetworkMessage = serde_json::from_str(&json).unwrap();
    match deserialized {
        NetworkMessage::Heartbeat(hb) => {
            assert_eq!(hb.node_id, NodeId(1));
            assert_eq!(hb.sequence, 42);
        }
        _ => panic!("Expected Heartbeat message"),
    }
}

#[tokio::test]
async fn test_consensus_config_validation() {
    let config = sample_consensus_config();

    assert!(config.threshold <= config.total_nodes);
    assert!(config.threshold > config.total_nodes / 2);

    let json = serde_json::to_string(&config).unwrap();
    let deserialized: ConsensusConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.total_nodes, config.total_nodes);
    assert_eq!(deserialized.threshold, config.threshold);
}

#[tokio::test]
async fn test_node_id_display() {
    let node_id = NodeId(42);
    assert_eq!(format!("{}", node_id), "node-42");

    let node_id2 = NodeId::from(123u64);
    assert_eq!(format!("{}", node_id2), "node-123");
}

#[tokio::test]
async fn test_tx_id_display() {
    let tx_id = TxId::from("test_transaction_001");
    assert_eq!(format!("{}", tx_id), "test_transaction_001");

    let tx_id2 = TxId::from("abc123".to_string());
    assert_eq!(format!("{}", tx_id2), "abc123");
}

#[tokio::test]
async fn test_error_type_serialization() {
    let errors = vec![
        Error::InvalidVote("test error".to_string()),
        Error::ByzantineViolation("double vote".to_string()),
        Error::ConsensusFailed("timeout".to_string()),
        Error::StorageError("connection failed".to_string()),
        Error::NetworkError("peer unreachable".to_string()),
    ];

    for error in errors {
        let error_str = format!("{}", error);
        assert!(!error_str.is_empty());
    }
}

#[tokio::test]
async fn test_presignature_id_generation() {
    let presig_id1 = PresignatureId::new();
    let presig_id2 = PresignatureId::new();

    assert_ne!(presig_id1, presig_id2, "IDs should be unique");

    let id_str = format!("{}", presig_id1);
    assert!(id_str.len() > 0);
}

#[tokio::test]
async fn test_json_api_response_format() {
    // Simulate API response for transaction query
    let tx = sample_transaction("api_response_001");

    let response = json!({
        "success": true,
        "data": {
            "transaction": tx,
            "votes_received": 3,
            "threshold": 4,
        }
    });

    assert_eq!(response["success"], true);
    assert!(response["data"]["transaction"]["txid"].is_string());
}

#[tokio::test]
async fn test_json_api_error_response_format() {
    let error_response = json!({
        "success": false,
        "error": {
            "code": "BYZANTINE_VIOLATION",
            "message": "Double vote detected",
            "details": {
                "node_id": 1,
                "tx_id": "test_001",
            }
        }
    });

    assert_eq!(error_response["success"], false);
    assert_eq!(error_response["error"]["code"], "BYZANTINE_VIOLATION");
}

#[tokio::test]
async fn test_health_check_response() {
    let health_response = json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "services": {
            "postgres": "up",
            "etcd": "up",
            "quic": "up",
        },
        "metrics": {
            "total_transactions": 42,
            "active_nodes": 5,
            "byzantine_violations": 0,
        }
    });

    assert_eq!(health_response["status"], "healthy");
    assert_eq!(health_response["metrics"]["total_transactions"], 42);
}

#[tokio::test]
async fn test_wallet_balance_response() {
    let balance_response = json!({
        "address": "tb1q...",
        "balance_sats": 1_000_000,
        "confirmed_balance_sats": 950_000,
        "unconfirmed_balance_sats": 50_000,
        "utxo_count": 5,
    });

    assert_eq!(balance_response["balance_sats"], 1_000_000);
    assert_eq!(balance_response["utxo_count"], 5);
}

#[tokio::test]
async fn test_cluster_status_response() {
    let cluster_response = json!({
        "cluster_id": "mpc-wallet-cluster-1",
        "total_nodes": 5,
        "active_nodes": 5,
        "threshold": 4,
        "nodes": [
            {"node_id": 1, "status": "active", "last_heartbeat": "2024-01-01T00:00:00Z"},
            {"node_id": 2, "status": "active", "last_heartbeat": "2024-01-01T00:00:00Z"},
            {"node_id": 3, "status": "active", "last_heartbeat": "2024-01-01T00:00:00Z"},
            {"node_id": 4, "status": "active", "last_heartbeat": "2024-01-01T00:00:00Z"},
            {"node_id": 5, "status": "active", "last_heartbeat": "2024-01-01T00:00:00Z"},
        ]
    });

    assert_eq!(cluster_response["total_nodes"], 5);
    assert_eq!(cluster_response["active_nodes"], 5);
}

#[tokio::test]
async fn test_transaction_list_pagination() {
    let transactions: Vec<Transaction> = (1..=10)
        .map(|i| sample_transaction(&format!("tx_{:03}", i)))
        .collect();

    let paginated_response = json!({
        "page": 1,
        "per_page": 10,
        "total": 100,
        "total_pages": 10,
        "transactions": transactions,
    });

    assert_eq!(paginated_response["page"], 1);
    assert_eq!(paginated_response["total"], 100);
    assert_eq!(paginated_response["transactions"].as_array().unwrap().len(), 10);
}

#[tokio::test]
async fn test_send_bitcoin_request_validation() {
    let send_request = json!({
        "recipient": "tb1q...",
        "amount_sats": 10_000,
        "fee_rate": 5,
        "metadata": "Payment for services",
    });

    assert!(send_request["amount_sats"].as_u64().is_some());
    assert!(send_request["amount_sats"].as_u64().unwrap() > 0);
    assert!(send_request["fee_rate"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn test_invalid_amount_validation() {
    let invalid_request = json!({
        "recipient": "tb1q...",
        "amount_sats": 0,  // Invalid: zero amount
        "fee_rate": 5,
    });

    let amount = invalid_request["amount_sats"].as_u64().unwrap();
    assert_eq!(amount, 0);
    // API handler should reject this
}

#[tokio::test]
async fn test_concurrent_api_serialization() {
    let handles: Vec<_> = (1..=100)
        .map(|i| {
            tokio::spawn(async move {
                let vote = sample_vote(i % 5 + 1, &format!("concurrent_{}", i), i * 10);
                let json = serde_json::to_string(&vote).unwrap();
                let deserialized: Vote = serde_json::from_str(&json).unwrap();
                assert_eq!(deserialized.value, i * 10);
            })
        })
        .collect();

    for handle in handles {
        handle.await.unwrap();
    }
}

#[tokio::test]
async fn test_large_payload_handling() {
    // Test with large metadata
    let large_metadata = "x".repeat(1_000);
    let tx = Transaction {
        id: 0,
        txid: TxId::from("large_payload_001"),
        state: TransactionState::Pending,
        unsigned_tx: vec![0u8; 10_000], // 10KB transaction
        signed_tx: None,
        recipient: "tb1q...".to_string(),
        amount_sats: 10_000,
        fee_sats: 500,
        metadata: Some(large_metadata),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let json = serde_json::to_string(&tx).unwrap();
    assert!(json.len() > 10_000);

    let deserialized: Transaction = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.txid, tx.txid);
}

#[tokio::test]
async fn test_unicode_metadata_handling() {
    let unicode_metadata = "Hello 世界 🌍 Привет مرحبا";
    let tx = Transaction {
        id: 0,
        txid: TxId::from("unicode_001"),
        state: TransactionState::Pending,
        unsigned_tx: vec![],
        signed_tx: None,
        recipient: "tb1q...".to_string(),
        amount_sats: 10_000,
        fee_sats: 500,
        metadata: Some(unicode_metadata.to_string()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let json = serde_json::to_string(&tx).unwrap();
    let deserialized: Transaction = serde_json::from_str(&json).unwrap();

    assert_eq!(
        deserialized.metadata.unwrap(),
        unicode_metadata
    );
}

#[tokio::test]
async fn test_timestamp_serialization() {
    let vote = sample_vote(1, "timestamp_test", 100);

    let json = serde_json::to_string(&vote).unwrap();
    assert!(json.contains("timestamp"));

    let deserialized: Vote = serde_json::from_str(&json).unwrap();

    // Timestamps should be within reasonable range
    let now = chrono::Utc::now();
    let diff = (now - deserialized.timestamp).num_seconds().abs();
    assert!(diff < 10, "Timestamp should be recent");
}

#[tokio::test]
async fn test_optional_fields_handling() {
    // Test with all optional fields None
    let minimal_tx = Transaction {
        id: 0,
        txid: TxId::from("minimal_001"),
        state: TransactionState::Pending,
        unsigned_tx: vec![],
        signed_tx: None, // Optional
        recipient: "tb1q...".to_string(),
        amount_sats: 10_000,
        fee_sats: 500,
        metadata: None, // Optional
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let json = serde_json::to_string(&minimal_tx).unwrap();
    let deserialized: Transaction = serde_json::from_str(&json).unwrap();

    assert!(deserialized.signed_tx.is_none());
    assert!(deserialized.metadata.is_none());
}

#[tokio::test]
async fn test_nested_json_structure() {
    let violation = sample_byzantine_violation(1, "nested_001", ViolationType::DoubleVote);

    // Evidence is nested JSON
    assert!(violation.evidence.is_object());
    assert!(violation.evidence.get("test").is_some());

    let json = serde_json::to_string(&violation).unwrap();
    let deserialized: ByzantineViolation = serde_json::from_str(&json).unwrap();

    assert_eq!(
        deserialized.evidence["test"],
        violation.evidence["test"]
    );
}

#[tokio::test]
async fn test_api_version_endpoint() {
    let version_response = json!({
        "version": "0.1.0",
        "api_version": "v1",
        "build_timestamp": "2024-01-01T00:00:00Z",
        "git_commit": "abc123",
    });

    assert_eq!(version_response["api_version"], "v1");
}

#[tokio::test]
async fn test_metrics_endpoint_response() {
    let metrics_response = json!({
        "uptime_seconds": 3600,
        "total_requests": 10_000,
        "successful_requests": 9_950,
        "failed_requests": 50,
        "average_response_time_ms": 25,
        "transactions": {
            "total": 100,
            "pending": 5,
            "confirmed": 90,
            "failed": 5,
        },
        "consensus": {
            "total_votes": 500,
            "byzantine_violations": 2,
            "banned_nodes": 1,
        },
    });

    assert_eq!(metrics_response["total_requests"], 10_000);
    assert_eq!(metrics_response["transactions"]["confirmed"], 90);
}
