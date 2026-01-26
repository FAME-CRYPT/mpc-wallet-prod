//! Node Heartbeat Service
//!
//! Periodically sends heartbeat signals to update node health status in PostgreSQL.
//! This allows the cluster to track which nodes are alive and responsive.

use std::sync::Arc;
use std::time::Duration;
use threshold_storage::PostgresStorage;
use threshold_types::NodeId;
use tokio::time;
use tracing::{error, info};

/// Heartbeat service for node health monitoring
pub struct HeartbeatService {
    /// PostgreSQL storage
    postgres: Arc<PostgresStorage>,
    /// Current node ID
    node_id: NodeId,
    /// Heartbeat interval (default: 10 seconds)
    interval: Duration,
}

impl HeartbeatService {
    /// Create a new heartbeat service
    ///
    /// # Arguments
    /// * `postgres` - PostgreSQL storage instance
    /// * `node_id` - Current node's ID
    ///
    /// # Returns
    /// New HeartbeatService instance
    pub fn new(postgres: Arc<PostgresStorage>, node_id: NodeId) -> Self {
        Self {
            postgres,
            node_id,
            interval: Duration::from_secs(10), // Send heartbeat every 10 seconds
        }
    }

    /// Create with custom interval
    pub fn with_interval(postgres: Arc<PostgresStorage>, node_id: NodeId, interval: Duration) -> Self {
        Self {
            postgres,
            node_id,
            interval,
        }
    }

    /// Start the heartbeat loop (runs indefinitely)
    ///
    /// This should be spawned as a background task using `tokio::spawn`
    pub async fn start(self: Arc<Self>) {
        let mut interval = time::interval(self.interval);

        info!("Starting heartbeat service for node {} (interval: {:?})", self.node_id, self.interval);

        loop {
            interval.tick().await;

            // Update node heartbeat in PostgreSQL
            match self.postgres.update_node_last_seen(&self.node_id).await {
                Ok(_) => {
                    info!("Heartbeat sent successfully for node {}", self.node_id);
                }
                Err(e) => {
                    error!("Failed to send heartbeat for node {}: {}", self.node_id, e);
                    // Continue trying - don't crash the service
                }
            }
        }
    }

    /// Spawn heartbeat service as a background task
    ///
    /// # Returns
    /// tokio::JoinHandle that can be used to monitor or stop the service
    pub fn spawn(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            self.start().await;
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_heartbeat_service_creation() {
        // This is a basic test to ensure the service can be created
        // Real testing would require a PostgreSQL connection

        // Note: Cannot actually test without a real PostgreSQL instance
        // This test just verifies compilation and structure
    }
}
