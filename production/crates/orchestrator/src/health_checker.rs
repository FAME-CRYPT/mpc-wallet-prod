//! Background health checker for MPC nodes
//!
//! This module spawns a background task that periodically:
//! 1. Pings all registered nodes to measure latency
//! 2. Updates health metrics based on responses
//! 3. Cleans up stale nodes that have missed heartbeats

use crate::error::{OrchestrationError, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};
use serde::{Deserialize, Serialize};

/// Interval between health check runs (seconds)
const HEALTH_CHECK_INTERVAL_SECS: u64 = 15;

/// Timeout for health check requests (seconds)
const HEALTH_CHECK_TIMEOUT_SECS: u64 = 5;

/// Maximum number of consecutive failures before marking node as failed
const MAX_CONSECUTIVE_FAILURES: u32 = 3;

/// Node health information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHealth {
    pub node_id: u64,
    pub endpoint: String,
    pub is_healthy: bool,
    pub latency_ms: Option<u64>,
    pub last_check: chrono::DateTime<chrono::Utc>,
    pub consecutive_failures: u32,
}

/// Background health checker for MPC nodes
pub struct HealthChecker {
    nodes: Vec<(u64, String)>, // (node_id, endpoint)
    health_state: Arc<RwLock<HashMap<u64, NodeHealth>>>,
    shutdown: Arc<RwLock<bool>>,
}

impl HealthChecker {
    /// Create a new health checker
    pub fn new(nodes: Vec<(u64, String)>) -> Self {
        Self {
            nodes,
            health_state: Arc::new(RwLock::new(HashMap::new())),
            shutdown: Arc::new(RwLock::new(false)),
        }
    }

    /// Get health status for all nodes
    pub async fn get_health_status(&self) -> HashMap<u64, NodeHealth> {
        self.health_state.read().await.clone()
    }

    /// Check if a specific node is healthy
    pub async fn is_node_healthy(&self, node_id: u64) -> bool {
        self.health_state
            .read().await
            .get(&node_id)
            .map(|h| h.is_healthy)
            .unwrap_or(false)
    }

    /// Start the health checker in the background
    pub fn start(self: Arc<Self>) -> JoinHandle<Result<()>> {
        info!(
            "Starting health checker for {} nodes (interval: {}s)",
            self.nodes.len(),
            HEALTH_CHECK_INTERVAL_SECS
        );

        tokio::spawn(async move {
            match self.run().await {
                Ok(()) => {
                    info!("Health checker stopped normally");
                    Ok(())
                }
                Err(e) => {
                    warn!("Health checker error: {}", e);
                    Err(e)
                }
            }
        })
    }

    /// Main health checking loop
    async fn run(&self) -> Result<()> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(HEALTH_CHECK_TIMEOUT_SECS))
            .build()
            .map_err(|e| OrchestrationError::Internal(format!("Failed to create HTTP client: {}", e)))?;

        let mut interval = tokio::time::interval(Duration::from_secs(HEALTH_CHECK_INTERVAL_SECS));

        loop {
            // Check shutdown signal
            if *self.shutdown.read().await {
                info!("Shutdown signal received, stopping health checker");
                return Ok(());
            }

            interval.tick().await;

            if self.nodes.is_empty() {
                debug!("No nodes configured, skipping health check");
                continue;
            }

            debug!("Running health check for {} nodes", self.nodes.len());

            // Check each node in parallel
            let check_futures: Vec<_> = self.nodes
                .iter()
                .map(|(node_id, endpoint)| {
                    let client = client.clone();
                    let endpoint = endpoint.clone();
                    let node_id = *node_id;
                    async move {
                        let start = Instant::now();
                        let health_url = format!("{}/health", endpoint);

                        match client.get(&health_url).send().await {
                            Ok(resp) if resp.status().is_success() => {
                                let latency_ms = start.elapsed().as_millis() as u64;
                                (node_id, true, Some(latency_ms))
                            }
                            Ok(resp) => {
                                warn!(
                                    "Node {} health check failed: HTTP {}",
                                    node_id,
                                    resp.status()
                                );
                                (node_id, false, None)
                            }
                            Err(e) => {
                                warn!("Node {} health check failed: {}", node_id, e);
                                (node_id, false, None)
                            }
                        }
                    }
                })
                .collect();

            let results = futures::future::join_all(check_futures).await;

            // Update health state with results
            let mut health_state = self.health_state.write().await;

            for (node_id, success, latency_ms) in results {
                let endpoint = self.nodes.iter()
                    .find(|(id, _)| *id == node_id)
                    .map(|(_, ep)| ep.clone())
                    .unwrap_or_default();

                // Get existing health record if any
                let mut consecutive_failures = health_state
                    .get(&node_id)
                    .map(|h| h.consecutive_failures)
                    .unwrap_or(0);

                // Update failure count
                if success {
                    consecutive_failures = 0;
                } else {
                    consecutive_failures += 1;
                }

                let health = NodeHealth {
                    node_id,
                    endpoint,
                    is_healthy: success && consecutive_failures < MAX_CONSECUTIVE_FAILURES,
                    latency_ms,
                    last_check: chrono::Utc::now(),
                    consecutive_failures,
                };

                health_state.insert(node_id, health.clone());

                if !health.is_healthy {
                    warn!(
                        "Node {} marked unhealthy after {} consecutive failures",
                        node_id, consecutive_failures
                    );
                } else if let Some(latency) = latency_ms {
                    debug!("Node {} healthy (latency: {}ms)", node_id, latency);
                }
            }

            // Clean up stale nodes (no health check for >5 minutes)
            let now = chrono::Utc::now();
            let stale_threshold = chrono::Duration::minutes(5);
            let stale_nodes: Vec<u64> = health_state
                .iter()
                .filter(|(_, h)| {
                    let age = now.signed_duration_since(h.last_check);
                    age > stale_threshold
                })
                .map(|(id, _)| *id)
                .collect();

            for node_id in stale_nodes {
                info!("Removing stale health record for node {}", node_id);
                health_state.remove(&node_id);
            }
        }
    }

    /// Initiate graceful shutdown
    pub async fn shutdown(&self) {
        info!("Initiating health checker shutdown");
        *self.shutdown.write().await = true;
    }
}

/// Builder for HealthChecker
pub struct HealthCheckerBuilder {
    nodes: Vec<(u64, String)>,
}

impl HealthCheckerBuilder {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
        }
    }

    pub fn add_node(mut self, node_id: u64, endpoint: String) -> Self {
        self.nodes.push((node_id, endpoint));
        self
    }

    pub fn with_nodes(mut self, nodes: Vec<(u64, String)>) -> Self {
        self.nodes = nodes;
        self
    }

    pub fn with_etcd(self, _etcd: Arc<Mutex<threshold_storage::EtcdStorage>>) -> Self {
        // Ignore etcd parameter for now - using in-memory state instead
        self
    }

    pub fn build(self) -> Result<Arc<HealthChecker>> {
        Ok(Arc::new(HealthChecker::new(self.nodes)))
    }
}

impl Default for HealthCheckerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder() {
        let builder = HealthCheckerBuilder::new()
            .add_node(1, "http://node1:8080".to_string())
            .add_node(2, "http://node2:8080".to_string());

        assert_eq!(builder.nodes.len(), 2);
    }

    #[test]
    fn test_node_health_serialization() {
        let health = NodeHealth {
            node_id: 1,
            endpoint: "http://node1:8080".to_string(),
            is_healthy: true,
            latency_ms: Some(50),
            last_check: chrono::Utc::now(),
            consecutive_failures: 0,
        };

        let json = serde_json::to_string(&health).unwrap();
        let deserialized: NodeHealth = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.node_id, 1);
        assert!(deserialized.is_healthy);
        assert_eq!(deserialized.latency_ms, Some(50));
    }

    #[tokio::test]
    async fn test_health_checker_creation() {
        let nodes = vec![
            (1, "http://node1:8080".to_string()),
            (2, "http://node2:8080".to_string()),
        ];

        let checker = HealthChecker::new(nodes);
        assert_eq!(checker.nodes.len(), 2);

        let status = checker.get_health_status().await;
        assert_eq!(status.len(), 0); // No checks run yet
    }
}
