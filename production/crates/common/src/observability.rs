//! Observability Infrastructure for MPC Wallet
//!
//! This module provides unified observability primitives for both coordinator
//! and nodes, including:
//!
//! - **Structured Tracing**: Request-scoped spans with correlation IDs
//! - **Metrics**: Counters, gauges, and histograms for protocol performance
//! - **Event Types**: Standardized event types for log aggregation
//!
//! ## Node-First Philosophy
//!
//! The observability design follows a "node-first" principle: nodes are the
//! authoritative source of truth for protocol execution metrics. The coordinator
//! aggregates and exposes these metrics but does not generate them.
//!
//! ## Usage
//!
//! ```ignore
//! use common::observability::{ProtocolMetrics, SessionSpan, EventType};
//!
//! // Create metrics collector
//! let metrics = ProtocolMetrics::new("node-0");
//!
//! // Track a signing session
//! let span = SessionSpan::new("grant-abc123", "cggmp24-signing");
//! span.record_round_start(1);
//! metrics.record_round_duration("cggmp24", 1, duration);
//! span.record_round_complete(1);
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Standardized event types for structured logging.
///
/// These event types enable consistent log aggregation and filtering
/// across all MPC wallet components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    // Session lifecycle
    SessionCreated,
    SessionStarted,
    SessionCompleted,
    SessionFailed,
    SessionTimeout,

    // Protocol rounds
    RoundStarted,
    RoundCompleted,
    RoundTimeout,
    MessageSent,
    MessageReceived,

    // Grant system
    GrantReceived,
    GrantValidated,
    GrantRejected,
    GrantExpired,

    // Node health
    NodeHealthy,
    NodeUnhealthy,
    NodeConnected,
    NodeDisconnected,

    // Transport
    TransportConnected,
    TransportDisconnected,
    TransportError,

    // Errors
    ProtocolError,
    ValidationError,
    TimeoutError,
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::SessionCreated => "session_created",
            Self::SessionStarted => "session_started",
            Self::SessionCompleted => "session_completed",
            Self::SessionFailed => "session_failed",
            Self::SessionTimeout => "session_timeout",
            Self::RoundStarted => "round_started",
            Self::RoundCompleted => "round_completed",
            Self::RoundTimeout => "round_timeout",
            Self::MessageSent => "message_sent",
            Self::MessageReceived => "message_received",
            Self::GrantReceived => "grant_received",
            Self::GrantValidated => "grant_validated",
            Self::GrantRejected => "grant_rejected",
            Self::GrantExpired => "grant_expired",
            Self::NodeHealthy => "node_healthy",
            Self::NodeUnhealthy => "node_unhealthy",
            Self::NodeConnected => "node_connected",
            Self::NodeDisconnected => "node_disconnected",
            Self::TransportConnected => "transport_connected",
            Self::TransportDisconnected => "transport_disconnected",
            Self::TransportError => "transport_error",
            Self::ProtocolError => "protocol_error",
            Self::ValidationError => "validation_error",
            Self::TimeoutError => "timeout_error",
        };
        write!(f, "{}", s)
    }
}

/// A structured log event with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    /// Event type for filtering.
    pub event_type: EventType,
    /// Unix timestamp in milliseconds.
    pub timestamp_ms: u64,
    /// Correlation ID for tracing (e.g., session_id).
    pub correlation_id: Option<String>,
    /// Party index that generated this event.
    pub party_index: Option<u16>,
    /// Protocol name (cggmp24, frost).
    pub protocol: Option<String>,
    /// Round number if applicable.
    pub round: Option<u16>,
    /// Duration in milliseconds if applicable.
    pub duration_ms: Option<u64>,
    /// Additional context as key-value pairs.
    #[serde(default)]
    pub context: HashMap<String, String>,
    /// Error message if this is an error event.
    pub error: Option<String>,
}

impl LogEvent {
    /// Create a new log event.
    pub fn new(event_type: EventType) -> Self {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Self {
            event_type,
            timestamp_ms,
            correlation_id: None,
            party_index: None,
            protocol: None,
            round: None,
            duration_ms: None,
            context: HashMap::new(),
            error: None,
        }
    }

    /// Set correlation ID.
    pub fn with_correlation_id(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }

    /// Set party index.
    pub fn with_party(mut self, party_index: u16) -> Self {
        self.party_index = Some(party_index);
        self
    }

    /// Set protocol name.
    pub fn with_protocol(mut self, protocol: impl Into<String>) -> Self {
        self.protocol = Some(protocol.into());
        self
    }

    /// Set round number.
    pub fn with_round(mut self, round: u16) -> Self {
        self.round = Some(round);
        self
    }

    /// Set duration.
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration_ms = Some(duration.as_millis() as u64);
        self
    }

    /// Add context key-value pair.
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }

    /// Set error message.
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    /// Emit this event using tracing.
    pub fn emit(&self) {
        // Use tracing macros based on event type
        let json = serde_json::to_string(&self).unwrap_or_default();

        match self.event_type {
            EventType::SessionFailed
            | EventType::SessionTimeout
            | EventType::RoundTimeout
            | EventType::GrantRejected
            | EventType::GrantExpired
            | EventType::NodeUnhealthy
            | EventType::NodeDisconnected
            | EventType::TransportDisconnected
            | EventType::TransportError
            | EventType::ProtocolError
            | EventType::ValidationError
            | EventType::TimeoutError => {
                tracing::warn!(event = %json, "observability_event");
            }
            _ => {
                tracing::info!(event = %json, "observability_event");
            }
        }
    }
}

/// Metrics collector for protocol performance monitoring.
///
/// Thread-safe metrics collection with atomic counters.
#[derive(Debug)]
pub struct ProtocolMetrics {
    /// Node identifier for this metrics instance.
    pub node_id: String,

    // Session metrics
    sessions_started: AtomicU64,
    sessions_completed: AtomicU64,
    sessions_failed: AtomicU64,

    // Round metrics
    rounds_completed: AtomicU64,
    rounds_timeout: AtomicU64,

    // Message metrics
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
    messages_bytes_sent: AtomicU64,
    messages_bytes_received: AtomicU64,

    // Grant metrics
    grants_validated: AtomicU64,
    grants_rejected: AtomicU64,

    // Timing histograms (stored as buckets)
    round_durations: Arc<RwLock<Vec<DurationBucket>>>,
    session_durations: Arc<RwLock<Vec<DurationBucket>>>,
}

/// A bucket for duration histogram.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct DurationBucket {
    protocol: String,
    round: Option<u16>,
    duration_ms: u64,
    timestamp_ms: u64,
}

impl ProtocolMetrics {
    /// Create a new metrics collector.
    pub fn new(node_id: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            sessions_started: AtomicU64::new(0),
            sessions_completed: AtomicU64::new(0),
            sessions_failed: AtomicU64::new(0),
            rounds_completed: AtomicU64::new(0),
            rounds_timeout: AtomicU64::new(0),
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
            messages_bytes_sent: AtomicU64::new(0),
            messages_bytes_received: AtomicU64::new(0),
            grants_validated: AtomicU64::new(0),
            grants_rejected: AtomicU64::new(0),
            round_durations: Arc::new(RwLock::new(Vec::new())),
            session_durations: Arc::new(RwLock::new(Vec::new())),
        }
    }

    // Counter increments

    pub fn inc_sessions_started(&self) {
        self.sessions_started.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_sessions_completed(&self) {
        self.sessions_completed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_sessions_failed(&self) {
        self.sessions_failed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_rounds_completed(&self) {
        self.rounds_completed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_rounds_timeout(&self) {
        self.rounds_timeout.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_messages_sent(&self, bytes: u64) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
        self.messages_bytes_sent.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn inc_messages_received(&self, bytes: u64) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
        self.messages_bytes_received
            .fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn inc_grants_validated(&self) {
        self.grants_validated.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_grants_rejected(&self) {
        self.grants_rejected.fetch_add(1, Ordering::Relaxed);
    }

    // Duration recording

    /// Record a round duration.
    pub fn record_round_duration(&self, protocol: &str, round: u16, duration: Duration) {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let bucket = DurationBucket {
            protocol: protocol.to_string(),
            round: Some(round),
            duration_ms: duration.as_millis() as u64,
            timestamp_ms,
        };

        if let Ok(mut durations) = self.round_durations.write() {
            durations.push(bucket);
            // Keep only last 1000 entries
            if durations.len() > 1000 {
                durations.remove(0);
            }
        }
    }

    /// Record a session duration.
    pub fn record_session_duration(&self, protocol: &str, duration: Duration) {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let bucket = DurationBucket {
            protocol: protocol.to_string(),
            round: None,
            duration_ms: duration.as_millis() as u64,
            timestamp_ms,
        };

        if let Ok(mut durations) = self.session_durations.write() {
            durations.push(bucket);
            // Keep only last 1000 entries
            if durations.len() > 1000 {
                durations.remove(0);
            }
        }
    }

    /// Get current metrics snapshot.
    pub fn snapshot(&self) -> MetricsSnapshot {
        let round_durations = self
            .round_durations
            .read()
            .map(|d| d.clone())
            .unwrap_or_default();
        let session_durations = self
            .session_durations
            .read()
            .map(|d| d.clone())
            .unwrap_or_default();

        // Calculate average round duration per protocol
        let mut round_avg: HashMap<String, (u64, u64)> = HashMap::new(); // (sum, count)
        for bucket in &round_durations {
            let entry = round_avg.entry(bucket.protocol.clone()).or_insert((0, 0));
            entry.0 += bucket.duration_ms;
            entry.1 += 1;
        }

        let round_duration_avg: HashMap<String, f64> = round_avg
            .into_iter()
            .map(|(k, (sum, count))| (k, sum as f64 / count as f64))
            .collect();

        // Calculate average session duration per protocol
        let mut session_avg: HashMap<String, (u64, u64)> = HashMap::new();
        for bucket in &session_durations {
            let entry = session_avg.entry(bucket.protocol.clone()).or_insert((0, 0));
            entry.0 += bucket.duration_ms;
            entry.1 += 1;
        }

        let session_duration_avg: HashMap<String, f64> = session_avg
            .into_iter()
            .map(|(k, (sum, count))| (k, sum as f64 / count as f64))
            .collect();

        MetricsSnapshot {
            node_id: self.node_id.clone(),
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            sessions_started: self.sessions_started.load(Ordering::Relaxed),
            sessions_completed: self.sessions_completed.load(Ordering::Relaxed),
            sessions_failed: self.sessions_failed.load(Ordering::Relaxed),
            rounds_completed: self.rounds_completed.load(Ordering::Relaxed),
            rounds_timeout: self.rounds_timeout.load(Ordering::Relaxed),
            messages_sent: self.messages_sent.load(Ordering::Relaxed),
            messages_received: self.messages_received.load(Ordering::Relaxed),
            messages_bytes_sent: self.messages_bytes_sent.load(Ordering::Relaxed),
            messages_bytes_received: self.messages_bytes_received.load(Ordering::Relaxed),
            grants_validated: self.grants_validated.load(Ordering::Relaxed),
            grants_rejected: self.grants_rejected.load(Ordering::Relaxed),
            round_duration_avg_ms: round_duration_avg,
            session_duration_avg_ms: session_duration_avg,
        }
    }
}

/// A snapshot of current metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub node_id: String,
    pub timestamp_ms: u64,

    // Counters
    pub sessions_started: u64,
    pub sessions_completed: u64,
    pub sessions_failed: u64,
    pub rounds_completed: u64,
    pub rounds_timeout: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub messages_bytes_sent: u64,
    pub messages_bytes_received: u64,
    pub grants_validated: u64,
    pub grants_rejected: u64,

    // Computed averages (protocol -> avg ms)
    pub round_duration_avg_ms: HashMap<String, f64>,
    pub session_duration_avg_ms: HashMap<String, f64>,
}

/// A session-scoped span for tracking protocol execution.
///
/// Use this to wrap protocol operations for structured tracing.
pub struct SessionSpan {
    session_id: String,
    protocol: String,
    party_index: Option<u16>,
    start_time: Instant,
    current_round: u16,
    round_start: Option<Instant>,
}

impl SessionSpan {
    /// Create a new session span.
    pub fn new(session_id: impl Into<String>, protocol: impl Into<String>) -> Self {
        let span = Self {
            session_id: session_id.into(),
            protocol: protocol.into(),
            party_index: None,
            start_time: Instant::now(),
            current_round: 0,
            round_start: None,
        };

        LogEvent::new(EventType::SessionStarted)
            .with_correlation_id(&span.session_id)
            .with_protocol(&span.protocol)
            .emit();

        span
    }

    /// Set the party index.
    pub fn with_party(mut self, party_index: u16) -> Self {
        self.party_index = Some(party_index);
        self
    }

    /// Record start of a round.
    pub fn record_round_start(&mut self, round: u16) {
        self.current_round = round;
        self.round_start = Some(Instant::now());

        let mut event = LogEvent::new(EventType::RoundStarted)
            .with_correlation_id(&self.session_id)
            .with_protocol(&self.protocol)
            .with_round(round);

        if let Some(party) = self.party_index {
            event = event.with_party(party);
        }

        event.emit();
    }

    /// Record completion of a round.
    pub fn record_round_complete(&mut self, round: u16) -> Duration {
        let duration = self
            .round_start
            .map(|s| s.elapsed())
            .unwrap_or(Duration::ZERO);

        let mut event = LogEvent::new(EventType::RoundCompleted)
            .with_correlation_id(&self.session_id)
            .with_protocol(&self.protocol)
            .with_round(round)
            .with_duration(duration);

        if let Some(party) = self.party_index {
            event = event.with_party(party);
        }

        event.emit();

        self.round_start = None;
        duration
    }

    /// Record a message sent.
    pub fn record_message_sent(&self, to_party: Option<u16>, bytes: usize) {
        let mut event = LogEvent::new(EventType::MessageSent)
            .with_correlation_id(&self.session_id)
            .with_protocol(&self.protocol)
            .with_round(self.current_round)
            .with_context("bytes", bytes.to_string());

        if let Some(party) = self.party_index {
            event = event.with_party(party);
        }

        if let Some(to) = to_party {
            event = event.with_context("to_party", to.to_string());
        } else {
            event = event.with_context("to_party", "broadcast".to_string());
        }

        event.emit();
    }

    /// Record a message received.
    pub fn record_message_received(&self, from_party: u16, bytes: usize) {
        let mut event = LogEvent::new(EventType::MessageReceived)
            .with_correlation_id(&self.session_id)
            .with_protocol(&self.protocol)
            .with_round(self.current_round)
            .with_context("from_party", from_party.to_string())
            .with_context("bytes", bytes.to_string());

        if let Some(party) = self.party_index {
            event = event.with_party(party);
        }

        event.emit();
    }

    /// Get session duration so far.
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Complete the session successfully.
    pub fn complete(self) -> Duration {
        let duration = self.start_time.elapsed();

        let mut event = LogEvent::new(EventType::SessionCompleted)
            .with_correlation_id(&self.session_id)
            .with_protocol(&self.protocol)
            .with_duration(duration);

        if let Some(party) = self.party_index {
            event = event.with_party(party);
        }

        event.emit();

        duration
    }

    /// Fail the session with an error.
    pub fn fail(self, error: impl Into<String>) -> Duration {
        let duration = self.start_time.elapsed();

        let mut event = LogEvent::new(EventType::SessionFailed)
            .with_correlation_id(&self.session_id)
            .with_protocol(&self.protocol)
            .with_duration(duration)
            .with_error(error);

        if let Some(party) = self.party_index {
            event = event.with_party(party);
        }

        event.emit();

        duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_event_builder() {
        let event = LogEvent::new(EventType::SessionStarted)
            .with_correlation_id("session-123")
            .with_party(0)
            .with_protocol("cggmp24")
            .with_round(1)
            .with_context("wallet_id", "test-wallet");

        assert_eq!(event.event_type, EventType::SessionStarted);
        assert_eq!(event.correlation_id, Some("session-123".to_string()));
        assert_eq!(event.party_index, Some(0));
        assert_eq!(event.protocol, Some("cggmp24".to_string()));
        assert_eq!(event.round, Some(1));
        assert_eq!(
            event.context.get("wallet_id"),
            Some(&"test-wallet".to_string())
        );
    }

    #[test]
    fn test_metrics_counters() {
        let metrics = ProtocolMetrics::new("test-node");

        metrics.inc_sessions_started();
        metrics.inc_sessions_started();
        metrics.inc_sessions_completed();
        metrics.inc_messages_sent(100);
        metrics.inc_messages_received(200);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.sessions_started, 2);
        assert_eq!(snapshot.sessions_completed, 1);
        assert_eq!(snapshot.messages_sent, 1);
        assert_eq!(snapshot.messages_received, 1);
        assert_eq!(snapshot.messages_bytes_sent, 100);
        assert_eq!(snapshot.messages_bytes_received, 200);
    }

    #[test]
    fn test_metrics_duration_recording() {
        let metrics = ProtocolMetrics::new("test-node");

        metrics.record_round_duration("cggmp24", 1, Duration::from_millis(100));
        metrics.record_round_duration("cggmp24", 2, Duration::from_millis(200));
        metrics.record_round_duration("frost", 1, Duration::from_millis(50));

        let snapshot = metrics.snapshot();

        // CGGMP24 average should be (100 + 200) / 2 = 150
        assert!(
            (snapshot.round_duration_avg_ms.get("cggmp24").unwrap() - 150.0).abs() < 0.01,
            "Expected 150, got {}",
            snapshot.round_duration_avg_ms.get("cggmp24").unwrap()
        );

        // FROST average should be 50
        assert!(
            (snapshot.round_duration_avg_ms.get("frost").unwrap() - 50.0).abs() < 0.01,
            "Expected 50, got {}",
            snapshot.round_duration_avg_ms.get("frost").unwrap()
        );
    }

    #[test]
    fn test_event_type_display() {
        assert_eq!(EventType::SessionStarted.to_string(), "session_started");
        assert_eq!(EventType::RoundCompleted.to_string(), "round_completed");
        assert_eq!(EventType::GrantValidated.to_string(), "grant_validated");
    }

    #[test]
    fn test_metrics_snapshot_serialization() {
        let metrics = ProtocolMetrics::new("test-node");
        metrics.inc_sessions_started();

        let snapshot = metrics.snapshot();
        let json = serde_json::to_string(&snapshot).unwrap();

        // Should be valid JSON
        let parsed: MetricsSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.node_id, "test-node");
        assert_eq!(parsed.sessions_started, 1);
    }
}
