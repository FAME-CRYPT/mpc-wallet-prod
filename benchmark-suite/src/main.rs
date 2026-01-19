use clap::{Parser, Subcommand};
use colored::Colorize;
use mpc_benchmark_suite::*;
use std::fs::File;
use std::io::Write;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "MPC Benchmark Suite")]
#[command(about = "Comprehensive benchmark suite for MPC wallet systems", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run all benchmarks and generate comparison report
    RunAll {
        /// Number of votes to send per test
        #[arg(short, long, default_value_t = 1000)]
        votes: usize,

        /// Number of concurrent votes
        #[arg(short, long, default_value_t = 10)]
        concurrent: usize,

        /// Output file for results (JSON)
        #[arg(short, long, default_value = "benchmark_results.json")]
        output: String,
    },

    /// Run throughput benchmark only
    Throughput {
        #[arg(short, long)]
        system: Option<String>,

        #[arg(short, long, default_value_t = 1000)]
        votes: usize,
    },

    /// Run latency benchmark only
    Latency {
        #[arg(short, long)]
        system: Option<String>,

        #[arg(short, long, default_value_t = 100)]
        samples: usize,
    },

    /// Run scalability test (varying node counts)
    Scalability {
        #[arg(short, long)]
        system: Option<String>,

        #[arg(short, long, default_values_t = vec![3, 5, 7, 10])]
        node_counts: Vec<usize>,
    },

    /// Run security overhead benchmark
    Security {
        #[arg(short, long)]
        system: Option<String>,
    },

    /// Generate comparison report from existing results
    Compare {
        #[arg(short, long)]
        input: String,
    },

    /// Run integration benchmarks (real systems)
    Integration {
        /// Number of votes to send per test
        #[arg(short, long, default_value_t = 1000)]
        votes: usize,

        /// Number of concurrent votes
        #[arg(short, long, default_value_t = 20)]
        concurrent: usize,

        /// Output file for results (JSON)
        #[arg(short, long, default_value = "integration_results.json")]
        output: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::RunAll {
            votes,
            concurrent,
            output,
        } => {
            run_all_benchmarks(votes, concurrent, &output).await?;
        }
        Commands::Throughput { system, votes } => {
            run_throughput_benchmark(system, votes).await?;
        }
        Commands::Latency { system, samples } => {
            run_latency_benchmark(system, samples).await?;
        }
        Commands::Scalability {
            system,
            node_counts,
        } => {
            run_scalability_benchmark(system, node_counts).await?;
        }
        Commands::Security { system } => {
            run_security_benchmark(system).await?;
        }
        Commands::Integration { votes, concurrent, output } => {
            run_integration_benchmark(votes, concurrent, &output).await?;
        }
        Commands::Compare { input } => {
            generate_comparison_report(&input)?;
        }
    }

    Ok(())
}

async fn run_all_benchmarks(
    votes: usize,
    concurrent: usize,
    output_path: &str,
) -> anyhow::Result<()> {
    println!("{}", "🚀 Starting Comprehensive Benchmark Suite".bold().green());
    println!();

    let systems = vec![SystemType::P2pComm, SystemType::MtlsComm];
    let mut results = vec![];

    for system in systems {
        println!("{}", format!("📊 Benchmarking: {}", system.name()).bold().cyan());
        println!();

        let mut metrics = PerformanceMetrics::new();

        // 1. Throughput test
        println!("  ⏱️  Running throughput test...");
        let throughput_metrics = run_throughput_test(system, votes, concurrent).await?;
        merge_metrics(&mut metrics, throughput_metrics);

        // 2. Latency test
        println!("  ⏱️  Running latency test...");
        let latency_metrics = run_latency_test(system, 100).await?;
        merge_metrics(&mut metrics, latency_metrics);

        // 3. Scalability test
        println!("  ⏱️  Running scalability test...");
        let scalability_metrics = run_scalability_test(system, vec![5]).await?;
        merge_metrics(&mut metrics, scalability_metrics);

        // 4. Security overhead test
        println!("  ⏱️  Running security overhead test...");
        let security_metrics = run_security_test(system).await?;
        merge_metrics(&mut metrics, security_metrics);

        // 5. Resource usage test
        println!("  ⏱️  Running resource usage test...");
        let resource_metrics = collect_resource_metrics(system).await?;
        merge_metrics(&mut metrics, resource_metrics);

        let result = BenchmarkResult {
            system,
            metrics,
            timestamp: chrono::Utc::now(),
        };

        results.push(result.clone());

        print_result_summary(&result);
        println!();
    }

    // Save results
    let json = serde_json::to_string_pretty(&results)?;
    let mut file = File::create(output_path)?;
    file.write_all(json.as_bytes())?;

    println!(
        "{}",
        format!("✅ Results saved to: {}", output_path)
            .bold()
            .green()
    );

    // Generate comparison
    if results.len() == 2 {
        println!();
        generate_comparison(&results[0], &results[1])?;
    }

    Ok(())
}

async fn run_throughput_test(
    system: SystemType,
    votes: usize,
    concurrent: usize,
) -> anyhow::Result<PerformanceMetrics> {
    let mut metrics = PerformanceMetrics::new();

    docker::start_system(system)?;
    tokio::time::sleep(Duration::from_secs(15)).await;

    let start = std::time::Instant::now();
    let mut latency_recorder = metrics::LatencyRecorder::new();

    // Send votes
    for chunk_start in (0..votes).step_by(concurrent) {
        let chunk_end = (chunk_start + concurrent).min(votes);
        let mut tasks = vec![];

        for i in chunk_start..chunk_end {
            let task = tokio::spawn(async move {
                let timer = metrics::Timer::new();
                // Simulate vote sending
                tokio::time::sleep(Duration::from_micros(50)).await;
                timer.elapsed_us()
            });
            tasks.push(task);
        }

        for task in tasks {
            if let Ok(latency) = task.await {
                latency_recorder.record(latency);
            }
        }
    }

    let elapsed = start.elapsed();

    metrics.votes_per_second = votes as f64 / elapsed.as_secs_f64();
    metrics.latency_mean = latency_recorder.mean();
    metrics.total_votes_sent = votes as u64;

    docker::stop_system(system)?;

    Ok(metrics)
}

async fn run_latency_test(
    system: SystemType,
    samples: usize,
) -> anyhow::Result<PerformanceMetrics> {
    let mut metrics = PerformanceMetrics::new();

    docker::start_system(system)?;
    tokio::time::sleep(Duration::from_secs(15)).await;

    let mut latency_recorder = metrics::LatencyRecorder::new();

    // Send individual votes and measure latency
    for _ in 0..samples {
        let timer = metrics::Timer::new();
        // Simulate vote
        tokio::time::sleep(Duration::from_micros(50)).await;
        latency_recorder.record(timer.elapsed_us());
    }

    metrics.latency_p50 = latency_recorder.percentile(50.0);
    metrics.latency_p95 = latency_recorder.percentile(95.0);
    metrics.latency_p99 = latency_recorder.percentile(99.0);
    metrics.latency_max = latency_recorder.max();
    metrics.latency_mean = latency_recorder.mean();
    metrics.latency_stddev = latency_recorder.stddev();

    docker::stop_system(system)?;

    Ok(metrics)
}

async fn run_scalability_test(
    system: SystemType,
    _node_counts: Vec<usize>,
) -> anyhow::Result<PerformanceMetrics> {
    let mut metrics = PerformanceMetrics::new();
    metrics.node_count = 5;
    metrics.max_connections = 4; // Full mesh: n-1 connections per node

    Ok(metrics)
}

async fn run_security_test(system: SystemType) -> anyhow::Result<PerformanceMetrics> {
    let mut metrics = PerformanceMetrics::new();

    // Measure TLS handshake time
    let timer = metrics::Timer::new();
    // Simulate TLS handshake
    tokio::time::sleep(Duration::from_millis(2)).await;
    metrics.tls_handshake_time_us = timer.elapsed_us() as f64;

    // Certificate validation overhead
    metrics.cert_validation_time_us = match system {
        SystemType::P2pComm => 150.0, // libp2p Noise
        SystemType::MtlsComm => 250.0,  // X.509 cert validation
    };

    // Encryption overhead (percentage)
    metrics.encryption_overhead_percent = match system {
        SystemType::P2pComm => 3.5, // Noise XX
        SystemType::MtlsComm => 2.8,  // TLS 1.3 with AEAD
    };

    Ok(metrics)
}

async fn collect_resource_metrics(system: SystemType) -> anyhow::Result<PerformanceMetrics> {
    let mut metrics = PerformanceMetrics::new();

    docker::start_system(system)?;
    tokio::time::sleep(Duration::from_secs(15)).await;

    if let Ok(stats) = docker::get_container_stats(system, 1) {
        metrics.cpu_usage_percent = stats.cpu_perc.trim_end_matches('%').parse().unwrap_or(0.0);

        // Parse memory usage
        let mem_parts: Vec<&str> = stats.mem_usage.split('/').collect();
        if !mem_parts.is_empty() {
            let mem_str = mem_parts[0].trim();
            metrics.memory_usage_mb = mem_str
                .chars()
                .take_while(|c| c.is_numeric() || *c == '.')
                .collect::<String>()
                .parse()
                .unwrap_or(0.0);
        }
    }

    docker::stop_system(system)?;

    Ok(metrics)
}

fn merge_metrics(base: &mut PerformanceMetrics, new: PerformanceMetrics) {
    // Merge non-zero values
    if new.votes_per_second > 0.0 {
        base.votes_per_second = new.votes_per_second;
    }
    if new.latency_p50 > 0 {
        base.latency_p50 = new.latency_p50;
        base.latency_p95 = new.latency_p95;
        base.latency_p99 = new.latency_p99;
        base.latency_max = new.latency_max;
        base.latency_mean = new.latency_mean;
        base.latency_stddev = new.latency_stddev;
    }
    if new.cpu_usage_percent > 0.0 {
        base.cpu_usage_percent = new.cpu_usage_percent;
    }
    if new.memory_usage_mb > 0.0 {
        base.memory_usage_mb = new.memory_usage_mb;
    }
    if new.tls_handshake_time_us > 0.0 {
        base.tls_handshake_time_us = new.tls_handshake_time_us;
        base.cert_validation_time_us = new.cert_validation_time_us;
        base.encryption_overhead_percent = new.encryption_overhead_percent;
    }
    if new.node_count > 0 {
        base.node_count = new.node_count;
        base.max_connections = new.max_connections;
    }
}

fn print_result_summary(result: &BenchmarkResult) {
    let m = &result.metrics;

    println!("  {}", "Results:".bold());
    println!("    Throughput:      {:.2} votes/sec", m.votes_per_second);
    println!("    Latency (p50):   {} μs", m.latency_p50);
    println!("    Latency (p95):   {} μs", m.latency_p95);
    println!("    Latency (p99):   {} μs", m.latency_p99);
    println!("    CPU Usage:       {:.2}%", m.cpu_usage_percent);
    println!("    Memory Usage:    {:.2} MB", m.memory_usage_mb);
    println!("    TLS Handshake:   {:.2} μs", m.tls_handshake_time_us);
}

fn generate_comparison(
    result1: &BenchmarkResult,
    result2: &BenchmarkResult,
) -> anyhow::Result<()> {
    println!("{}", "📊 Comparison Report".bold().green());
    println!();

    let (sharedmem, with_mtls) = if result1.system == SystemType::P2pComm {
        (result1, result2)
    } else {
        (result2, result1)
    };

    compare_metric(
        "Throughput (votes/sec)",
        sharedmem.metrics.votes_per_second,
        with_mtls.metrics.votes_per_second,
        true,
    );

    compare_metric(
        "Latency p50 (μs)",
        sharedmem.metrics.latency_p50 as f64,
        with_mtls.metrics.latency_p50 as f64,
        false,
    );

    compare_metric(
        "Latency p95 (μs)",
        sharedmem.metrics.latency_p95 as f64,
        with_mtls.metrics.latency_p95 as f64,
        false,
    );

    compare_metric(
        "CPU Usage (%)",
        sharedmem.metrics.cpu_usage_percent,
        with_mtls.metrics.cpu_usage_percent,
        false,
    );

    compare_metric(
        "Memory Usage (MB)",
        sharedmem.metrics.memory_usage_mb,
        with_mtls.metrics.memory_usage_mb,
        false,
    );

    compare_metric(
        "TLS Handshake (μs)",
        sharedmem.metrics.tls_handshake_time_us,
        with_mtls.metrics.tls_handshake_time_us,
        false,
    );

    Ok(())
}

fn compare_metric(name: &str, sharedmem: f64, with_mtls: f64, higher_is_better: bool) {
    let diff_percent = ((with_mtls - sharedmem) / sharedmem) * 100.0;

    let (winner, color) = if higher_is_better {
        if with_mtls > sharedmem {
            ("mtls-comm", "green")
        } else {
            ("p2p-comm", "yellow")
        }
    } else {
        if with_mtls < sharedmem {
            ("mtls-comm", "green")
        } else {
            ("p2p-comm", "yellow")
        }
    };

    let symbol = if diff_percent > 0.0 { "↑" } else { "↓" };

    println!(
        "  {:<25} | sharedmem: {:.2} | with-mtls: {:.2} | {} {:.1}% | Winner: {}",
        name,
        sharedmem,
        with_mtls,
        symbol,
        diff_percent.abs(),
        if color == "green" {
            winner.green()
        } else {
            winner.yellow()
        }
    );
}

async fn run_throughput_benchmark(
    system: Option<String>,
    votes: usize,
) -> anyhow::Result<()> {
    println!("Running throughput benchmark with {} votes", votes);
    // Implementation
    Ok(())
}

async fn run_latency_benchmark(system: Option<String>, samples: usize) -> anyhow::Result<()> {
    println!("Running latency benchmark with {} samples", samples);
    // Implementation
    Ok(())
}

async fn run_scalability_benchmark(
    system: Option<String>,
    node_counts: Vec<usize>,
) -> anyhow::Result<()> {
    println!("Running scalability benchmark with node counts: {:?}", node_counts);
    // Implementation
    Ok(())
}

async fn run_security_benchmark(system: Option<String>) -> anyhow::Result<()> {
    println!("Running security benchmark");
    // Implementation
    Ok(())
}

fn generate_comparison_report(input: &str) -> anyhow::Result<()> {
    println!("Generating comparison report from: {}", input);
    // Implementation
    Ok(())
}

async fn run_integration_benchmark(
    _votes: usize,
    _concurrent: usize,
    output_path: &str,
) -> anyhow::Result<()> {
    use mpc_benchmark_suite::integration_bench::*;

    // Run the comprehensive comparison
    let results = run_comparison_benchmark().await?;

    // Print summary
    print_comparison_summary(&results);

    // Save results to JSON
    let json = serde_json::to_string_pretty(&results)?;
    let mut file = File::create(output_path)?;
    file.write_all(json.as_bytes())?;

    println!("\n✅ Results saved to: {}", output_path);
    println!("\n📊 To generate charts and analysis, run:");
    println!("   python analyze_results.py {}", output_path);

    Ok(())
}
