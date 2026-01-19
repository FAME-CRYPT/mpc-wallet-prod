//! Background health checker for registered nodes.
//!
//! This module spawns a background task that periodically:
//! 1. Pings all registered nodes to measure latency
//! 2. Updates health metrics based on responses
//! 3. Cleans up stale nodes that have missed heartbeats

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::state::AppState;

/// Interval between health check runs (seconds).
const HEALTH_CHECK_INTERVAL_SECS: u64 = 15;

/// Timeout for health check requests (seconds).
const HEALTH_CHECK_TIMEOUT_SECS: u64 = 5;

/// Spawn the background health checker task.
///
/// This task periodically:
/// 1. Pings all registered nodes to measure latency
/// 2. Updates health metrics based on responses
/// 3. Cleans up stale nodes that have missed heartbeats
pub fn spawn_health_checker(state: Arc<RwLock<AppState>>) -> tokio::task::JoinHandle<()> {
    info!(
        "Starting background health checker (interval: {}s)",
        HEALTH_CHECK_INTERVAL_SECS
    );

    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(HEALTH_CHECK_TIMEOUT_SECS))
            .build()
            .expect("Failed to create HTTP client");

        let mut interval = tokio::time::interval(Duration::from_secs(HEALTH_CHECK_INTERVAL_SECS));

        loop {
            interval.tick().await;

            // Get list of nodes to check
            let nodes_to_check: Vec<(uuid::Uuid, String)> = {
                let s = state.read().await;
                s.node_registry.nodes_for_health_check()
            };

            if nodes_to_check.is_empty() {
                debug!("No nodes registered, skipping health check");
                continue;
            }

            debug!("Running health check for {} nodes", nodes_to_check.len());

            // Check each node in parallel
            let check_futures: Vec<_> = nodes_to_check
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

            // Update registry with results
            {
                let mut s = state.write().await;

                for (node_id, success, latency_ms) in results {
                    if success {
                        if let Some(latency) = latency_ms {
                            s.node_registry
                                .record_health_check_success(node_id, latency);
                        }
                    } else {
                        s.node_registry.record_heartbeat_failure(node_id);
                    }
                }

                // Clean up stale nodes
                let stale_count = s.node_registry.cleanup_stale();
                if stale_count > 0 {
                    info!("Marked {} stale nodes as offline", stale_count);
                }
            }
        }
    })
}
