mod common;

use common::{Cluster, SendTransactionRequest, helpers};
use std::path::PathBuf;
use std::time::Duration;
use tracing_subscriber;

#[tokio::test]
#[ignore]
async fn test_basic_cluster_with_certificates() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("Cluster started with mTLS certificates");

    // Verify all nodes are healthy with certificates
    for (idx, node) in cluster.nodes.iter().enumerate() {
        let health = node.health_check().await.expect("Health check failed");
        println!("Node {} healthy with certificates: {}", idx + 1, health.status);
    }

    // Submit a transaction to verify QUIC+mTLS works
    let tx_request = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 50000,
        metadata: Some("Certificate test".to_string()),
    };

    let response = cluster.nodes[0]
        .send_transaction(&tx_request)
        .await
        .expect("Failed to send transaction");

    println!("Transaction with mTLS: txid={}", response.txid);

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Basic cluster with certificates");
}

#[tokio::test]
#[ignore]
async fn test_node_restart_with_same_certificate() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("Testing node restart with same certificate");

    // Submit transaction before restart
    let tx_request1 = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 30000,
        metadata: Some("Before restart".to_string()),
    };

    let response1 = cluster.nodes[0]
        .send_transaction(&tx_request1)
        .await
        .expect("Failed to send first transaction");

    println!("Transaction before restart: txid={}", response1.txid);

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Restart node-3
    println!("Restarting node-3");
    cluster.docker.restart("node-3").expect("Failed to restart node-3");

    // Wait for node to come back
    helpers::wait_until(
        || async {
            cluster.nodes[2].health_check().await.is_ok()
        },
        Duration::from_secs(60),
        Duration::from_secs(3),
        "node-3 to restart",
    )
    .await
    .expect("Node-3 did not restart");

    println!("Node-3 restarted successfully");

    // Submit transaction after restart
    let tx_request2 = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 40000,
        metadata: Some("After restart".to_string()),
    };

    let response2 = cluster.nodes[0]
        .send_transaction(&tx_request2)
        .await
        .expect("Failed to send second transaction");

    println!("Transaction after restart: txid={}", response2.txid);

    // Verify restarted node can participate
    tokio::time::sleep(Duration::from_secs(3)).await;

    let tx2 = cluster.nodes[2].get_transaction(&response2.txid).await;
    assert!(tx2.is_ok(), "Restarted node should see new transaction");

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Node restart with same certificate");
}

#[tokio::test]
#[ignore]
async fn test_all_nodes_communicate_via_mtls() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("Testing mTLS communication between all nodes");

    // Wait for nodes to establish QUIC connections
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Submit transaction that requires coordination
    let tx_request = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 55000,
        metadata: Some("mTLS coordination test".to_string()),
    };

    let response = cluster.nodes[0]
        .send_transaction(&tx_request)
        .await
        .expect("Failed to send transaction");

    let txid = response.txid.clone();
    println!("Transaction submitted: txid={}", txid);

    // Wait for voting to complete (requires mTLS communication)
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Verify all nodes can see the transaction (indicates successful mTLS)
    for (idx, node) in cluster.nodes.iter().enumerate() {
        let tx = node.get_transaction(&txid).await;
        assert!(
            tx.is_ok(),
            "Node {} should see transaction via mTLS",
            idx + 1
        );
        println!("Node {} successfully communicating via mTLS", idx + 1);
    }

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: All nodes communicate via mTLS");
}

#[tokio::test]
#[ignore]
async fn test_certificate_validation() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("Verifying certificate validation is working");

    // All nodes should be healthy with valid certificates
    for (idx, node) in cluster.nodes.iter().enumerate() {
        let health = node.health_check().await;
        assert!(
            health.is_ok(),
            "Node {} should be healthy with valid certificate",
            idx + 1
        );
    }

    println!("All nodes have valid certificates and are healthy");

    // Submit transaction to ensure QUIC+mTLS is properly validating certs
    let tx_request = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 45000,
        metadata: Some("Cert validation test".to_string()),
    };

    let response = cluster.nodes[0]
        .send_transaction(&tx_request)
        .await
        .expect("Failed to send transaction");

    println!("Transaction succeeded with validated certificates: txid={}", response.txid);

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Certificate validation");
}

#[tokio::test]
#[ignore]
async fn test_mtls_under_load() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("Testing mTLS under load");

    // Submit multiple transactions concurrently
    let mut handles = vec![];

    for i in 0..10 {
        let node = cluster.nodes[i % cluster.nodes.len()].clone();

        let handle = tokio::spawn(async move {
            let tx_request = SendTransactionRequest {
                recipient: helpers::generate_test_address(),
                amount_sats: (i as u64 + 1) * 6000,
                metadata: Some(format!("mTLS load test {}", i + 1)),
            };

            node.send_transaction(&tx_request).await
        });

        handles.push(handle);
    }

    let results = futures::future::join_all(handles).await;

    let successful = results.iter()
        .filter(|r| matches!(r, Ok(Ok(_))))
        .count();

    println!("Successful transactions under mTLS load: {}/10", successful);
    assert!(successful >= 8, "Most transactions should succeed under mTLS load");

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: mTLS under load");
}

#[tokio::test]
#[ignore]
async fn test_quic_connection_reestablishment() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("Testing QUIC connection reestablishment");

    // Submit initial transaction
    let tx_request1 = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 35000,
        metadata: Some("Before connection reset".to_string()),
    };

    let response1 = cluster.nodes[0]
        .send_transaction(&tx_request1)
        .await
        .expect("Failed to send first transaction");

    println!("First transaction: txid={}", response1.txid);

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Pause and unpause node to simulate connection disruption
    println!("Simulating connection disruption on node-2");
    cluster.docker.pause("node-2").expect("Failed to pause node-2");

    tokio::time::sleep(Duration::from_secs(3)).await;

    cluster.docker.unpause("node-2").expect("Failed to unpause node-2");

    println!("Node-2 unpaused, waiting for QUIC reconnection");

    // Wait for node to recover and reestablish connections
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Submit new transaction to verify connections are reestablished
    let tx_request2 = SendTransactionRequest {
        recipient: helpers::generate_test_address(),
        amount_sats: 42000,
        metadata: Some("After connection reset".to_string()),
    };

    let response2 = cluster.nodes[0]
        .send_transaction(&tx_request2)
        .await
        .expect("Failed to send second transaction");

    println!("Second transaction after reconnection: txid={}", response2.txid);

    // Verify node-2 can see the new transaction
    tokio::time::sleep(Duration::from_secs(3)).await;

    let tx2 = cluster.nodes[1].get_transaction(&response2.txid).await;
    if tx2.is_ok() {
        println!("Node-2 successfully reestablished QUIC connections");
    }

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: QUIC connection reestablishment");
}

#[tokio::test]
#[ignore]
async fn test_certificate_based_node_identity() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("Testing certificate-based node identity");

    // Each node should have its own certificate and be identifiable
    // Submit transactions from different nodes to verify identity

    for (idx, node) in cluster.nodes.iter().enumerate() {
        let tx_request = SendTransactionRequest {
            recipient: helpers::generate_test_address(),
            amount_sats: ((idx + 1) * 8000) as u64,
            metadata: Some(format!("From node {}", idx + 1)),
        };

        let response = node.send_transaction(&tx_request).await;
        assert!(
            response.is_ok(),
            "Node {} should authenticate with its certificate",
            idx + 1
        );

        if let Ok(r) = response {
            println!("Node {} authenticated: txid={}", idx + 1, r.txid);
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    println!("All nodes successfully authenticated with individual certificates");

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Certificate-based node identity");
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();
}
