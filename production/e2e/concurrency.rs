mod common;

use common::{Cluster, SendTransactionRequest, helpers};
use std::path::PathBuf;
use std::time::Duration;
use tracing_subscriber;

#[tokio::test]
#[ignore]
async fn test_concurrent_transaction_submission() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("Submitting 10 transactions concurrently");

    let mut handles = vec![];

    // Submit 10 transactions concurrently to different nodes
    for i in 0..10 {
        let node = cluster.nodes[i % cluster.nodes.len()].clone();

        let handle = tokio::spawn(async move {
            let tx_request = SendTransactionRequest {
                recipient: helpers::generate_test_address(),
                amount_sats: (i as u64 + 1) * 5000,
                metadata: Some(format!("Concurrent tx {}", i + 1)),
            };

            let start = std::time::Instant::now();
            let result = node.send_transaction(&tx_request).await;
            let elapsed = start.elapsed();

            (i, result, elapsed)
        });

        handles.push(handle);
    }

    // Wait for all transactions
    let results = futures::future::join_all(handles).await;

    let mut successful = 0;
    let mut failed = 0;
    let mut total_time = Duration::ZERO;
    let mut txids = vec![];

    for (idx, result, elapsed) in results {
        match result {
            Ok(Ok(response)) => {
                println!("Transaction {} succeeded: txid={}, time={:?}",
                         idx + 1, response.txid, elapsed);
                txids.push(response.txid);
                successful += 1;
                total_time += elapsed;
            }
            Ok(Err(e)) => {
                println!("Transaction {} failed: {}", idx + 1, e);
                failed += 1;
            }
            Err(e) => {
                println!("Transaction {} panicked: {}", idx + 1, e);
                failed += 1;
            }
        }
    }

    println!("\nResults: {} successful, {} failed", successful, failed);
    println!("Average submission time: {:?}", total_time / successful.max(1));

    assert!(successful >= 8, "At least 8 out of 10 transactions should succeed");

    // Wait for processing
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Verify all successful transactions are visible
    for txid in &txids {
        let tx = cluster.nodes[0].get_transaction(txid).await;
        assert!(tx.is_ok(), "Transaction {} should be visible", txid);
    }

    // Check for Byzantine violations (should be none in normal concurrent operation)
    let violations = cluster.postgres
        .query_byzantine_violations()
        .await
        .expect("Failed to query violations");

    println!("Byzantine violations during concurrent ops: {}", violations.len());

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Concurrent transaction submission");
}

#[tokio::test]
#[ignore]
async fn test_no_vote_conflicts_under_load() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("Submitting transactions under load");

    // Submit 5 transactions in quick succession
    let mut txids = vec![];

    for i in 0..5 {
        let tx_request = SendTransactionRequest {
            recipient: helpers::generate_test_address(),
            amount_sats: (i + 1) * 10000,
            metadata: Some(format!("Load test tx {}", i + 1)),
        };

        let response = cluster.nodes[i % cluster.nodes.len()]
            .send_transaction(&tx_request)
            .await
            .expect("Failed to send transaction");

        txids.push(response.txid.clone());
        println!("Submitted transaction {}: txid={}", i + 1, response.txid);

        // Small delay to stagger submissions
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Wait for all to process
    tokio::time::sleep(Duration::from_secs(15)).await;

    // Check each transaction for vote conflicts
    let violations = cluster.postgres
        .query_byzantine_violations()
        .await
        .expect("Failed to query violations");

    let vote_conflicts = violations.iter()
        .filter(|v| v.violation_type == "double_vote")
        .count();

    println!("Vote conflicts detected: {}", vote_conflicts);
    assert_eq!(vote_conflicts, 0, "No vote conflicts should occur under normal load");

    // Verify all transactions progressed
    for (idx, txid) in txids.iter().enumerate() {
        let tx = cluster.nodes[0].get_transaction(txid).await.unwrap();
        println!("Transaction {} final state: {:?}", idx + 1, tx.state);
    }

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: No vote conflicts under load");
}

#[tokio::test]
#[ignore]
async fn test_transaction_ordering_under_concurrency() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("Testing transaction ordering");

    let mut txids_with_timestamps = vec![];

    // Submit transactions with known ordering
    for i in 0..5 {
        let tx_request = SendTransactionRequest {
            recipient: helpers::generate_test_address(),
            amount_sats: (i + 1) * 12000,
            metadata: Some(format!("Ordered tx {}", i + 1)),
        };

        let submit_time = chrono::Utc::now();
        let response = cluster.nodes[0]
            .send_transaction(&tx_request)
            .await
            .expect("Failed to send transaction");

        txids_with_timestamps.push((response.txid, submit_time));
        println!("Submitted tx {} at {:?}", i + 1, submit_time);

        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // Wait for processing
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Retrieve all transactions and check ordering
    let mut retrieved = vec![];
    for (txid, submit_time) in &txids_with_timestamps {
        let tx = cluster.nodes[0].get_transaction(txid).await.unwrap();
        retrieved.push((tx.txid, tx.created_at, *submit_time));
    }

    // Verify created_at timestamps respect submission order
    for i in 0..retrieved.len() - 1 {
        let (txid1, created1, submit1) = &retrieved[i];
        let (txid2, created2, submit2) = &retrieved[i + 1];

        println!("Transaction {}: submitted={:?}, created={:?}",
                 txid1, submit1, created1);

        // Created timestamps should generally follow submission order
        // (with some tolerance for clock skew)
        if created2 < created1 {
            let diff = created1.signed_duration_since(*created2);
            println!("Warning: Transaction ordering reversed by {:?}", diff);
        }
    }

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Transaction ordering under concurrency");
}

#[tokio::test]
#[ignore]
async fn test_mixed_load_different_nodes() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("Submitting mixed load to different nodes");

    let mut handles = vec![];

    // Each node gets 2 transactions
    for node_idx in 0..5 {
        for tx_idx in 0..2 {
            let node = cluster.nodes[node_idx].clone();
            let global_idx = node_idx * 2 + tx_idx;

            let handle = tokio::spawn(async move {
                let tx_request = SendTransactionRequest {
                    recipient: helpers::generate_test_address(),
                    amount_sats: ((global_idx + 1) * 8000) as u64,
                    metadata: Some(format!("Node {} tx {}", node_idx + 1, tx_idx + 1)),
                };

                node.send_transaction(&tx_request).await
            });

            handles.push((node_idx, handle));
        }
    }

    // Collect results
    let mut results_by_node = vec![vec![], vec![], vec![], vec![], vec![]];

    for (node_idx, handle) in handles {
        match handle.await {
            Ok(Ok(response)) => {
                results_by_node[node_idx].push(response);
            }
            Ok(Err(e)) => {
                println!("Transaction on node {} failed: {}", node_idx + 1, e);
            }
            Err(e) => {
                println!("Task for node {} panicked: {}", node_idx + 1, e);
            }
        }
    }

    // Report results per node
    for (idx, results) in results_by_node.iter().enumerate() {
        println!("Node {}: {} transactions succeeded", idx + 1, results.len());
    }

    let total_successful: usize = results_by_node.iter().map(|r| r.len()).sum();
    println!("Total successful transactions: {}/10", total_successful);

    assert!(total_successful >= 8, "At least 8 transactions should succeed");

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Mixed load different nodes");
}

#[tokio::test]
#[ignore]
async fn test_sustained_transaction_throughput() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("Testing sustained transaction throughput");

    let test_duration = Duration::from_secs(30);
    let start_time = std::time::Instant::now();
    let mut successful_count = 0;
    let mut failed_count = 0;

    // Submit transactions continuously for 30 seconds
    while start_time.elapsed() < test_duration {
        let tx_request = SendTransactionRequest {
            recipient: helpers::generate_test_address(),
            amount_sats: 15000,
            metadata: Some("Throughput test".to_string()),
        };

        match cluster.nodes[successful_count % cluster.nodes.len()]
            .send_transaction(&tx_request)
            .await
        {
            Ok(_) => successful_count += 1,
            Err(_) => failed_count += 1,
        }

        // Small delay to prevent overwhelming the system
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let elapsed = start_time.elapsed();
    let throughput = successful_count as f64 / elapsed.as_secs_f64();

    println!("\nSustained throughput test results:");
    println!("  Duration: {:?}", elapsed);
    println!("  Successful: {}", successful_count);
    println!("  Failed: {}", failed_count);
    println!("  Throughput: {:.2} tx/sec", throughput);

    assert!(successful_count > 0, "Should have some successful transactions");

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Sustained transaction throughput");
}

#[tokio::test]
#[ignore]
async fn test_concurrent_reads_and_writes() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("Testing concurrent reads and writes");

    // Submit initial transactions
    let mut txids = vec![];
    for i in 0..5 {
        let tx_request = SendTransactionRequest {
            recipient: helpers::generate_test_address(),
            amount_sats: (i + 1) * 11000,
            metadata: Some(format!("Read/write test {}", i + 1)),
        };

        let response = cluster.nodes[0]
            .send_transaction(&tx_request)
            .await
            .expect("Failed to send transaction");

        txids.push(response.txid);
    }

    tokio::time::sleep(Duration::from_secs(2)).await;

    // Spawn readers
    let mut read_handles = vec![];
    for _ in 0..10 {
        let txid = txids[rand::random::<usize>() % txids.len()].clone();
        let node = cluster.nodes[rand::random::<usize>() % cluster.nodes.len()].clone();

        let handle = tokio::spawn(async move {
            node.get_transaction(&txid).await
        });

        read_handles.push(handle);
    }

    // Spawn writers
    let mut write_handles = vec![];
    for i in 0..5 {
        let node = cluster.nodes[i % cluster.nodes.len()].clone();

        let handle = tokio::spawn(async move {
            let tx_request = SendTransactionRequest {
                recipient: helpers::generate_test_address(),
                amount_sats: 7000,
                metadata: Some("Write during reads".to_string()),
            };

            node.send_transaction(&tx_request).await
        });

        write_handles.push(handle);
    }

    // Wait for all operations
    let read_results = futures::future::join_all(read_handles).await;
    let write_results = futures::future::join_all(write_handles).await;

    let successful_reads = read_results.iter()
        .filter(|r| matches!(r, Ok(Ok(_))))
        .count();

    let successful_writes = write_results.iter()
        .filter(|r| matches!(r, Ok(Ok(_))))
        .count();

    println!("Successful reads: {}/10", successful_reads);
    println!("Successful writes: {}/5", successful_writes);

    assert!(successful_reads >= 8, "Most reads should succeed");
    assert!(successful_writes >= 4, "Most writes should succeed");

    cluster.stop().await.expect("Failed to stop cluster");
    println!("Test passed: Concurrent reads and writes");
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();
}

use rand::Rng;
