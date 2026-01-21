mod common;

use common::{Cluster, SendTransactionRequest, assertions, helpers};
use std::path::PathBuf;
use std::time::Duration;
use threshold_types::TransactionState;
use tracing_subscriber;

#[tokio::test]
#[ignore]
async fn test_full_transaction_lifecycle() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    // 1. Submit transaction via node-1
    println!("Step 1: Submitting transaction via node-1");
    let tx_request = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 50000,
        metadata: Some("E2E test transaction".to_string()),
    };

    let response = cluster.nodes[0]
        .send_transaction(&tx_request)
        .await
        .expect("Failed to send transaction");

    let txid = response.txid.clone();
    println!("Transaction submitted: txid={}", txid);

    // Verify initial state
    assert_eq!(response.state, TransactionState::Pending);
    assert_eq!(response.amount_sats, 50000);

    // 2. Wait for votes to propagate
    println!("Step 2: Waiting for vote propagation");
    tokio::time::sleep(Duration::from_secs(3)).await;

    // 3. Verify all nodes received the transaction
    println!("Step 3: Verifying transaction visibility on all nodes");
    for (idx, node) in cluster.nodes.iter().enumerate() {
        let tx = node
            .get_transaction(&txid)
            .await
            .expect(&format!("Node {} cannot see transaction", idx + 1));

        println!("Node {} sees transaction in state: {:?}", idx + 1, tx.state);

        // Should be in voting or later state
        assert!(
            matches!(
                tx.state,
                TransactionState::Voting
                    | TransactionState::ThresholdReached
                    | TransactionState::Approved
                    | TransactionState::Signing
            ),
            "Node {} has unexpected state: {:?}",
            idx + 1,
            tx.state
        );
    }

    // 4. Wait for consensus (threshold reached)
    println!("Step 4: Waiting for consensus");
    helpers::wait_until(
        || async {
            let tx = cluster.nodes[0].get_transaction(&txid).await.ok()?;
            Some(matches!(
                tx.state,
                TransactionState::ThresholdReached | TransactionState::Approved
            ))
            .filter(|&v| v)
        },
        Duration::from_secs(30),
        Duration::from_secs(2),
        "consensus to be reached",
    )
    .await
    .expect("Consensus not reached in time");

    let tx = cluster.nodes[0]
        .get_transaction(&txid)
        .await
        .expect("Failed to get transaction");

    println!("Consensus reached! Final state: {:?}", tx.state);

    // 5. Verify signatures in PostgreSQL
    println!("Step 5: Verifying signatures in database");
    let signatures = cluster
        .postgres
        .query_signatures(&txid)
        .await
        .expect("Failed to query signatures");

    println!("Signatures recorded: {}", signatures.len());
    assert!(
        signatures.len() >= 4,
        "Expected at least 4 signatures, got {}",
        signatures.len()
    );

    // 6. Verify audit log
    println!("Step 6: Verifying audit log");
    let audit_logs = cluster
        .postgres
        .query_audit_log(&txid)
        .await
        .expect("Failed to query audit log");

    println!("Audit log entries: {}", audit_logs.len());
    assert!(!audit_logs.is_empty(), "Audit log should not be empty");

    // Verify state transition was logged
    let has_state_change = audit_logs
        .iter()
        .any(|log| log.event_type == "transaction_state_change");
    assert!(has_state_change, "State change not logged in audit trail");

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Full transaction lifecycle");
}

#[tokio::test]
#[ignore]
async fn test_transaction_visibility_across_nodes() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    // Submit transaction to node-1
    let tx_request = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 100000,
        metadata: None,
    };

    let response = cluster.nodes[0]
        .send_transaction(&tx_request)
        .await
        .expect("Failed to send transaction");

    let txid = response.txid.clone();

    // Wait for propagation
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Query from different nodes
    for (idx, node) in cluster.nodes.iter().enumerate() {
        let tx = node
            .get_transaction(&txid)
            .await
            .expect(&format!("Node {} cannot see transaction", idx + 1));

        println!("Node {} sees transaction: txid={}, state={:?}",
                 idx + 1, tx.txid, tx.state);

        assert_eq!(tx.txid, txid);
        assert_eq!(tx.amount_sats, 100000);
    }

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Transaction visibility across nodes");
}

#[tokio::test]
#[ignore]
async fn test_concurrent_transactions() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("Submitting 5 concurrent transactions");

    // Submit 5 transactions concurrently
    let mut handles = vec![];

    for i in 0..5 {
        let node = cluster.nodes[i % cluster.nodes.len()].clone();
        let handle = tokio::spawn(async move {
            let tx_request = SendTransactionRequest {
                recipient: helpers::generate_test_address(),
                amount_sats: 10000 * (i as u64 + 1),
                metadata: Some(format!("Concurrent tx {}", i + 1)),
            };

            node.send_transaction(&tx_request).await
        });

        handles.push(handle);
    }

    let results: Vec<_> = futures::future::join_all(handles).await;

    // Verify all transactions succeeded
    let mut txids = vec![];
    for (idx, result) in results.into_iter().enumerate() {
        let response = result
            .expect("Task panicked")
            .expect(&format!("Transaction {} failed", idx + 1));

        txids.push(response.txid.clone());
        println!("Transaction {}: txid={}", idx + 1, response.txid);
    }

    // Wait for processing
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Verify all transactions are visible
    for txid in &txids {
        let tx = cluster.nodes[0]
            .get_transaction(txid)
            .await
            .expect("Transaction not found");

        println!("Transaction {} state: {:?}", txid, tx.state);
    }

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Concurrent transactions");
}

#[tokio::test]
#[ignore]
async fn test_transaction_state_transitions() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    // Submit transaction
    let tx_request = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 75000,
        metadata: Some("State transition test".to_string()),
    };

    let response = cluster.nodes[0]
        .send_transaction(&tx_request)
        .await
        .expect("Failed to send transaction");

    let txid = response.txid.clone();

    // Track state transitions
    let mut seen_states = vec![response.state];

    for _ in 0..20 {
        tokio::time::sleep(Duration::from_secs(1)).await;

        let tx = cluster.nodes[0]
            .get_transaction(&txid)
            .await
            .expect("Failed to get transaction");

        if !seen_states.contains(&tx.state) {
            println!("State transition: {:?} -> {:?}",
                     seen_states.last().unwrap(), tx.state);
            seen_states.push(tx.state);
        }

        // Stop once we reach a terminal or stable state
        if matches!(
            tx.state,
            TransactionState::Approved
                | TransactionState::Signed
                | TransactionState::Failed
                | TransactionState::Rejected
        ) {
            break;
        }
    }

    println!("Observed state transitions: {:?}", seen_states);

    // Verify we saw expected progression
    assert!(seen_states.len() >= 2, "Should see at least 2 states");
    assert_eq!(seen_states[0], TransactionState::Pending);

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Transaction state transitions");
}

#[tokio::test]
#[ignore]
async fn test_transaction_list_endpoint() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    // Submit 3 transactions
    for i in 1..=3 {
        let tx_request = SendTransactionRequest {
            recipient: helpers::generate_test_address(),
            amount_sats: i * 10000,
            metadata: Some(format!("Tx {}", i)),
        };

        cluster.nodes[0]
            .send_transaction(&tx_request)
            .await
            .expect("Failed to send transaction");
    }

    tokio::time::sleep(Duration::from_secs(2)).await;

    // List all transactions
    let list_response = cluster.nodes[0]
        .list_transactions()
        .await
        .expect("Failed to list transactions");

    println!("Total transactions: {}", list_response.total);
    assert!(list_response.total >= 3, "Should have at least 3 transactions");
    assert_eq!(list_response.transactions.len(), list_response.total);

    for tx in &list_response.transactions {
        println!("  - txid={}, state={:?}, amount={}",
                 tx.txid, tx.state, tx.amount_sats);
    }

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Transaction list endpoint");
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();
}
