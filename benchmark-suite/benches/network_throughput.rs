use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mpc_benchmark_suite::*;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

async fn measure_throughput(
    system: SystemType,
    vote_count: usize,
    concurrent: usize,
) -> PerformanceMetrics {
    let mut metrics = PerformanceMetrics::new();

    // Start the system
    docker::start_system(system).expect("Failed to start system");

    // Wait for system to be ready
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Record latencies
    let mut latency_recorder = metrics::LatencyRecorder::new();

    // Send votes and measure
    let start = Instant::now();
    let mut sent_count = 0u64;
    let mut received_count = 0u64;
    let mut error_count = 0u64;

    // Batch votes in chunks for concurrency
    let chunk_size = concurrent;
    for chunk_start in (0..vote_count).step_by(chunk_size) {
        let chunk_end = (chunk_start + chunk_size).min(vote_count);
        let mut tasks = vec![];

        for i in chunk_start..chunk_end {
            let task = tokio::spawn(async move {
                let vote_start = Instant::now();

                // Send vote (simulated - would use actual API)
                let tx_id = format!("BENCH_TX_{:06}", i);
                let result = send_vote(&tx_id, i as u64 % 1000).await;

                let latency = vote_start.elapsed().as_micros() as u64;

                (result, latency)
            });

            tasks.push(task);
        }

        // Wait for all tasks in this chunk
        for task in tasks {
            match task.await {
                Ok((Ok(_), latency)) => {
                    sent_count += 1;
                    received_count += 1;
                    latency_recorder.record(latency);
                }
                Ok((Err(_), latency)) => {
                    sent_count += 1;
                    error_count += 1;
                    latency_recorder.record(latency);
                }
                Err(_) => {
                    error_count += 1;
                }
            }
        }
    }

    let elapsed = start.elapsed();

    // Calculate throughput
    metrics.votes_per_second = sent_count as f64 / elapsed.as_secs_f64();
    metrics.messages_per_second = (sent_count * 4) as f64 / elapsed.as_secs_f64(); // 4 messages per vote (broadcast to 4 peers)

    // Calculate latencies
    metrics.latency_p50 = latency_recorder.percentile(50.0);
    metrics.latency_p95 = latency_recorder.percentile(95.0);
    metrics.latency_p99 = latency_recorder.percentile(99.0);
    metrics.latency_max = latency_recorder.max();
    metrics.latency_mean = latency_recorder.mean();
    metrics.latency_stddev = latency_recorder.stddev();

    // Get resource usage
    if let Ok(stats) = docker::get_container_stats(system, 1) {
        metrics.cpu_usage_percent = parse_cpu_percent(&stats.cpu_perc);
        metrics.memory_usage_mb = parse_memory_mb(&stats.mem_usage);
        let (sent, received) = parse_network_io(&stats.net_io);
        metrics.network_bytes_sent = sent;
        metrics.network_bytes_received = received;
    }

    // Reliability metrics
    metrics.total_votes_sent = sent_count;
    metrics.total_votes_received = received_count;
    metrics.delivery_success_rate = (received_count as f64 / sent_count as f64) * 100.0;
    metrics.error_count = error_count;

    metrics.node_count = 5;

    // Stop the system
    docker::stop_system(system).expect("Failed to stop system");

    metrics
}

async fn send_vote(tx_id: &str, value: u64) -> anyhow::Result<()> {
    // This would call the actual API endpoint
    // For now, simulate with a small delay
    tokio::time::sleep(Duration::from_micros(100)).await;
    Ok(())
}

fn parse_cpu_percent(s: &str) -> f64 {
    s.trim_end_matches('%').parse().unwrap_or(0.0)
}

fn parse_memory_mb(s: &str) -> f64 {
    // Parse "123.4MiB / 1GiB" format
    let parts: Vec<&str> = s.split('/').collect();
    if parts.is_empty() {
        return 0.0;
    }

    let mem_str = parts[0].trim();
    let value: f64 = mem_str
        .chars()
        .take_while(|c| c.is_numeric() || *c == '.')
        .collect::<String>()
        .parse()
        .unwrap_or(0.0);

    if mem_str.contains("GiB") || mem_str.contains("GB") {
        value * 1024.0
    } else {
        value
    }
}

fn parse_network_io(s: &str) -> (u64, u64) {
    // Parse "1.2kB / 3.4kB" format
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() != 2 {
        return (0, 0);
    }

    let sent = parse_bytes(parts[0].trim());
    let received = parse_bytes(parts[1].trim());

    (sent, received)
}

fn parse_bytes(s: &str) -> u64 {
    let value: f64 = s
        .chars()
        .take_while(|c| c.is_numeric() || *c == '.')
        .collect::<String>()
        .parse()
        .unwrap_or(0.0);

    if s.contains("GB") {
        (value * 1_000_000_000.0) as u64
    } else if s.contains("MB") {
        (value * 1_000_000.0) as u64
    } else if s.contains("kB") || s.contains("KB") {
        (value * 1_000.0) as u64
    } else {
        value as u64
    }
}

fn benchmark_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("network_throughput");

    // Test different vote counts
    for vote_count in [100, 500, 1000, 5000].iter() {
        // p2p-comm (libp2p)
        group.throughput(Throughput::Elements(*vote_count as u64));
        group.bench_with_input(
            BenchmarkId::new("p2p-comm", vote_count),
            vote_count,
            |b, &vote_count| {
                b.to_async(&rt).iter(|| {
                    measure_throughput(
                        black_box(SystemType::P2pComm),
                        black_box(vote_count),
                        black_box(10),
                    )
                });
            },
        );

        // mtls-comm (pure mTLS)
        group.bench_with_input(
            BenchmarkId::new("mtls-comm", vote_count),
            vote_count,
            |b, &vote_count| {
                b.to_async(&rt).iter(|| {
                    measure_throughput(
                        black_box(SystemType::MtlsComm),
                        black_box(vote_count),
                        black_box(10),
                    )
                });
            },
        );
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(60))
        .warm_up_time(Duration::from_secs(10));
    targets = benchmark_throughput
}

criterion_main!(benches);
