//! Node discovery and health tracking types for P2P migration.
//!
//! This module provides the shared types used for node registration,
//! heartbeat protocol, and health scoring between coordinator and nodes.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Supported MPC protocols for capability advertisement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolCapability {
    /// CGGMP24 for ECDSA (Native SegWit)
    Cggmp24,
    /// FROST for Schnorr (Taproot)
    Frost,
    /// Auxiliary info generation
    AuxInfo,
}

/// Capabilities advertised by a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapabilities {
    /// Supported protocols
    pub protocols: Vec<ProtocolCapability>,
    /// Whether primes are available (for CGGMP24)
    pub has_primes: bool,
    /// Whether aux_info is available (for CGGMP24)
    pub has_aux_info: bool,
    /// Number of CGGMP24 key shares held
    pub cggmp24_keyshare_count: usize,
    /// Number of FROST key shares held
    pub frost_keyshare_count: usize,
    /// Wallet IDs for which this node holds key shares
    pub wallet_ids: Vec<String>,
}

impl Default for NodeCapabilities {
    fn default() -> Self {
        Self {
            protocols: vec![ProtocolCapability::Cggmp24, ProtocolCapability::Frost],
            has_primes: false,
            has_aux_info: false,
            cggmp24_keyshare_count: 0,
            frost_keyshare_count: 0,
            wallet_ids: Vec::new(),
        }
    }
}

/// Health metrics for a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHealthMetrics {
    /// Average response latency in milliseconds (exponential moving average)
    pub avg_latency_ms: f64,
    /// Success rate of heartbeats (0.0 - 1.0)
    pub reliability: f64,
    /// Uptime in seconds since first registration
    pub uptime_secs: u64,
    /// Number of successful heartbeats
    pub successful_heartbeats: u64,
    /// Number of failed heartbeats
    pub failed_heartbeats: u64,
    /// Last measured latency in milliseconds
    pub last_latency_ms: u64,
}

impl Default for NodeHealthMetrics {
    fn default() -> Self {
        Self {
            avg_latency_ms: 0.0,
            reliability: 1.0,
            uptime_secs: 0,
            successful_heartbeats: 0,
            failed_heartbeats: 0,
            last_latency_ms: 0,
        }
    }
}

impl NodeHealthMetrics {
    /// Calculate health score (0.0 - 1.0).
    ///
    /// Weights:
    /// - 40% reliability (successful/total heartbeats)
    /// - 30% latency (linear decay from 1.0 at 0ms to 0.0 at 1000ms)
    /// - 30% uptime (starts at 0.5, grows to 1.0 after 1 hour)
    pub fn health_score(&self) -> f64 {
        // Latency score: 0ms = 1.0, 1000ms = 0.0 (linear decay)
        let latency_score = (1.0 - (self.avg_latency_ms / 1000.0).min(1.0)).max(0.0);

        // Uptime score: 0 = 0.5, 1 hour+ = 1.0 (linear growth)
        let uptime_score = (0.5 + 0.5 * (self.uptime_secs as f64 / 3600.0).min(1.0)).min(1.0);

        // Weighted average
        0.4 * self.reliability + 0.3 * latency_score + 0.3 * uptime_score
    }

    /// Update metrics with a new heartbeat result.
    pub fn record_heartbeat(&mut self, success: bool, latency_ms: Option<u64>) {
        if success {
            self.successful_heartbeats += 1;
            if let Some(latency) = latency_ms {
                self.last_latency_ms = latency;
                // Exponential moving average with alpha = 0.2
                if self.successful_heartbeats == 1 {
                    // First heartbeat - use actual value
                    self.avg_latency_ms = latency as f64;
                } else {
                    self.avg_latency_ms = 0.8 * self.avg_latency_ms + 0.2 * latency as f64;
                }
            }
        } else {
            self.failed_heartbeats += 1;
        }

        // Update reliability
        let total = self.successful_heartbeats + self.failed_heartbeats;
        if total > 0 {
            self.reliability = self.successful_heartbeats as f64 / total as f64;
        }
    }
}

/// Full node information stored in the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Unique node identifier
    pub node_id: Uuid,
    /// Party index (0-based)
    pub party_index: u16,
    /// Node's advertised endpoint URL
    pub endpoint: String,
    /// Node capabilities
    pub capabilities: NodeCapabilities,
    /// Health metrics
    pub health: NodeHealthMetrics,
    /// Unix timestamp of first registration
    pub registered_at: u64,
    /// Unix timestamp of last successful heartbeat
    pub last_heartbeat: u64,
    /// Whether the node is currently considered online
    pub is_online: bool,
}

impl NodeInfo {
    /// Create a new NodeInfo for initial registration.
    pub fn new(node_id: Uuid, party_index: u16, endpoint: String) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            node_id,
            party_index,
            endpoint,
            capabilities: NodeCapabilities::default(),
            health: NodeHealthMetrics::default(),
            registered_at: now,
            last_heartbeat: now,
            is_online: true,
        }
    }

    /// Check if the node has missed too many heartbeats.
    pub fn is_stale(&self, heartbeat_interval_secs: u64, max_missed: u64) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        now.saturating_sub(self.last_heartbeat) > heartbeat_interval_secs * max_missed
    }

    /// Update uptime based on registration time.
    pub fn update_uptime(&mut self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.health.uptime_secs = now.saturating_sub(self.registered_at);
    }
}

// ============================================================================
// API Request/Response Types
// ============================================================================

/// Request to register a node with the coordinator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterNodeRequest {
    /// Pre-shared key for authentication.
    /// Must match the coordinator's configured PSK to prevent unauthorized registration.
    pub psk: String,
    pub node_id: Uuid,
    pub party_index: u16,
    pub endpoint: String,
    pub capabilities: NodeCapabilities,
}

/// Response from node registration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterNodeResponse {
    pub success: bool,
    pub message: String,
    /// Recommended heartbeat interval in seconds
    pub heartbeat_interval_secs: u64,
    /// Secret token for certificate issuance (only returned once, store securely).
    /// This token is required when requesting TLS certificates from the coordinator.
    /// It is NOT exposed in any public API and should be treated as a secret.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cert_token: Option<String>,
}

/// Heartbeat request from a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    /// Secret token received during registration (for authentication)
    pub cert_token: String,
    pub node_id: Uuid,
    pub party_index: u16,
    /// Updated capabilities (may change if key shares added)
    pub capabilities: NodeCapabilities,
}

/// Heartbeat response to a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatResponse {
    pub success: bool,
    pub message: String,
    /// Current health score as seen by coordinator
    pub health_score: f64,
    /// Number of registered nodes
    pub registered_nodes: usize,
}

/// Response listing all registered nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListNodesResponse {
    pub nodes: Vec<NodeInfo>,
    pub online_count: usize,
    pub total_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_score_perfect() {
        // Simulate perfect node: 100% reliability, 0ms latency, 1 hour uptime
        let metrics = NodeHealthMetrics {
            reliability: 1.0,
            avg_latency_ms: 0.0,
            uptime_secs: 3600,
            ..Default::default()
        };

        let score = metrics.health_score();
        assert!(
            (score - 1.0).abs() < 0.01,
            "Perfect node should have score ~1.0"
        );
    }

    #[test]
    fn test_health_score_new_node() {
        let metrics = NodeHealthMetrics::default();
        // New node: 100% reliability (default), 0ms latency, 0 uptime
        let score = metrics.health_score();
        // 0.4 * 1.0 + 0.3 * 1.0 + 0.3 * 0.5 = 0.85
        assert!(
            (score - 0.85).abs() < 0.01,
            "New node should have score ~0.85, got {}",
            score
        );
    }

    #[test]
    fn test_health_score_degraded() {
        // Degraded node: 50% reliability, 500ms latency, 30min uptime
        let metrics = NodeHealthMetrics {
            reliability: 0.5,
            avg_latency_ms: 500.0,
            uptime_secs: 1800,
            ..Default::default()
        };

        let score = metrics.health_score();
        // 0.4 * 0.5 + 0.3 * 0.5 + 0.3 * 0.75 = 0.575
        assert!(
            (score - 0.575).abs() < 0.01,
            "Degraded node should have score ~0.575, got {}",
            score
        );
    }

    #[test]
    fn test_record_heartbeat_success() {
        let mut metrics = NodeHealthMetrics::default();

        metrics.record_heartbeat(true, Some(100));
        assert_eq!(metrics.successful_heartbeats, 1);
        assert_eq!(metrics.failed_heartbeats, 0);
        assert_eq!(metrics.reliability, 1.0);
        assert_eq!(metrics.avg_latency_ms, 100.0);

        metrics.record_heartbeat(true, Some(200));
        assert_eq!(metrics.successful_heartbeats, 2);
        // EMA: 0.8 * 100 + 0.2 * 200 = 120
        assert!((metrics.avg_latency_ms - 120.0).abs() < 0.01);
    }

    #[test]
    fn test_record_heartbeat_failure() {
        let mut metrics = NodeHealthMetrics::default();

        metrics.record_heartbeat(true, Some(100));
        metrics.record_heartbeat(false, None);

        assert_eq!(metrics.successful_heartbeats, 1);
        assert_eq!(metrics.failed_heartbeats, 1);
        assert_eq!(metrics.reliability, 0.5);
    }

    #[test]
    fn test_node_info_stale_detection() {
        let mut node = NodeInfo::new(Uuid::new_v4(), 0, "http://localhost:3000".to_string());

        // Fresh node should not be stale
        assert!(!node.is_stale(10, 3));

        // Simulate old heartbeat (40 seconds ago, with 10s interval and 3 max missed)
        node.last_heartbeat = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 40;

        assert!(node.is_stale(10, 3)); // 40s > 10s * 3 = 30s
    }
}
