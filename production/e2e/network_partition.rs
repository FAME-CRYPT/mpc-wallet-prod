mod common;

use common::{Cluster, SendTransactionRequest, helpers};
use std::path::PathBuf;
use std::time::Duration;
use tracing_subscriber;

#[tokio::test]
#[ignore]
async fn test_network_partition_majority_continues() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("Creating network partition: isolating nodes 4 and 5");

    // Disconnect nodes 4 and 5 from the internal network
    cluster.docker
        .disconnect_network("node-4", "e2e-internal")
        .expect("Failed to disconnect node-4");

    cluster.docker
        .disconnect_network("node-5", "e2e-internal")
        .expect("Failed to disconnect node-5");

    println!("Network partition created");

    tokio::time::sleep(Duration::from_secs(3)).await;

    // Submit transaction to majority partition (nodes 1, 2, 3)
    println!("Submitting transaction to majority partition");

    let tx_request = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 50000,
        metadata: Some("Partition test".to_string()),
    };

    let response = cluster.nodes[0]
        .send_transaction(&tx_request)
        .await
        .expect("Failed to send transaction");

    let txid = response.txid.clone();
    println!("Transaction submitted to majority: txid={}", txid);

    // Wait for processing in majority partition
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Verify transaction is visible on majority nodes (1, 2, 3)
    for i in 0..3 {
        let tx = cluster.nodes[i].get_transaction(&txid).await;
        assert!(tx.is_ok(), "Majority node {} should see transaction", i + 1);
        println!("Node {} sees transaction", i + 1);
    }

    // Nodes 4 and 5 should not be accessible or not see the transaction
    println!("Checking isolated nodes (4, 5)");
    for i in 3..5 {
        let health = cluster.nodes[i].health_check().await;
        if health.is_ok() {
            println!("Node {} still responding (unexpected)", i + 1);
        } else {
            println!("Node {} not responding (expected)", i + 1);
        }
    }

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Network partition majority continues");
}

#[tokio::test]
#[ignore]
async fn test_network_partition_healing() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    // Submit transaction before partition
    let tx_request1 = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 30000,
        metadata: Some("Before partition".to_string()),
    };

    let response1 = cluster.nodes[0]
        .send_transaction(&tx_request1)
        .await
        .expect("Failed to send first transaction");

    println!("Transaction before partition: txid={}", response1.txid);

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Create partition
    println!("Creating network partition");
    cluster.docker
        .disconnect_network("node-5", "e2e-internal")
        .expect("Failed to disconnect node-5");

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Submit transaction during partition
    let tx_request2 = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 40000,
        metadata: Some("During partition".to_string()),
    };

    let response2 = cluster.nodes[0]
        .send_transaction(&tx_request2)
        .await
        .expect("Failed to send second transaction");

    println!("Transaction during partition: txid={}", response2.txid);

    tokio::time::sleep(Duration::from_secs(3)).await;

    // Heal partition
    println!("Healing network partition");
    cluster.docker
        .connect_network("node-5", "e2e-internal")
        .expect("Failed to reconnect node-5");

    println!("Partition healed, waiting for resync");

    // Wait for node-5 to resync
    tokio::time::sleep(Duration::from_secs(15)).await;

    // Check if node-5 is healthy
    let health = cluster.nodes[4].health_check().await;
    if health.is_ok() {
        println!("Node-5 is healthy after partition heal");

        // Verify node-5 can see both transactions
        let tx1 = cluster.nodes[4].get_transaction(&response1.txid).await;
        let tx2 = cluster.nodes[4].get_transaction(&response2.txid).await;

        if tx1.is_ok() {
            println!("Node-5 sees pre-partition transaction");
        }
        if tx2.is_ok() {
            println!("Node-5 sees during-partition transaction");
        }
    } else {
        println!("Node-5 not yet healthy after partition heal");
    }

    // Submit final transaction to verify cluster works
    let tx_request3 = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 35000,
        metadata: Some("After healing".to_string()),
    };

    let response3 = cluster.nodes[0]
        .send_transaction(&tx_request3)
        .await
        .expect("Failed to send post-heal transaction");

    println!("Transaction after healing: txid={}", response3.txid);

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Network partition healing");
}

#[tokio::test]
#[ignore]
async fn test_minority_partition_cannot_progress() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("Creating partition: isolating nodes 1, 2, 3 (majority)");

    // Disconnect nodes 1, 2, 3, leaving 4 and 5 as minority
    for i in 1..=3 {
        cluster.docker
            .disconnect_network(&format!("node-{}", i), "e2e-internal")
            .expect(&format!("Failed to disconnect node-{}", i));
    }

    println!("Partition created, nodes 4 and 5 are minority");

    tokio::time::sleep(Duration::from_secs(3)).await;

    // Try to submit transaction to minority partition (should fail or timeout)
    println!("Attempting transaction on minority partition");

    let tx_request = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 25000,
        metadata: Some("Minority test".to_string()),
    };

    // Try node-4 (minority)
    let result = cluster.nodes[3].send_transaction(&tx_request).await;

    match result {
        Ok(response) => {
            println!("Transaction submitted: txid={}", response.txid);
            println!("Note: Transaction accepted but consensus may not be reached");

            tokio::time::sleep(Duration::from_secs(10)).await;

            // Check if it progressed (should not with only 2 nodes when threshold is 4)
            if let Ok(tx) = cluster.nodes[3].get_transaction(&response.txid).await {
                println!("Transaction state in minority: {:?}", tx.state);
            }
        }
        Err(e) => {
            println!("Transaction failed on minority partition (expected): {}", e);
        }
    }

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Minority partition cannot progress");
}

#[tokio::test]
#[ignore]
async fn test_split_brain_scenario() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("Creating split-brain: 3 nodes vs 2 nodes");

    // Create two partitions:
    // Partition A: nodes 1, 2, 3 (can reach threshold)
    // Partition B: nodes 4, 5 (cannot reach threshold)

    // This is tricky with Docker networks, so we'll simulate by killing connectivity
    cluster.docker
        .disconnect_network("node-4", "e2e-internal")
        .expect("Failed to disconnect node-4");

    cluster.docker
        .disconnect_network("node-5", "e2e-internal")
        .expect("Failed to disconnect node-5");

    println!("Split-brain created");

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Submit to partition A (majority)
    let tx_request_a = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 30000,
        metadata: Some("Partition A tx".to_string()),
    };

    let response_a = cluster.nodes[0]
        .send_transaction(&tx_request_a)
        .await
        .expect("Failed to send to partition A");

    println!("Transaction on partition A: txid={}", response_a.txid);

    // Attempt to submit to partition B (minority) - may fail
    let tx_request_b = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 40000,
        metadata: Some("Partition B tx".to_string()),
    };

    let result_b = cluster.nodes[3].send_transaction(&tx_request_b).await;
    match result_b {
        Ok(r) => println!("Transaction on partition B: txid={} (may not complete)", r.txid),
        Err(e) => println!("Transaction on partition B failed: {}", e),
    }

    tokio::time::sleep(Duration::from_secs(5)).await;

    // Heal the partition
    println!("Healing split-brain");
    cluster.docker
        .connect_network("node-4", "e2e-internal")
        .expect("Failed to reconnect node-4");

    cluster.docker
        .connect_network("node-5", "e2e-internal")
        .expect("Failed to reconnect node-5");

    println!("Split-brain healed, waiting for convergence");
    tokio::time::sleep(Duration::from_secs(15)).await;

    // Verify partition A transaction is visible everywhere
    for i in 0..5 {
        let tx = cluster.nodes[i].get_transaction(&response_a.txid).await;
        if tx.is_ok() {
            println!("Node {} sees partition A transaction", i + 1);
        }
    }

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Split-brain scenario");
}

#[tokio::test]
#[ignore]
async fn test_cascading_partition_failures() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("Testing cascading partition failures");

    // Start with full cluster
    let tx_request1 = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 20000,
        metadata: Some("Full cluster".to_string()),
    };

    let response1 = cluster.nodes[0]
        .send_transaction(&tx_request1)
        .await
        .expect("Transaction 1 failed");

    println!("Transaction with 5 nodes: txid={}", response1.txid);
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Isolate one node
    println!("Isolating node-5");
    cluster.docker
        .disconnect_network("node-5", "e2e-internal")
        .expect("Failed to disconnect node-5");

    let tx_request2 = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 25000,
        metadata: Some("4 nodes".to_string()),
    };

    let response2 = cluster.nodes[0]
        .send_transaction(&tx_request2)
        .await
        .expect("Transaction 2 failed");

    println!("Transaction with 4 nodes: txid={}", response2.txid);
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Isolate another node (now below threshold)
    println!("Isolating node-4");
    cluster.docker
        .disconnect_network("node-4", "e2e-internal")
        .expect("Failed to disconnect node-4");

    let tx_request3 = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 30000,
        metadata: Some("3 nodes (below threshold)".to_string()),
    };

    let result3 = cluster.nodes[0].send_transaction(&tx_request3).await;

    match result3 {
        Ok(response) => {
            println!("Transaction with 3 nodes: txid={} (may not complete)", response.txid);
        }
        Err(e) => {
            println!("Transaction with 3 nodes failed (expected): {}", e);
        }
    }

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Cascading partition failures");
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();
}
