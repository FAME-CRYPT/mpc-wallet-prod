mod common;

use common::{Cluster, SendTransactionRequest, helpers};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tracing_subscriber;

#[tokio::test]
#[ignore]
async fn benchmark_transaction_throughput() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("=== Transaction Throughput Benchmark ===");

    let test_duration = Duration::from_secs(60);
    let start_time = Instant::now();
    let mut successful_count = 0;
    let mut failed_count = 0;
    let mut latencies = vec![];

    // Submit transactions continuously
    while start_time.elapsed() < test_duration {
        let tx_request = SendTransactionRequest {
            recipient: helpers::generate_test_address(),
            amount_sats: 25000,
            metadata: Some("Throughput benchmark".to_string()),
        };

        let submit_start = Instant::now();
        match cluster.nodes[successful_count % cluster.nodes.len()]
            .send_transaction(&tx_request)
            .await
        {
            Ok(_) => {
                let latency = submit_start.elapsed();
                latencies.push(latency.as_millis() as f64);
                successful_count += 1;
            }
            Err(_) => failed_count += 1,
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let total_elapsed = start_time.elapsed();
    let throughput = successful_count as f64 / total_elapsed.as_secs_f64();

    // Calculate latency statistics
    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = helpers::calculate_percentile(&latencies, 50.0);
    let p95 = helpers::calculate_percentile(&latencies, 95.0);
    let p99 = helpers::calculate_percentile(&latencies, 99.0);
    let avg = latencies.iter().sum::<f64>() / latencies.len() as f64;

    println!("\n=== Results ===");
    println!("Duration: {:?}", total_elapsed);
    println!("Successful: {}", successful_count);
    println!("Failed: {}", failed_count);
    println!("Throughput: {:.2} tx/sec", throughput);
    println!("\nLatency Statistics:");
    println!("  Average: {:.2} ms", avg);
    println!("  P50: {:.2} ms", p50);
    println!("  P95: {:.2} ms", p95);
    println!("  P99: {:.2} ms", p99);

    // Performance assertions
    assert!(throughput >= 5.0, "Throughput should be at least 5 tx/sec");
    assert!(p95 < 1000.0, "P95 latency should be under 1 second");

    cluster.stop().await.expect("Failed to stop cluster");
}

#[tokio::test]
#[ignore]
async fn benchmark_vote_propagation_latency() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("=== Vote Propagation Latency Benchmark ===");

    let mut propagation_times = vec![];

    for i in 0..20 {
        let tx_request = SendTransactionRequest {
            recipient: helpers::generate_test_address(),
            amount_sats: (i + 1) * 7000,
            metadata: Some(format!("Propagation test {}", i + 1)),
        };

        let submit_time = Instant::now();

        let response = cluster.nodes[0]
            .send_transaction(&tx_request)
            .await
            .expect("Failed to send transaction");

        let txid = response.txid.clone();

        // Poll until all nodes see the transaction
        let mut all_nodes_see_tx = false;
        let poll_start = Instant::now();

        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(100)).await;

            let mut count = 0;
            for node in &cluster.nodes {
                if node.get_transaction(&txid).await.is_ok() {
                    count += 1;
                }
            }

            if count == 5 {
                all_nodes_see_tx = true;
                break;
            }
        }

        if all_nodes_see_tx {
            let propagation_time = poll_start.elapsed();
            propagation_times.push(propagation_time.as_millis() as f64);
            println!("Transaction {} propagated in {:?}", i + 1, propagation_time);
        } else {
            println!("Transaction {} did not propagate to all nodes", i + 1);
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // Calculate statistics
    propagation_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = helpers::calculate_percentile(&propagation_times, 50.0);
    let p95 = helpers::calculate_percentile(&propagation_times, 95.0);
    let p99 = helpers::calculate_percentile(&propagation_times, 99.0);
    let avg = propagation_times.iter().sum::<f64>() / propagation_times.len() as f64;

    println!("\n=== Propagation Latency Statistics ===");
    println!("Samples: {}", propagation_times.len());
    println!("Average: {:.2} ms", avg);
    println!("P50: {:.2} ms", p50);
    println!("P95: {:.2} ms", p95);
    println!("P99: {:.2} ms", p99);

    assert!(p95 < 2000.0, "P95 propagation latency should be under 2 seconds");

    cluster.stop().await.expect("Failed to stop cluster");
}

#[tokio::test]
#[ignore]
async fn benchmark_consensus_completion_time() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("=== Consensus Completion Time Benchmark ===");

    let mut consensus_times = vec![];

    for i in 0..15 {
        let tx_request = SendTransactionRequest {
            recipient: helpers::generate_test_address(),
            amount_sats: (i + 1) * 9000,
            metadata: Some(format!("Consensus test {}", i + 1)),
        };

        let submit_time = Instant::now();

        let response = cluster.nodes[0]
            .send_transaction(&tx_request)
            .await
            .expect("Failed to send transaction");

        let txid = response.txid.clone();

        // Poll until consensus is reached
        let poll_start = Instant::now();
        let mut consensus_reached = false;

        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(200)).await;

            if let Ok(tx) = cluster.nodes[0].get_transaction(&txid).await {
                if matches!(
                    tx.state,
                    threshold_types::TransactionState::ThresholdReached
                        | threshold_types::TransactionState::Approved
                        | threshold_types::TransactionState::Signing
                ) {
                    consensus_reached = true;
                    break;
                }
            }
        }

        if consensus_reached {
            let consensus_time = poll_start.elapsed();
            consensus_times.push(consensus_time.as_millis() as f64);
            println!("Transaction {} reached consensus in {:?}", i + 1, consensus_time);
        } else {
            println!("Transaction {} did not reach consensus", i + 1);
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // Calculate statistics
    consensus_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = helpers::calculate_percentile(&consensus_times, 50.0);
    let p95 = helpers::calculate_percentile(&consensus_times, 95.0);
    let p99 = helpers::calculate_percentile(&consensus_times, 99.0);
    let avg = consensus_times.iter().sum::<f64>() / consensus_times.len() as f64;

    println!("\n=== Consensus Time Statistics ===");
    println!("Samples: {}", consensus_times.len());
    println!("Average: {:.2} ms", avg);
    println!("P50: {:.2} ms", p50);
    println!("P95: {:.2} ms", p95);
    println!("P99: {:.2} ms", p99);

    assert!(p95 < 5000.0, "P95 consensus time should be under 5 seconds");

    cluster.stop().await.expect("Failed to stop cluster");
}

#[tokio::test]
#[ignore]
async fn benchmark_api_response_times() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("=== API Response Time Benchmark ===");

    // Submit some transactions first
    let mut txids = vec![];
    for i in 0..5 {
        let tx_request = SendTransactionRequest {
            recipient: helpers::generate_test_address(),
            amount_sats: (i + 1) * 11000,
            metadata: Some(format!("API test {}", i + 1)),
        };

        if let Ok(response) = cluster.nodes[0].send_transaction(&tx_request).await {
            txids.push(response.txid);
        }
    }

    tokio::time::sleep(Duration::from_secs(3)).await;

    // Benchmark GET requests
    let mut get_latencies = vec![];

    for _ in 0..100 {
        if let Some(txid) = txids.get(rand::random::<usize>() % txids.len()) {
            let start = Instant::now();
            let _ = cluster.nodes[0].get_transaction(txid).await;
            let latency = start.elapsed();
            get_latencies.push(latency.as_micros() as f64 / 1000.0);
        }

        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // Benchmark LIST requests
    let mut list_latencies = vec![];

    for _ in 0..50 {
        let start = Instant::now();
        let _ = cluster.nodes[0].list_transactions().await;
        let latency = start.elapsed();
        list_latencies.push(latency.as_micros() as f64 / 1000.0);

        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // Calculate statistics
    get_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    list_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());

    println!("\n=== GET Transaction Response Times ===");
    println!("Average: {:.2} ms", get_latencies.iter().sum::<f64>() / get_latencies.len() as f64);
    println!("P50: {:.2} ms", helpers::calculate_percentile(&get_latencies, 50.0));
    println!("P95: {:.2} ms", helpers::calculate_percentile(&get_latencies, 95.0));
    println!("P99: {:.2} ms", helpers::calculate_percentile(&get_latencies, 99.0));

    println!("\n=== LIST Transactions Response Times ===");
    println!("Average: {:.2} ms", list_latencies.iter().sum::<f64>() / list_latencies.len() as f64);
    println!("P50: {:.2} ms", helpers::calculate_percentile(&list_latencies, 50.0));
    println!("P95: {:.2} ms", helpers::calculate_percentile(&list_latencies, 95.0));
    println!("P99: {:.2} ms", helpers::calculate_percentile(&list_latencies, 99.0));

    let get_p99 = helpers::calculate_percentile(&get_latencies, 99.0);
    assert!(get_p99 < 200.0, "GET P99 should be under 200ms");

    cluster.stop().await.expect("Failed to stop cluster");
}

#[tokio::test]
#[ignore]
async fn benchmark_concurrent_load() {
    init_tracing();

    let compose_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docker-compose.e2e.yml");
    let test_name = helpers::test_id();

    let cluster = Cluster::start(compose_file, &test_name)
        .await
        .expect("Failed to start cluster");

    println!("=== Concurrent Load Benchmark ===");

    let concurrent_requests = 20;
    let mut handles = vec![];

    let start_time = Instant::now();

    for i in 0..concurrent_requests {
        let node = cluster.nodes[i % cluster.nodes.len()].clone();

        let handle = tokio::spawn(async move {
            let tx_request = SendTransactionRequest {
                recipient: helpers::generate_test_address(),
                amount_sats: ((i + 1) * 6000) as u64,
                metadata: Some(format!("Concurrent load {}", i + 1)),
            };

            let submit_start = Instant::now();
            let result = node.send_transaction(&tx_request).await;
            let latency = submit_start.elapsed();

            (result.is_ok(), latency)
        });

        handles.push(handle);
    }

    let results = futures::future::join_all(handles).await;
    let total_time = start_time.elapsed();

    let successful = results.iter()
        .filter(|r| matches!(r, Ok((true, _))))
        .count();

    let latencies: Vec<f64> = results.iter()
        .filter_map(|r| {
            if let Ok((true, latency)) = r {
                Some(latency.as_millis() as f64)
            } else {
                None
            }
        })
        .collect();

    println!("\n=== Concurrent Load Results ===");
    println!("Total requests: {}", concurrent_requests);
    println!("Successful: {}", successful);
    println!("Failed: {}", concurrent_requests - successful);
    println!("Total time: {:?}", total_time);

    if !latencies.is_empty() {
        let mut sorted = latencies.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        println!("\nLatency under concurrent load:");
        println!("  Average: {:.2} ms", latencies.iter().sum::<f64>() / latencies.len() as f64);
        println!("  P50: {:.2} ms", helpers::calculate_percentile(&sorted, 50.0));
        println!("  P95: {:.2} ms", helpers::calculate_percentile(&sorted, 95.0));
        println!("  P99: {:.2} ms", helpers::calculate_percentile(&sorted, 99.0));
    }

    cluster.stop().await.expect("Failed to stop cluster");
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();
}

use rand::Rng;
