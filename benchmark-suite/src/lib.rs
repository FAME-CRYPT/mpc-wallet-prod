use serde::{Deserialize, Serialize};
use std::time::Duration;

pub mod integration_bench;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    pub system: SystemType,
    pub node_count: usize,
    pub vote_count: usize,
    pub concurrent_votes: usize,
    pub warmup_duration: Duration,
    pub test_duration: Duration,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SystemType {
    P2pComm,
    MtlsComm,
}

impl SystemType {
    pub fn name(&self) -> &'static str {
        match self {
            SystemType::P2pComm => "p2p-comm (libp2p)",
            SystemType::MtlsComm => "mtls-comm (pure mTLS)",
        }
    }

    pub fn docker_compose_path(&self) -> &'static str {
        match self {
            SystemType::P2pComm => "../p2p-comm/docker-compose.yml",
            SystemType::MtlsComm => "../mtls-comm/docker-compose.yml",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub system: SystemType,
    pub metrics: PerformanceMetrics,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    // Throughput
    pub votes_per_second: f64,
    pub messages_per_second: f64,
    pub bytes_per_second: f64,

    // Latency (microseconds)
    pub latency_p50: u64,
    pub latency_p95: u64,
    pub latency_p99: u64,
    pub latency_max: u64,
    pub latency_mean: f64,
    pub latency_stddev: f64,

    // Consensus
    pub vote_processing_time_us: f64,
    pub byzantine_check_time_us: f64,
    pub etcd_write_time_us: f64,
    pub state_transition_time_us: f64,

    // Resource usage
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: f64,
    pub network_bytes_sent: u64,
    pub network_bytes_received: u64,

    // Reliability
    pub total_votes_sent: u64,
    pub total_votes_received: u64,
    pub delivery_success_rate: f64,
    pub error_count: u64,
    pub timeout_count: u64,

    // Security
    pub tls_handshake_time_us: f64,
    pub cert_validation_time_us: f64,
    pub encryption_overhead_percent: f64,

    // Scalability
    pub node_count: usize,
    pub max_connections: usize,
    pub connection_establishment_time_us: f64,
}

impl PerformanceMetrics {
    pub fn new() -> Self {
        Self {
            votes_per_second: 0.0,
            messages_per_second: 0.0,
            bytes_per_second: 0.0,
            latency_p50: 0,
            latency_p95: 0,
            latency_p99: 0,
            latency_max: 0,
            latency_mean: 0.0,
            latency_stddev: 0.0,
            vote_processing_time_us: 0.0,
            byzantine_check_time_us: 0.0,
            etcd_write_time_us: 0.0,
            state_transition_time_us: 0.0,
            cpu_usage_percent: 0.0,
            memory_usage_mb: 0.0,
            network_bytes_sent: 0,
            network_bytes_received: 0,
            total_votes_sent: 0,
            total_votes_received: 0,
            delivery_success_rate: 0.0,
            error_count: 0,
            timeout_count: 0,
            tls_handshake_time_us: 0.0,
            cert_validation_time_us: 0.0,
            encryption_overhead_percent: 0.0,
            node_count: 0,
            max_connections: 0,
            connection_establishment_time_us: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonReport {
    pub mtls_sharedmem: BenchmarkResult,
    pub mtls_with_mtls: BenchmarkResult,
    pub winner: Winner,
    pub improvements: Vec<Improvement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Winner {
    pub throughput: SystemType,
    pub latency: SystemType,
    pub resource_efficiency: SystemType,
    pub reliability: SystemType,
    pub overall: SystemType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Improvement {
    pub metric: String,
    pub percentage: f64,
    pub winner: SystemType,
}

pub mod docker {
    use super::*;
    use std::process::Command;

    pub fn start_system(system: SystemType) -> anyhow::Result<()> {
        let path = system.docker_compose_path();
        let output = Command::new("docker-compose")
            .args(&["-f", path, "up", "-d"])
            .output()?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to start {}: {}",
                system.name(),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }

    pub fn stop_system(system: SystemType) -> anyhow::Result<()> {
        let path = system.docker_compose_path();
        let output = Command::new("docker-compose")
            .args(&["-f", path, "down"])
            .output()?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to stop {}: {}",
                system.name(),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }

    pub fn get_container_stats(system: SystemType, node_id: usize) -> anyhow::Result<ContainerStats> {
        let container_name = match system {
            SystemType::P2pComm => format!("p2p-comm-node-{}", node_id),
            SystemType::MtlsComm => format!("mtls-node-{}", node_id),
        };

        let output = Command::new("docker")
            .args(&["stats", "--no-stream", "--format", "{{json .}}", &container_name])
            .output()?;

        if !output.status.success() {
            anyhow::bail!("Failed to get stats for {}", container_name);
        }

        let stats: ContainerStats = serde_json::from_slice(&output.stdout)?;
        Ok(stats)
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct ContainerStats {
        #[serde(rename = "CPUPerc")]
        pub cpu_perc: String,
        #[serde(rename = "MemUsage")]
        pub mem_usage: String,
        #[serde(rename = "NetIO")]
        pub net_io: String,
    }
}

pub mod metrics {
    use std::time::Instant;

    pub struct LatencyRecorder {
        samples: Vec<u64>,
    }

    impl LatencyRecorder {
        pub fn new() -> Self {
            Self {
                samples: Vec::new(),
            }
        }

        pub fn record(&mut self, duration_us: u64) {
            self.samples.push(duration_us);
        }

        pub fn percentile(&self, p: f64) -> u64 {
            if self.samples.is_empty() {
                return 0;
            }

            let mut sorted = self.samples.clone();
            sorted.sort_unstable();

            let index = ((p / 100.0) * sorted.len() as f64) as usize;
            sorted[index.min(sorted.len() - 1)]
        }

        pub fn mean(&self) -> f64 {
            if self.samples.is_empty() {
                return 0.0;
            }

            let sum: u64 = self.samples.iter().sum();
            sum as f64 / self.samples.len() as f64
        }

        pub fn stddev(&self) -> f64 {
            if self.samples.is_empty() {
                return 0.0;
            }

            let mean = self.mean();
            let variance: f64 = self
                .samples
                .iter()
                .map(|&x| {
                    let diff = x as f64 - mean;
                    diff * diff
                })
                .sum::<f64>()
                / self.samples.len() as f64;

            variance.sqrt()
        }

        pub fn max(&self) -> u64 {
            self.samples.iter().copied().max().unwrap_or(0)
        }
    }

    pub struct Timer {
        start: Instant,
    }

    impl Timer {
        pub fn new() -> Self {
            Self {
                start: Instant::now(),
            }
        }

        pub fn elapsed_us(&self) -> u64 {
            self.start.elapsed().as_micros() as u64
        }
    }
}
