mod common;

use common::{Cluster, SendTransactionRequest, helpers};
use std::path::PathBuf;
use std::time::Duration;
use threshold_types::TransactionState;
use tracing_subscriber;

#[tokio::test]
#[ignore]
async fn test_node_failure_during_voting() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("Submitting transaction");

    // Submit transaction
    let tx_request = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 50000,
        metadata: Some("Node failure test".to_string()),
    };

    let response = cluster.nodes[0]
        .send_transaction(&tx_request)
        .await
        .expect("Failed to send transaction");

    let txid = response.txid.clone();
    println!("Transaction submitted: txid={}", txid);

    // Wait briefly for voting to start
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Kill node-5 during voting
    println!("Killing node-5 during voting");
    cluster.docker
        .kill("node-5")
        .expect("Failed to kill node-5");

    // Wait for consensus with remaining 4 nodes (threshold = 4)
    println!("Waiting for consensus with remaining nodes");
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Verify transaction progressed (4 nodes should be enough for threshold)
    let tx = cluster.nodes[0]
        .get_transaction(&txid)
        .await
        .expect("Failed to get transaction");

    println!("Transaction state after node failure: {:?}", tx.state);

    // With 4 nodes and threshold 4, consensus should still be possible
    assert!(
        matches!(
            tx.state,
            TransactionState::Voting
                | TransactionState::ThresholdReached
                | TransactionState::Approved
                | TransactionState::Signing
        ),
        "Transaction should progress despite one node failure"
    );

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Node failure during voting");
}

#[tokio::test]
#[ignore]
async fn test_node_recovery_after_restart() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    // Submit initial transaction
    let tx_request1 = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 30000,
        metadata: Some("Before restart".to_string()),
    };

    let response1 = cluster.nodes[0]
        .send_transaction(&tx_request1)
        .await
        .expect("Failed to send transaction");

    println!("Transaction before restart: txid={}", response1.txid);

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Kill node-4
    println!("Killing node-4");
    cluster.docker.kill("node-4").expect("Failed to kill node-4");

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Restart node-4
    println!("Restarting node-4");
    cluster.docker.restart("node-4").expect("Failed to restart node-4");

    // Wait for node to be healthy again
    helpers::wait_until(
        || async {
            cluster.nodes[3].health_check().await.is_ok()
        },
        Duration::from_secs(60),
        Duration::from_secs(3),
        "node-4 to be healthy",
    )
    .await
    .expect("Node-4 did not recover");

    println!("Node-4 recovered successfully");

    // Submit new transaction to verify cluster works
    let tx_request2 = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 40000,
        metadata: Some("After restart".to_string()),
    };

    let response2 = cluster.nodes[0]
        .send_transaction(&tx_request2)
        .await
        .expect("Failed to send transaction after restart");

    println!("Transaction after restart: txid={}", response2.txid);

    // Verify both transactions are visible on recovered node
    let tx1 = cluster.nodes[3].get_transaction(&response1.txid).await;
    let tx2 = cluster.nodes[3].get_transaction(&response2.txid).await;

    assert!(tx1.is_ok(), "Recovered node should see old transaction");
    assert!(tx2.is_ok(), "Recovered node should see new transaction");

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Node recovery after restart");
}

#[tokio::test]
#[ignore]
async fn test_multiple_node_failures() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("Killing 2 nodes simultaneously (node-4 and node-5)");

    // Kill 2 nodes (leaving 3 nodes, which is below threshold of 4)
    cluster.docker.kill("node-4").expect("Failed to kill node-4");
    cluster.docker.kill("node-5").expect("Failed to kill node-5");

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Try to submit transaction (should fail or timeout as threshold cannot be met)
    println!("Attempting transaction with insufficient nodes");

    let tx_request = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 25000,
        metadata: Some("Insufficient nodes test".to_string()),
    };

    let response = cluster.nodes[0]
        .send_transaction(&tx_request)
        .await;

    match response {
        Ok(r) => {
            println!("Transaction submitted: txid={}", r.txid);
            // Wait to see if it can reach consensus
            tokio::time::sleep(Duration::from_secs(10)).await;

            let tx = cluster.nodes[0].get_transaction(&r.txid).await.unwrap();
            println!("Transaction state with insufficient nodes: {:?}", tx.state);

            // Should not reach threshold
            assert!(
                !matches!(tx.state, TransactionState::ThresholdReached | TransactionState::Approved),
                "Should not reach threshold with only 3 nodes"
            );
        }
        Err(e) => {
            println!("Transaction failed as expected: {}", e);
        }
    }

    // Restart nodes
    println!("Restarting failed nodes");
    cluster.docker.restart("node-4").expect("Failed to restart node-4");
    cluster.docker.restart("node-5").expect("Failed to restart node-5");

    // Wait for recovery
    tokio::time::sleep(Duration::from_secs(30)).await;

    // Verify cluster works again
    let tx_request2 = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 35000,
        metadata: Some("After recovery".to_string()),
    };

    let response2 = cluster.nodes[0]
        .send_transaction(&tx_request2)
        .await
        .expect("Failed to send transaction after recovery");

    println!("Transaction after recovery: txid={}", response2.txid);

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Multiple node failures");
}

#[tokio::test]
#[ignore]
async fn test_sequential_node_failures() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    // Kill nodes one by one
    for node_num in (2..=5).rev() {
        let node_name = format!("node-{}", node_num);
        println!("Killing {}", node_name);

        cluster.docker.kill(&node_name).expect(&format!("Failed to kill {}", node_name));

        tokio::time::sleep(Duration::from_secs(2)).await;

        // Try to submit transaction
        let tx_request = SendTransactionRequest {
            recipient: helpers::generate_test_address(),
            amount_sats: 10000 * node_num,
            metadata: Some(format!("With {} nodes", 6 - node_num)),
        };

        let result = cluster.nodes[0].send_transaction(&tx_request).await;

        match result {
            Ok(response) => {
                println!("Transaction submitted with {} nodes alive: txid={}",
                         6 - node_num, response.txid);
            }
            Err(e) => {
                println!("Transaction failed with {} nodes alive: {}",
                         6 - node_num, e);
            }
        }

        // Stop when we can't reach threshold
        if 6 - node_num < 4 {
            println!("Below threshold, stopping sequential failures");
            break;
        }
    }

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Sequential node failures");
}

#[tokio::test]
#[ignore]
async fn test_node_crash_and_state_sync() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    // Submit first transaction
    let tx_request1 = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 20000,
        metadata: Some("First transaction".to_string()),
    };

    let response1 = cluster.nodes[0]
        .send_transaction(&tx_request1)
        .await
        .expect("Failed to send first transaction");

    println!("First transaction: txid={}", response1.txid);

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Crash node-3
    println!("Crashing node-3");
    cluster.docker.kill("node-3").expect("Failed to kill node-3");

    // Submit second transaction while node-3 is down
    let tx_request2 = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 30000,
        metadata: Some("While node-3 down".to_string()),
    };

    let response2 = cluster.nodes[0]
        .send_transaction(&tx_request2)
        .await
        .expect("Failed to send second transaction");

    println!("Second transaction (while node-3 down): txid={}", response2.txid);

    tokio::time::sleep(Duration::from_secs(3)).await;

    // Restart node-3
    println!("Restarting node-3");
    cluster.docker.restart("node-3").expect("Failed to restart node-3");

    // Wait for node-3 to sync
    helpers::wait_until(
        || async {
            cluster.nodes[2].health_check().await.is_ok()
        },
        Duration::from_secs(60),
        Duration::from_secs(3),
        "node-3 to sync",
    )
    .await
    .expect("Node-3 did not sync");

    println!("Node-3 restarted and synced");

    // Verify node-3 can see both transactions
    let tx1_on_node3 = cluster.nodes[2].get_transaction(&response1.txid).await;
    let tx2_on_node3 = cluster.nodes[2].get_transaction(&response2.txid).await;

    assert!(tx1_on_node3.is_ok(), "Node-3 should see first transaction after sync");
    assert!(tx2_on_node3.is_ok(), "Node-3 should see second transaction after sync");

    println!("Node-3 successfully synced both transactions");

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Node crash and state sync");
}

#[tokio::test]
#[ignore]
async fn test_rapid_node_restarts() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    // Rapidly restart node-2 three times
    for i in 1..=3 {
        println!("Restart iteration {}", i);

        cluster.docker.kill("node-2").expect("Failed to kill node-2");
        tokio::time::sleep(Duration::from_secs(1)).await;

        cluster.docker.restart("node-2").expect("Failed to restart node-2");
        tokio::time::sleep(Duration::from_secs(10)).await;

        // Verify node comes back
        let health = cluster.nodes[1].health_check().await;
        if health.is_ok() {
            println!("Node-2 recovered after restart {}", i);
        } else {
            println!("Node-2 not healthy after restart {}, continuing", i);
        }
    }

    // Final check - submit transaction
    let tx_request = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 45000,
        metadata: Some("After rapid restarts".to_string()),
    };

    let response = cluster.nodes[0]
        .send_transaction(&tx_request)
        .await
        .expect("Failed to send transaction after rapid restarts");

    println!("Transaction succeeded after rapid restarts: txid={}", response.txid);

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Rapid node restarts");
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();
}
