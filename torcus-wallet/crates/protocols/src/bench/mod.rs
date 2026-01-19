//! Benchmarking utilities for MPC protocols.
//!
//! This module provides timing instrumentation for measuring the performance
//! of various protocol steps in CGGMP24 and FROST signing.
//!
//! ## Running Benchmarks
//!
//! Run FROST benchmarks (fast):
//! ```bash
//! cargo test --package protocols benchmark_frost --release -- --nocapture
//! ```
//!
//! Run CGGMP24 benchmarks (first run is slow, subsequent runs use cached primes):
//! ```bash
//! cargo test --package protocols benchmark_cggmp24 --release -- --nocapture
//! ```

pub mod protocol_benchmarks;
pub mod simulation;

pub use protocol_benchmarks::{
    run_cggmp24_benchmark, run_cggmp24_benchmark_with_cache, run_frost_benchmark,
    ProtocolBenchmarkResult,
};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A timer that tracks the duration of a specific step.
#[derive(Debug)]
pub struct StepTimer {
    name: String,
    start: Instant,
    recorder: Arc<Mutex<BenchmarkRecorder>>,
}

impl StepTimer {
    /// Create a new step timer.
    pub fn new(name: impl Into<String>, recorder: Arc<Mutex<BenchmarkRecorder>>) -> Self {
        Self {
            name: name.into(),
            start: Instant::now(),
            recorder,
        }
    }

    /// Stop the timer and record the duration.
    pub fn stop(self) -> Duration {
        let elapsed = self.start.elapsed();
        if let Ok(mut rec) = self.recorder.lock() {
            rec.record_step(&self.name, elapsed);
        }
        elapsed
    }
}

impl Drop for StepTimer {
    fn drop(&mut self) {
        // If stop() wasn't called explicitly, record on drop
        let elapsed = self.start.elapsed();
        if let Ok(mut rec) = self.recorder.lock() {
            if !rec.steps.contains_key(&self.name) {
                rec.record_step(&self.name, elapsed);
            }
        }
    }
}

/// Records benchmark timings for protocol steps.
#[derive(Debug, Clone, Default)]
pub struct BenchmarkRecorder {
    /// Name of the protocol being benchmarked
    pub protocol: String,
    /// Party index
    pub party_index: u16,
    /// Session ID
    pub session_id: String,
    /// Individual step timings
    pub steps: HashMap<String, StepTiming>,
    /// Total protocol duration
    pub total_duration: Option<Duration>,
    /// Start time of the entire protocol
    start_time: Option<Instant>,
}

/// Timing information for a single step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepTiming {
    /// Name of the step
    pub name: String,
    /// Duration in milliseconds
    pub duration_ms: f64,
    /// Order in which this step was recorded
    pub order: usize,
}

impl BenchmarkRecorder {
    /// Create a new recorder for a protocol.
    pub fn new(
        protocol: impl Into<String>,
        party_index: u16,
        session_id: impl Into<String>,
    ) -> Self {
        Self {
            protocol: protocol.into(),
            party_index,
            session_id: session_id.into(),
            steps: HashMap::new(),
            total_duration: None,
            start_time: Some(Instant::now()),
        }
    }

    /// Record a step timing.
    pub fn record_step(&mut self, name: &str, duration: Duration) {
        let order = self.steps.len();
        self.steps.insert(
            name.to_string(),
            StepTiming {
                name: name.to_string(),
                duration_ms: duration.as_secs_f64() * 1000.0,
                order,
            },
        );
    }

    /// Mark the protocol as complete and record total duration.
    pub fn complete(&mut self) {
        if let Some(start) = self.start_time {
            self.total_duration = Some(start.elapsed());
        }
    }

    /// Get the total duration in milliseconds.
    pub fn total_duration_ms(&self) -> f64 {
        self.total_duration
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    }

    /// Generate a report of all timings.
    pub fn report(&self) -> BenchmarkReport {
        let mut steps: Vec<StepTiming> = self.steps.values().cloned().collect();
        steps.sort_by_key(|s| s.order);

        BenchmarkReport {
            protocol: self.protocol.clone(),
            party_index: self.party_index,
            session_id: self.session_id.clone(),
            steps,
            total_duration_ms: self.total_duration_ms(),
        }
    }
}

/// A complete benchmark report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkReport {
    /// Name of the protocol
    pub protocol: String,
    /// Party index
    pub party_index: u16,
    /// Session ID
    pub session_id: String,
    /// All step timings in order
    pub steps: Vec<StepTiming>,
    /// Total duration in milliseconds
    pub total_duration_ms: f64,
}

impl BenchmarkReport {
    /// Print the report to the log.
    pub fn log(&self) {
        tracing::info!("========================================");
        tracing::info!("  BENCHMARK REPORT: {}", self.protocol);
        tracing::info!("========================================");
        tracing::info!("Party: {}", self.party_index);
        tracing::info!("Session: {}", self.session_id);
        tracing::info!("----------------------------------------");

        for step in &self.steps {
            tracing::info!("  {:40} {:>10.2} ms", step.name, step.duration_ms);
        }

        tracing::info!("----------------------------------------");
        tracing::info!("  {:40} {:>10.2} ms", "TOTAL", self.total_duration_ms);
        tracing::info!("========================================");
    }

    /// Format as a compact string for display.
    pub fn to_compact_string(&self) -> String {
        let mut s = format!(
            "{} (party {}) - Total: {:.2}ms\n",
            self.protocol, self.party_index, self.total_duration_ms
        );

        for step in &self.steps {
            let pct = if self.total_duration_ms > 0.0 {
                (step.duration_ms / self.total_duration_ms) * 100.0
            } else {
                0.0
            };
            s.push_str(&format!(
                "  {:40} {:>8.2}ms ({:>5.1}%)\n",
                step.name, step.duration_ms, pct
            ));
        }

        s
    }
}

/// Macro to time a block of code.
#[macro_export]
macro_rules! time_step {
    ($recorder:expr, $name:expr, $block:expr) => {{
        let start = std::time::Instant::now();
        let result = $block;
        let elapsed = start.elapsed();
        if let Ok(mut rec) = $recorder.lock() {
            rec.record_step($name, elapsed);
        }
        result
    }};
}

pub use time_step;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_benchmark_recorder() {
        let mut recorder = BenchmarkRecorder::new("test-protocol", 0, "test-session");

        recorder.record_step("step1", Duration::from_millis(100));
        recorder.record_step("step2", Duration::from_millis(200));
        recorder.record_step("step3", Duration::from_millis(50));

        recorder.complete();

        let report = recorder.report();
        assert_eq!(report.steps.len(), 3);
        assert_eq!(report.steps[0].name, "step1");
        assert_eq!(report.steps[1].name, "step2");
        assert_eq!(report.steps[2].name, "step3");
    }
}
