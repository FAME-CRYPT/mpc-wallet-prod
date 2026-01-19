/// Integration benchmark runner that tests actual system performance
///
/// This module provides tools to benchmark both p2p-comm and mtls-comm
/// systems by running real voting operations and measuring actual performance.

use crate::*;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tokio::time::sleep;

pub struct IntegrationBenchmark {
    pub system: SystemType,
    pub work_dir: PathBuf,
    pub node_count: usize,
}

impl IntegrationBenchmark {
    pub fn new_sharedmem() -> Self {
        // Use relative path from benchmark-suite directory
        let work_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("Failed to get parent directory")
            .join("p2p-comm");

        Self {
            system: SystemType::P2pComm,
            work_dir,
            node_count: 5,
        }
    }

    pub fn new_with_mtls() -> Self {
        // Use relative path from benchmark-suite directory
        let work_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("Failed to get parent directory")
            .join("mtls-comm");

        Self {
            system: SystemType::MtlsComm,
            work_dir,
            node_count: 5,
        }
    }

    /// Build the system in release mode
    pub fn build(&self) -> anyhow::Result<()> {
        println!("Building {:?} in release mode...", self.system);

        let output = Command::new("cargo")
            .current_dir(&self.work_dir)
            .arg("build")
            .arg("--release")
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Build failed: {}", stderr);
        }

        println!("✓ Build completed successfully");
        Ok(())
    }

    /// Get the binary name for the system
    fn get_binary_name(&self) -> &str {
        match self.system {
            SystemType::P2pComm => "threshold-voting-system",
            SystemType::MtlsComm => "threshold-voting",
        }
    }

    /// Submit a vote using the CLI
    pub async fn submit_vote(&self, node_id: u64, tx_id: &str, value: u64) -> anyhow::Result<Duration> {
        let start = Instant::now();

        let binary_name = self.get_binary_name();
        let binary = self.work_dir.join(format!("target/release/{}", binary_name));

        let output = Command::new(&binary)
            .current_dir(&self.work_dir)
            .env("NODE_ID", node_id.to_string())
            .env("RUST_LOG", "error") // Minimize logging during benchmarks
            .arg("vote")
            .arg("--tx-id")
            .arg(tx_id)
            .arg("--value")
            .arg(value.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output()?;

        let elapsed = start.elapsed();

        if !output.status.success() {
            anyhow::bail!("Vote submission failed");
        }

        Ok(elapsed)
    }

    /// Query transaction status
    pub async fn query_status(&self, tx_id: &str) -> anyhow::Result<String> {
        let binary_name = self.get_binary_name();
        let binary = self.work_dir.join(format!("target/release/{}", binary_name));

        let output = Command::new(&binary)
            .current_dir(&self.work_dir)
            .env("NODE_ID", "1")
            .env("RUST_LOG", "error")
            .arg("status")
            .arg("--tx-id")
            .arg(tx_id)
            .output()?;

        if !output.status.success() {
            anyhow::bail!("Status query failed");
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Run throughput benchmark
    pub async fn benchmark_throughput(&self, vote_count: usize, concurrent: usize) -> anyhow::Result<PerformanceMetrics> {
        println!("\n=== Throughput Benchmark for {:?} ===", self.system);
        println!("  Vote Count: {}", vote_count);
        println!("  Concurrent: {}", concurrent);

        let mut metrics = PerformanceMetrics::new();
        let mut latency_recorder = metrics::LatencyRecorder::new();

        let start = Instant::now();
        let mut sent_count = 0u64;
        let mut error_count = 0u64;

        // Submit votes in batches
        for chunk_start in (0..vote_count).step_by(concurrent) {
            let chunk_end = (chunk_start + concurrent).min(vote_count);
            let mut tasks = vec![];

            for i in chunk_start..chunk_end {
                let tx_id = format!("BENCH_TX_{:06}", i);
                let node_id = (i % self.node_count) as u64 + 1;
                let value = (i % 100) as u64;

                let bench = Self {
                    system: self.system,
                    work_dir: self.work_dir.clone(),
                    node_count: self.node_count,
                };

                let task = tokio::spawn(async move {
                    bench.submit_vote(node_id, &tx_id, value).await
                });

                tasks.push(task);
            }

            // Wait for batch completion
            for task in tasks {
                match task.await {
                    Ok(Ok(latency)) => {
                        sent_count += 1;
                        latency_recorder.record(latency.as_micros() as u64);
                    }
                    _ => {
                        error_count += 1;
                    }
                }
            }

            // Progress indicator
            if chunk_end % 100 == 0 || chunk_end == vote_count {
                println!("  Progress: {}/{} votes sent", chunk_end, vote_count);
            }
        }

        let elapsed = start.elapsed();

        // Calculate metrics
        metrics.votes_per_second = sent_count as f64 / elapsed.as_secs_f64();
        metrics.messages_per_second = (sent_count * (self.node_count as u64 - 1)) as f64 / elapsed.as_secs_f64();
        metrics.bytes_per_second = metrics.messages_per_second * 512.0; // Assume ~512 bytes per message

        // Latency metrics
        metrics.latency_p50 = latency_recorder.percentile(50.0);
        metrics.latency_p95 = latency_recorder.percentile(95.0);
        metrics.latency_p99 = latency_recorder.percentile(99.0);
        metrics.latency_max = latency_recorder.max();
        metrics.latency_mean = latency_recorder.mean();
        metrics.latency_stddev = latency_recorder.stddev();

        // Reliability metrics
        metrics.total_votes_sent = sent_count;
        metrics.total_votes_received = sent_count; // Assume successful sends are received
        metrics.delivery_success_rate = ((sent_count - error_count) as f64 / sent_count as f64) * 100.0;
        metrics.error_count = error_count;
        metrics.node_count = self.node_count;

        println!("\n✓ Benchmark completed");
        println!("  Votes/sec:     {:.2}", metrics.votes_per_second);
        println!("  Messages/sec:  {:.2}", metrics.messages_per_second);
        println!("  Latency (p50): {} μs", metrics.latency_p50);
        println!("  Latency (p95): {} μs", metrics.latency_p95);
        println!("  Errors:        {}", error_count);

        Ok(metrics)
    }

    /// Run comprehensive benchmark suite
    pub async fn run_comprehensive_benchmark(&self) -> anyhow::Result<BenchmarkResult> {
        println!("\n╔════════════════════════════════════════════════╗");
        println!("║  Comprehensive Benchmark: {:?}  ║", self.system);
        println!("╚════════════════════════════════════════════════╝\n");

        // Warm-up
        println!("Phase 1: Warm-up (50 votes)");
        self.benchmark_throughput(50, 10).await?;
        sleep(Duration::from_secs(2)).await;

        // Main throughput test
        println!("\nPhase 2: Throughput Test (1000 votes)");
        let metrics = self.benchmark_throughput(1000, 20).await?;

        // Cool-down
        sleep(Duration::from_secs(2)).await;

        Ok(BenchmarkResult {
            system: self.system,
            timestamp: chrono::Utc::now(),
            metrics,
        })
    }
}

/// Run comparison benchmark between both systems
pub async fn run_comparison_benchmark() -> anyhow::Result<Vec<BenchmarkResult>> {
    println!("\n");
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                                                              ║");
    println!("║       MPC WALLET BENCHMARK SUITE - SYSTEM COMPARISON         ║");
    println!("║                                                              ║");
    println!("║  Comparing:                                                  ║");
    println!("║    • p2p-comm (libp2p + Noise Protocol + GossipSub)  ║");
    println!("║    • mtls-comm (Pure mTLS 1.3 + rustls)               ║");
    println!("║                                                              ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    let mut results = Vec::new();

    // Build both systems
    println!("═══ Building Systems ═══\n");

    let bench_sharedmem = IntegrationBenchmark::new_sharedmem();
    println!("[1/2] Building p2p-comm...");
    bench_sharedmem.build()?;

    let bench_with_mtls = IntegrationBenchmark::new_with_mtls();
    println!("[2/2] Building mtls-comm...");
    bench_with_mtls.build()?;

    println!("\n✓ All systems built successfully\n");

    // Run benchmarks
    println!("═══ Running Benchmarks ═══\n");

    println!(">>> Benchmarking System 1: p2p-comm <<<");
    let result1 = bench_sharedmem.run_comprehensive_benchmark().await?;
    results.push(result1);

    println!("\n\n>>> Benchmarking System 2: mtls-comm <<<");
    let result2 = bench_with_mtls.run_comprehensive_benchmark().await?;
    results.push(result2);

    Ok(results)
}

/// Print comparison summary
pub fn print_comparison_summary(results: &[BenchmarkResult]) {
    if results.len() < 2 {
        println!("Need at least 2 results for comparison");
        return;
    }

    let sharedmem = &results[0].metrics;
    let with_mtls = &results[1].metrics;

    println!("\n\n");
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                    BENCHMARK COMPARISON                      ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    println!("┌─────────────────────────────────┬─────────────────┬─────────────────┬───────────────┐");
    println!("│ Metric                          │ p2p-comm  │ mtls-comm  │ Difference    │");
    println!("├─────────────────────────────────┼─────────────────┼─────────────────┼───────────────┤");

    // Throughput
    println!("│ Votes/second                    │ {:>15.2} │ {:>15.2} │ {:>12.1}% │",
        sharedmem.votes_per_second,
        with_mtls.votes_per_second,
        ((with_mtls.votes_per_second - sharedmem.votes_per_second) / sharedmem.votes_per_second) * 100.0
    );

    println!("│ Messages/second                 │ {:>15.2} │ {:>15.2} │ {:>12.1}% │",
        sharedmem.messages_per_second,
        with_mtls.messages_per_second,
        ((with_mtls.messages_per_second - sharedmem.messages_per_second) / sharedmem.messages_per_second) * 100.0
    );

    // Latency
    println!("│ Latency p50 (μs)                │ {:>15} │ {:>15} │ {:>12.1}% │",
        sharedmem.latency_p50,
        with_mtls.latency_p50,
        ((with_mtls.latency_p50 as f64 - sharedmem.latency_p50 as f64) / sharedmem.latency_p50 as f64) * 100.0
    );

    println!("│ Latency p95 (μs)                │ {:>15} │ {:>15} │ {:>12.1}% │",
        sharedmem.latency_p95,
        with_mtls.latency_p95,
        ((with_mtls.latency_p95 as f64 - sharedmem.latency_p95 as f64) / sharedmem.latency_p95 as f64) * 100.0
    );

    println!("│ Latency p99 (μs)                │ {:>15} │ {:>15} │ {:>12.1}% │",
        sharedmem.latency_p99,
        with_mtls.latency_p99,
        ((with_mtls.latency_p99 as f64 - sharedmem.latency_p99 as f64) / sharedmem.latency_p99 as f64) * 100.0
    );

    // Reliability
    println!("│ Delivery Success Rate (%)       │ {:>15.2} │ {:>15.2} │ {:>12.1}% │",
        sharedmem.delivery_success_rate,
        with_mtls.delivery_success_rate,
        ((with_mtls.delivery_success_rate - sharedmem.delivery_success_rate) / sharedmem.delivery_success_rate) * 100.0
    );

    println!("└─────────────────────────────────┴─────────────────┴─────────────────┴───────────────┘");

    // Winner determination
    println!("\n🏆 PERFORMANCE WINNER\n");

    let mut sharedmem_wins = 0;
    let mut mtls_wins = 0;

    if sharedmem.votes_per_second > with_mtls.votes_per_second {
        sharedmem_wins += 1;
        println!("  ✓ Throughput:         p2p-comm");
    } else {
        mtls_wins += 1;
        println!("  ✓ Throughput:         mtls-comm");
    }

    if sharedmem.latency_p95 < with_mtls.latency_p95 {
        sharedmem_wins += 1;
        println!("  ✓ Latency (p95):      p2p-comm");
    } else {
        mtls_wins += 1;
        println!("  ✓ Latency (p95):      mtls-comm");
    }

    if sharedmem.delivery_success_rate > with_mtls.delivery_success_rate {
        sharedmem_wins += 1;
        println!("  ✓ Reliability:        p2p-comm");
    } else {
        mtls_wins += 1;
        println!("  ✓ Reliability:        mtls-comm");
    }

    println!("\n");
    if sharedmem_wins > mtls_wins {
        println!("  🎉 Overall Winner: p2p-comm ({} categories)", sharedmem_wins);
    } else if mtls_wins > sharedmem_wins {
        println!("  🎉 Overall Winner: mtls-comm ({} categories)", mtls_wins);
    } else {
        println!("  🤝 Result: TIE (both systems equal)");
    }
    println!();
}
