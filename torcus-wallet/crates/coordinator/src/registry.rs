//! Node registry for tracking registered MPC nodes.
//!
//! This module implements the coordinator-side node registry that tracks
//! node registrations, processes heartbeats, and calculates health scores.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::{debug, info, warn};
use uuid::Uuid;

use common::discovery::{NodeCapabilities, NodeInfo};

/// Configuration for the node registry.
#[derive(Debug, Clone)]
pub struct RegistryConfig {
    /// Expected heartbeat interval from nodes (seconds)
    pub heartbeat_interval_secs: u64,
    /// Number of missed heartbeats before marking node offline
    pub max_missed_heartbeats: u64,
    /// Whether to allow new node registrations (can be disabled for security)
    pub allow_registration: bool,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval_secs: 10,
            max_missed_heartbeats: 3,
            allow_registration: true,
        }
    }
}

/// Node registry for tracking registered MPC nodes.
#[derive(Debug)]
pub struct NodeRegistry {
    /// Registered nodes by node_id
    nodes: HashMap<Uuid, NodeInfo>,
    /// Index: party_index -> node_id (for quick lookup)
    party_index_map: HashMap<u16, Uuid>,
    /// Secret tokens for certificate issuance (party_index -> token_hash)
    /// We store the hash, not the raw token, for security.
    cert_token_hashes: HashMap<u16, String>,
    /// Configuration
    config: RegistryConfig,
}

impl NodeRegistry {
    /// Create a new empty registry.
    pub fn new(config: RegistryConfig) -> Self {
        Self {
            nodes: HashMap::new(),
            party_index_map: HashMap::new(),
            cert_token_hashes: HashMap::new(),
            config,
        }
    }

    /// Create with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(RegistryConfig::default())
    }

    /// Register a new node or update existing registration.
    ///
    /// Returns `Some(cert_token)` for new registrations (token for certificate issuance),
    /// or `None` for updates to existing registrations.
    pub fn register(
        &mut self,
        node_id: Uuid,
        party_index: u16,
        endpoint: String,
        capabilities: NodeCapabilities,
    ) -> Result<Option<String>, RegistryError> {
        if !self.config.allow_registration {
            return Err(RegistryError::RegistrationDisabled);
        }

        // Check if party_index is already registered by a different node
        if let Some(&existing_id) = self.party_index_map.get(&party_index) {
            if existing_id != node_id {
                // Allow re-registration if the old node is offline
                if let Some(existing) = self.nodes.get(&existing_id) {
                    if existing.is_online {
                        return Err(RegistryError::PartyIndexConflict {
                            party_index,
                            existing_node_id: existing_id,
                        });
                    }
                }
                // Remove the old registration
                info!(
                    "Removing stale registration for party {} (old node: {})",
                    party_index, existing_id
                );
                self.nodes.remove(&existing_id);
            }
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Some(existing) = self.nodes.get_mut(&node_id) {
            // Update existing registration
            info!(
                "Updating registration for node {} (party {})",
                node_id, party_index
            );
            existing.endpoint = endpoint;
            existing.capabilities = capabilities;
            existing.last_heartbeat = now;
            existing.is_online = true;
            // Keep existing health metrics
            // No new cert_token for updates
            Ok(None)
        } else {
            // New registration - generate a cert_token
            info!(
                "Registering new node {} (party {}) at {}",
                node_id, party_index, endpoint
            );
            let mut node = NodeInfo::new(node_id, party_index, endpoint);
            node.capabilities = capabilities;
            self.nodes.insert(node_id, node);
            self.party_index_map.insert(party_index, node_id);

            // Generate a secure random token for certificate issuance
            let cert_token = Uuid::new_v4().to_string();
            // Store the hash of the token (not the token itself)
            let token_hash = Self::hash_token(&cert_token);
            self.cert_token_hashes.insert(party_index, token_hash);

            Ok(Some(cert_token))
        }
    }

    /// Process a heartbeat from a node.
    pub fn heartbeat(
        &mut self,
        node_id: Uuid,
        capabilities: NodeCapabilities,
        latency_ms: Option<u64>,
    ) -> Result<f64, RegistryError> {
        let node = self
            .nodes
            .get_mut(&node_id)
            .ok_or(RegistryError::NodeNotFound { node_id })?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        node.last_heartbeat = now;
        node.is_online = true;
        node.capabilities = capabilities;
        node.update_uptime();
        node.health.record_heartbeat(true, latency_ms);

        debug!(
            "Heartbeat from node {} (party {}): latency={:?}ms, health={:.2}",
            node_id,
            node.party_index,
            latency_ms,
            node.health.health_score()
        );

        Ok(node.health.health_score())
    }

    /// Record a successful health check with measured latency.
    pub fn record_health_check_success(&mut self, node_id: Uuid, latency_ms: u64) {
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.health.record_heartbeat(true, Some(latency_ms));
            debug!(
                "Health check OK for node {} (party {}): latency={}ms",
                node_id, node.party_index, latency_ms
            );
        }
    }

    /// Mark a node's heartbeat as failed (coordinator couldn't reach it).
    pub fn record_heartbeat_failure(&mut self, node_id: Uuid) {
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.health.record_heartbeat(false, None);
            debug!(
                "Recorded heartbeat failure for node {} (party {})",
                node_id, node.party_index
            );
        }
    }

    /// Get a node by ID.
    #[allow(dead_code)]
    pub fn get(&self, node_id: &Uuid) -> Option<&NodeInfo> {
        self.nodes.get(node_id)
    }

    /// Get a node by party index.
    pub fn get_by_party(&self, party_index: u16) -> Option<&NodeInfo> {
        self.party_index_map
            .get(&party_index)
            .and_then(|id| self.nodes.get(id))
    }

    /// Get all online nodes.
    pub fn online_nodes(&self) -> Vec<&NodeInfo> {
        self.nodes.values().filter(|n| n.is_online).collect()
    }

    /// Get all registered nodes.
    pub fn all_nodes(&self) -> Vec<&NodeInfo> {
        self.nodes.values().collect()
    }

    /// Get online node endpoints sorted by party index (for backward compatibility).
    #[allow(dead_code)]
    pub fn online_endpoints(&self) -> Vec<String> {
        let mut endpoints: Vec<_> = self
            .online_nodes()
            .iter()
            .map(|n| (n.party_index, n.endpoint.clone()))
            .collect();
        endpoints.sort_by_key(|(idx, _)| *idx);
        endpoints.into_iter().map(|(_, e)| e).collect()
    }

    /// Get nodes that hold a key share for a specific wallet.
    #[allow(dead_code)]
    pub fn nodes_for_wallet(&self, wallet_id: &str) -> Vec<&NodeInfo> {
        self.online_nodes()
            .into_iter()
            .filter(|n| n.capabilities.wallet_ids.contains(&wallet_id.to_string()))
            .collect()
    }

    /// Clean up stale nodes (mark as offline).
    pub fn cleanup_stale(&mut self) -> usize {
        let mut marked_offline = 0;

        for node in self.nodes.values_mut() {
            if node.is_online
                && node.is_stale(
                    self.config.heartbeat_interval_secs,
                    self.config.max_missed_heartbeats,
                )
            {
                warn!(
                    "Node {} (party {}) is stale, marking offline",
                    node.node_id, node.party_index
                );
                node.is_online = false;
                marked_offline += 1;
            }
        }

        if marked_offline > 0 {
            info!(
                "Marked {} nodes as offline due to missed heartbeats",
                marked_offline
            );
        }

        marked_offline
    }

    /// Get registry statistics.
    pub fn stats(&self) -> RegistryStats {
        let total = self.nodes.len();
        let online = self.nodes.values().filter(|n| n.is_online).count();
        let avg_health: f64 = if online > 0 {
            self.online_nodes()
                .iter()
                .map(|n| n.health.health_score())
                .sum::<f64>()
                / online as f64
        } else {
            0.0
        };

        RegistryStats {
            total_nodes: total,
            online_nodes: online,
            avg_health_score: avg_health,
        }
    }

    /// Get the heartbeat interval configuration.
    pub fn heartbeat_interval(&self) -> u64 {
        self.config.heartbeat_interval_secs
    }

    /// Check if registry is empty.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Get node IDs and endpoints for health checking.
    pub fn nodes_for_health_check(&self) -> Vec<(Uuid, String)> {
        self.nodes
            .values()
            .map(|n| (n.node_id, n.endpoint.clone()))
            .collect()
    }

    /// Verify a certificate token for a given party index.
    ///
    /// Returns true if the token is valid for this party.
    /// Uses constant-time comparison to prevent timing attacks.
    pub fn verify_cert_token(&self, party_index: u16, token: &str) -> bool {
        use subtle::ConstantTimeEq;

        if let Some(stored_hash) = self.cert_token_hashes.get(&party_index) {
            let provided_hash = Self::hash_token(token);
            // Use constant-time comparison to prevent timing attacks
            stored_hash
                .as_bytes()
                .ct_eq(provided_hash.as_bytes())
                .into()
        } else {
            false
        }
    }

    /// Verify that a node_id is registered for the given party_index.
    ///
    /// This ensures that a valid cert_token holder cannot impersonate
    /// another node by supplying a different node_id.
    pub fn verify_node_for_party(&self, party_index: u16, node_id: Uuid) -> bool {
        if let Some(&registered_node_id) = self.party_index_map.get(&party_index) {
            registered_node_id == node_id
        } else {
            false
        }
    }

    /// Verify both cert_token and node_id for a party.
    ///
    /// This is the complete authentication check for heartbeats:
    /// 1. The cert_token must be valid for this party
    /// 2. The node_id must match the registered node for this party
    ///
    /// Returns an error message if verification fails, or Ok(()) if valid.
    pub fn verify_heartbeat_auth(
        &self,
        party_index: u16,
        node_id: Uuid,
        cert_token: &str,
    ) -> Result<(), &'static str> {
        // First verify the cert_token
        if !self.verify_cert_token(party_index, cert_token) {
            return Err("Invalid cert_token");
        }

        // Then verify the node_id matches the registered node for this party
        if !self.verify_node_for_party(party_index, node_id) {
            return Err("node_id does not match registered node for this party");
        }

        Ok(())
    }

    /// Hash a token using SHA-256.
    fn hash_token(token: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        hex::encode(hasher.finalize())
    }
}

/// Registry statistics.
#[derive(Debug, Clone)]
pub struct RegistryStats {
    pub total_nodes: usize,
    pub online_nodes: usize,
    pub avg_health_score: f64,
}

/// Registry errors.
#[derive(Debug, Clone)]
pub enum RegistryError {
    RegistrationDisabled,
    NodeNotFound {
        node_id: Uuid,
    },
    PartyIndexConflict {
        party_index: u16,
        existing_node_id: Uuid,
    },
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RegistrationDisabled => write!(f, "Node registration is disabled"),
            Self::NodeNotFound { node_id } => write!(f, "Node {} not found", node_id),
            Self::PartyIndexConflict {
                party_index,
                existing_node_id,
            } => {
                write!(
                    f,
                    "Party index {} already registered by node {}",
                    party_index, existing_node_id
                )
            }
        }
    }
}

impl std::error::Error for RegistryError {}

#[cfg(test)]
mod tests {
    use super::*;
    use common::discovery::ProtocolCapability;

    fn test_capabilities() -> NodeCapabilities {
        NodeCapabilities {
            protocols: vec![ProtocolCapability::Cggmp24],
            has_primes: true,
            has_aux_info: true,
            cggmp24_keyshare_count: 1,
            frost_keyshare_count: 0,
            wallet_ids: vec!["test-wallet".to_string()],
        }
    }

    #[test]
    fn test_register_new_node() {
        let mut registry = NodeRegistry::with_defaults();
        let node_id = Uuid::new_v4();

        let result = registry.register(
            node_id,
            0,
            "http://localhost:3000".to_string(),
            test_capabilities(),
        );

        assert!(result.is_ok());
        assert_eq!(registry.all_nodes().len(), 1);
        assert!(registry.get(&node_id).is_some());
        assert!(registry.get_by_party(0).is_some());
    }

    #[test]
    fn test_register_update_existing() {
        let mut registry = NodeRegistry::with_defaults();
        let node_id = Uuid::new_v4();

        registry
            .register(
                node_id,
                0,
                "http://localhost:3000".to_string(),
                test_capabilities(),
            )
            .unwrap();

        // Update with new endpoint
        registry
            .register(
                node_id,
                0,
                "http://localhost:3001".to_string(),
                test_capabilities(),
            )
            .unwrap();

        assert_eq!(registry.all_nodes().len(), 1);
        assert_eq!(
            registry.get(&node_id).unwrap().endpoint,
            "http://localhost:3001"
        );
    }

    #[test]
    fn test_party_index_conflict() {
        let mut registry = NodeRegistry::with_defaults();
        let node1 = Uuid::new_v4();
        let node2 = Uuid::new_v4();

        registry
            .register(
                node1,
                0,
                "http://localhost:3000".to_string(),
                test_capabilities(),
            )
            .unwrap();

        // Different node trying to register same party index should fail
        let result = registry.register(
            node2,
            0,
            "http://localhost:3001".to_string(),
            test_capabilities(),
        );

        assert!(matches!(
            result,
            Err(RegistryError::PartyIndexConflict { .. })
        ));
    }

    #[test]
    fn test_heartbeat() {
        let mut registry = NodeRegistry::with_defaults();
        let node_id = Uuid::new_v4();

        registry
            .register(
                node_id,
                0,
                "http://localhost:3000".to_string(),
                test_capabilities(),
            )
            .unwrap();

        let score = registry
            .heartbeat(node_id, test_capabilities(), Some(50))
            .unwrap();

        assert!(score > 0.0);
        assert_eq!(
            registry.get(&node_id).unwrap().health.successful_heartbeats,
            1
        );
    }

    #[test]
    fn test_online_endpoints() {
        let mut registry = NodeRegistry::with_defaults();

        registry
            .register(
                Uuid::new_v4(),
                1,
                "http://node-2:3000".to_string(),
                test_capabilities(),
            )
            .unwrap();
        registry
            .register(
                Uuid::new_v4(),
                0,
                "http://node-1:3000".to_string(),
                test_capabilities(),
            )
            .unwrap();

        let endpoints = registry.online_endpoints();
        assert_eq!(endpoints.len(), 2);
        // Should be sorted by party index
        assert_eq!(endpoints[0], "http://node-1:3000");
        assert_eq!(endpoints[1], "http://node-2:3000");
    }

    #[test]
    fn test_stats() {
        let mut registry = NodeRegistry::with_defaults();

        registry
            .register(
                Uuid::new_v4(),
                0,
                "http://localhost:3000".to_string(),
                test_capabilities(),
            )
            .unwrap();
        registry
            .register(
                Uuid::new_v4(),
                1,
                "http://localhost:3001".to_string(),
                test_capabilities(),
            )
            .unwrap();

        let stats = registry.stats();
        assert_eq!(stats.total_nodes, 2);
        assert_eq!(stats.online_nodes, 2);
        assert!(stats.avg_health_score > 0.0);
    }

    #[test]
    fn test_cert_token_issued_on_new_registration() {
        let mut registry = NodeRegistry::with_defaults();
        let node_id = Uuid::new_v4();

        let result = registry
            .register(
                node_id,
                0,
                "http://localhost:3000".to_string(),
                test_capabilities(),
            )
            .unwrap();

        // New registration should return a cert_token
        assert!(result.is_some());
        let cert_token = result.unwrap();

        // Token should be verifiable
        assert!(registry.verify_cert_token(0, &cert_token));

        // Wrong token should not verify
        assert!(!registry.verify_cert_token(0, "wrong-token"));

        // Wrong party should not verify
        assert!(!registry.verify_cert_token(1, &cert_token));
    }

    #[test]
    fn test_cert_token_not_issued_on_update() {
        let mut registry = NodeRegistry::with_defaults();
        let node_id = Uuid::new_v4();

        // First registration - should get cert_token
        let first_result = registry
            .register(
                node_id,
                0,
                "http://localhost:3000".to_string(),
                test_capabilities(),
            )
            .unwrap();
        assert!(first_result.is_some());
        let cert_token = first_result.unwrap();

        // Update registration - should NOT get new cert_token
        let update_result = registry
            .register(
                node_id,
                0,
                "http://localhost:3001".to_string(),
                test_capabilities(),
            )
            .unwrap();
        assert!(update_result.is_none());

        // Original token should still work
        assert!(registry.verify_cert_token(0, &cert_token));
    }

    #[test]
    fn test_cert_token_constant_time_comparison() {
        let mut registry = NodeRegistry::with_defaults();
        let node_id = Uuid::new_v4();

        let result = registry
            .register(
                node_id,
                0,
                "http://localhost:3000".to_string(),
                test_capabilities(),
            )
            .unwrap();

        let cert_token = result.unwrap();

        // Valid token should verify
        assert!(registry.verify_cert_token(0, &cert_token));

        // Similar but wrong tokens should not verify
        // (these tests ensure the comparison is actually working)
        assert!(!registry.verify_cert_token(0, &format!("{}x", cert_token)));
        assert!(!registry.verify_cert_token(0, ""));
        assert!(!registry.verify_cert_token(0, "a".repeat(cert_token.len()).as_str()));

        // Non-existent party should not verify
        assert!(!registry.verify_cert_token(99, &cert_token));
    }

    #[test]
    fn test_verify_heartbeat_auth() {
        let mut registry = NodeRegistry::with_defaults();
        let node_id = Uuid::new_v4();
        let other_node_id = Uuid::new_v4();

        let result = registry
            .register(
                node_id,
                0,
                "http://localhost:3000".to_string(),
                test_capabilities(),
            )
            .unwrap();

        let cert_token = result.unwrap();

        // Valid token + correct node_id should pass
        assert!(registry
            .verify_heartbeat_auth(0, node_id, &cert_token)
            .is_ok());

        // Valid token + wrong node_id should fail
        // This prevents impersonation attacks
        let result = registry.verify_heartbeat_auth(0, other_node_id, &cert_token);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("node_id"));

        // Invalid token + correct node_id should fail
        let result = registry.verify_heartbeat_auth(0, node_id, "wrong-token");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cert_token"));

        // Wrong party should fail
        let result = registry.verify_heartbeat_auth(99, node_id, &cert_token);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_node_for_party() {
        let mut registry = NodeRegistry::with_defaults();
        let node_id = Uuid::new_v4();
        let other_node_id = Uuid::new_v4();

        registry
            .register(
                node_id,
                0,
                "http://localhost:3000".to_string(),
                test_capabilities(),
            )
            .unwrap();

        // Correct node_id for party 0
        assert!(registry.verify_node_for_party(0, node_id));

        // Wrong node_id for party 0
        assert!(!registry.verify_node_for_party(0, other_node_id));

        // Non-existent party
        assert!(!registry.verify_node_for_party(99, node_id));
    }
}
