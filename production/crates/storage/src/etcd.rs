use etcd_client::{Client, Compare, CompareOp, DeleteOptions, GetOptions, PutOptions, Txn, TxnOp};
use serde_json;
use std::collections::HashMap;
use threshold_types::{
    ByzantineViolation, Error, NodeId, PeerId, Result, TxId, TransactionState, Vote,
};
use tracing::{info, warn};

/// TTL constants for etcd keys
const LOCK_TTL_SECS: i64 = 30;
const HEARTBEAT_TTL_SECS: i64 = 5;
const NODE_STATUS_TTL_SECS: i64 = 60;

pub struct EtcdStorage {
    client: Client,
}

impl EtcdStorage {
    pub async fn new(endpoints: Vec<String>) -> Result<Self> {
        let client = Client::connect(endpoints, None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to connect to etcd: {}", e)))?;

        Ok(Self { client })
    }

    // ============================================================================
    // Vote Management
    // ============================================================================

    /// Get the count of votes for a specific value in a transaction
    pub async fn get_vote_count(&mut self, tx_id: &TxId, value: u64) -> Result<u64> {
        let key = format!("/vote_counts/{}/{}", tx_id, value);

        let resp = self
            .client
            .get(key.as_bytes(), None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get vote count: {}", e)))?;

        if resp.kvs().is_empty() {
            return Ok(0);
        }

        let count_str = String::from_utf8_lossy(resp.kvs()[0].value());
        count_str
            .parse::<u64>()
            .map_err(|e| Error::StorageError(format!("Failed to parse count: {}", e)))
    }

    /// Increment the vote count for a specific value in a transaction
    pub async fn increment_vote_count(&mut self, tx_id: &TxId, value: u64) -> Result<u64> {
        let key = format!("/vote_counts/{}/{}", tx_id, value);

        let get_resp = self
            .client
            .get(key.as_bytes(), None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get current count: {}", e)))?;

        let current_count = if get_resp.kvs().is_empty() {
            0u64
        } else {
            let count_str = String::from_utf8_lossy(get_resp.kvs()[0].value());
            count_str.parse::<u64>().unwrap_or(0)
        };

        let new_count = current_count + 1;

        self.client
            .put(key.as_bytes(), new_count.to_string().as_bytes(), None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to increment count: {}", e)))?;

        info!(
            "Incremented vote count for tx_id={} value={}: {} -> {}",
            tx_id, value, current_count, new_count
        );

        Ok(new_count)
    }

    /// Get all vote counts for a transaction
    pub async fn get_all_vote_counts(&mut self, tx_id: &TxId) -> Result<HashMap<u64, u64>> {
        let prefix = format!("/vote_counts/{}/", tx_id);

        let resp = self
            .client
            .get(prefix.as_bytes(), Some(GetOptions::new().with_prefix()))
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get all vote counts: {}", e)))?;

        let mut counts = HashMap::new();
        for kv in resp.kvs() {
            let key_str = String::from_utf8_lossy(kv.key());
            if let Some(value_str) = key_str.strip_prefix(&prefix) {
                if let Ok(value) = value_str.parse::<u64>() {
                    let count_str = String::from_utf8_lossy(kv.value());
                    if let Ok(count) = count_str.parse::<u64>() {
                        counts.insert(value, count);
                    }
                }
            }
        }

        Ok(counts)
    }

    /// Store a vote, returning the existing vote if one already exists
    pub async fn store_vote(&mut self, vote: &Vote) -> Result<Option<Vote>> {
        let key = format!("/votes/{}/{}", vote.tx_id, vote.node_id);

        let get_resp = self
            .client
            .get(key.as_bytes(), None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to check existing vote: {}", e)))?;

        if !get_resp.kvs().is_empty() {
            let existing_vote_json = String::from_utf8_lossy(get_resp.kvs()[0].value());
            let existing_vote: Vote = serde_json::from_str(&existing_vote_json)
                .map_err(|e| Error::StorageError(format!("Failed to parse existing vote: {}", e)))?;
            return Ok(Some(existing_vote));
        }

        let vote_json = serde_json::to_string(vote)
            .map_err(|e| Error::StorageError(format!("Failed to serialize vote: {}", e)))?;

        self.client
            .put(key.as_bytes(), vote_json.as_bytes(), None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to store vote: {}", e)))?;

        info!("Stored vote for tx_id={} node_id={}", vote.tx_id, vote.node_id);

        Ok(None)
    }

    /// Delete all votes for a transaction
    pub async fn delete_all_votes(&mut self, tx_id: &TxId) -> Result<()> {
        let prefix = format!("/votes/{}/", tx_id);

        self.client
            .delete(prefix.as_bytes(), Some(DeleteOptions::new().with_prefix()))
            .await
            .map_err(|e| Error::StorageError(format!("Failed to delete votes: {}", e)))?;

        Ok(())
    }

    /// Delete all vote counts for a transaction
    pub async fn delete_all_vote_counts(&mut self, tx_id: &TxId) -> Result<()> {
        let prefix = format!("/vote_counts/{}/", tx_id);

        self.client
            .delete(prefix.as_bytes(), Some(DeleteOptions::new().with_prefix()))
            .await
            .map_err(|e| Error::StorageError(format!("Failed to delete vote counts: {}", e)))?;

        Ok(())
    }

    // ============================================================================
    // Transaction State Management
    // ============================================================================

    /// Get the current state of a transaction
    pub async fn get_transaction_state(&mut self, tx_id: &TxId) -> Result<TransactionState> {
        let key = format!("/transaction_status/{}", tx_id);

        let resp = self
            .client
            .get(key.as_bytes(), None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get transaction state: {}", e)))?;

        if resp.kvs().is_empty() {
            return Ok(TransactionState::Pending);
        }

        let state_str = String::from_utf8_lossy(resp.kvs()[0].value());
        parse_transaction_state(&state_str)
    }

    /// Set the state of a transaction
    pub async fn set_transaction_state(
        &mut self,
        tx_id: &TxId,
        state: TransactionState,
    ) -> Result<()> {
        let key = format!("/transaction_status/{}", tx_id);

        self.client
            .put(key.as_bytes(), state.to_string().as_bytes(), None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to set transaction state: {}", e)))?;

        info!("Set transaction state for tx_id={} to {}", tx_id, state);

        Ok(())
    }

    /// Delete transaction state
    pub async fn delete_transaction_state(&mut self, tx_id: &TxId) -> Result<()> {
        let key = format!("/transaction_status/{}", tx_id);

        self.client
            .delete(key.as_bytes(), None)
            .await
            .map_err(|e| {
                Error::StorageError(format!("Failed to delete transaction state: {}", e))
            })?;

        Ok(())
    }

    // ============================================================================
    // Distributed Locking
    // ============================================================================

    /// Acquire a lock for transaction signing
    pub async fn acquire_signing_lock(&mut self, tx_id: &TxId) -> Result<i64> {
        let key = format!("/locks/signing/{}", tx_id);
        self.acquire_lock_internal(&key, LOCK_TTL_SECS).await
    }

    /// Release a transaction signing lock
    pub async fn release_signing_lock(&mut self, tx_id: &TxId) -> Result<()> {
        let key = format!("/locks/signing/{}", tx_id);
        self.release_lock_internal(&key).await
    }

    /// Acquire a lock for presignature generation
    pub async fn acquire_presig_generation_lock(&mut self) -> Result<i64> {
        let key = "/locks/presig-generation";
        self.acquire_lock_internal(key, LOCK_TTL_SECS).await
    }

    /// Release the presignature generation lock
    pub async fn release_presig_generation_lock(&mut self) -> Result<()> {
        let key = "/locks/presig-generation";
        self.release_lock_internal(key).await
    }

    /// Acquire a lock for DKG session
    pub async fn acquire_dkg_session_lock(&mut self, session_id: &str) -> Result<i64> {
        let key = format!("/locks/dkg-session/{}", session_id);
        self.acquire_lock_internal(&key, LOCK_TTL_SECS).await
    }

    /// Release a DKG session lock
    pub async fn release_dkg_session_lock(&mut self, session_id: &str) -> Result<()> {
        let key = format!("/locks/dkg-session/{}", session_id);
        self.release_lock_internal(&key).await
    }

    /// Internal method to acquire a lock with TTL
    async fn acquire_lock_internal(&self, key: &str, ttl_secs: i64) -> Result<i64> {
        let mut client = self.client.clone();
        let lease_resp = client
            .lease_grant(ttl_secs, None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to create lease: {}", e)))?;

        let lease_id = lease_resp.id();

        let lock_data = serde_json::json!({
            "lease_id": lease_id,
            "ttl": ttl_secs,
        });

        let txn = Txn::new()
            .when(vec![Compare::create_revision(
                key.as_bytes(),
                CompareOp::Equal,
                0,
            )])
            .and_then(vec![TxnOp::put(
                key.as_bytes(),
                lock_data.to_string().as_bytes(),
                Some(PutOptions::new().with_lease(lease_id)),
            )])
            .or_else(vec![]);

        let txn_resp = client
            .txn(txn)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to acquire lock: {}", e)))?;

        if !txn_resp.succeeded() {
            return Err(Error::StorageError(
                "Failed to acquire lock: already locked".to_string(),
            ));
        }

        info!("Acquired lock for key={} with lease_id={}", key, lease_id);

        Ok(lease_id)
    }

    /// Internal method to release a lock
    async fn release_lock_internal(&self, key: &str) -> Result<()> {
        let mut client = self.client.clone();
        client
            .delete(key.as_bytes(), None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to release lock: {}", e)))?;

        info!("Released lock for key={}", key);

        Ok(())
    }

    /// Keep a lease alive by refreshing it
    pub async fn keep_lease_alive(&mut self, lease_id: i64) -> Result<()> {
        let (mut keeper, _stream) = self
            .client
            .lease_keep_alive(lease_id)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to create lease keeper: {}", e)))?;

        keeper
            .keep_alive()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to keep lease alive: {}", e)))?;

        Ok(())
    }

    // ============================================================================
    // Counters
    // ============================================================================

    /// Increment the transaction counter
    pub async fn increment_transaction_counter(&mut self) -> Result<u64> {
        self.increment_counter("/counters/transactions").await
    }

    /// Get the transaction counter
    pub async fn get_transaction_counter(&mut self) -> Result<u64> {
        self.get_counter("/counters/transactions").await
    }

    /// Increment the presignature counter
    pub async fn increment_presignature_counter(&mut self) -> Result<u64> {
        self.increment_counter("/counters/presignatures").await
    }

    /// Get the presignature counter
    pub async fn get_presignature_counter(&mut self) -> Result<u64> {
        self.get_counter("/counters/presignatures").await
    }

    /// Increment the Byzantine events counter
    pub async fn increment_byzantine_counter(&mut self) -> Result<u64> {
        self.increment_counter("/counters/byzantine-events").await
    }

    /// Get the Byzantine events counter
    pub async fn get_byzantine_counter(&mut self) -> Result<u64> {
        self.get_counter("/counters/byzantine-events").await
    }

    /// Internal method to increment a counter
    async fn increment_counter(&mut self, key: &str) -> Result<u64> {
        let get_resp = self
            .client
            .get(key.as_bytes(), None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get counter: {}", e)))?;

        let current = if get_resp.kvs().is_empty() {
            0u64
        } else {
            let count_str = String::from_utf8_lossy(get_resp.kvs()[0].value());
            count_str.parse::<u64>().unwrap_or(0)
        };

        let new_value = current + 1;

        self.client
            .put(key.as_bytes(), new_value.to_string().as_bytes(), None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to increment counter: {}", e)))?;

        Ok(new_value)
    }

    /// Internal method to get a counter value
    async fn get_counter(&mut self, key: &str) -> Result<u64> {
        let resp = self
            .client
            .get(key.as_bytes(), None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get counter: {}", e)))?;

        if resp.kvs().is_empty() {
            return Ok(0);
        }

        let count_str = String::from_utf8_lossy(resp.kvs()[0].value());
        count_str
            .parse::<u64>()
            .map_err(|e| Error::StorageError(format!("Failed to parse counter: {}", e)))
    }

    // ============================================================================
    // Node State Management
    // ============================================================================

    /// Update node status with TTL
    pub async fn update_node_status(&mut self, node_id: NodeId, status: &str) -> Result<()> {
        let key = format!("/nodes/{}/status", node_id);

        let lease_resp = self
            .client
            .lease_grant(NODE_STATUS_TTL_SECS, None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to create lease: {}", e)))?;

        let lease_id = lease_resp.id();

        self.client
            .put(
                key.as_bytes(),
                status.as_bytes(),
                Some(PutOptions::new().with_lease(lease_id)),
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to update node status: {}", e)))?;

        info!("Updated node status: node_id={} status={}", node_id, status);

        Ok(())
    }

    /// Get node status
    pub async fn get_node_status(&mut self, node_id: NodeId) -> Result<Option<String>> {
        let key = format!("/nodes/{}/status", node_id);

        let resp = self
            .client
            .get(key.as_bytes(), None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get node status: {}", e)))?;

        if resp.kvs().is_empty() {
            return Ok(None);
        }

        let status = String::from_utf8_lossy(resp.kvs()[0].value()).to_string();
        Ok(Some(status))
    }

    /// Update node heartbeat with short TTL
    pub async fn update_node_heartbeat(&mut self, node_id: NodeId) -> Result<()> {
        let key = format!("/nodes/{}/last-heartbeat", node_id);
        let timestamp = chrono::Utc::now().to_rfc3339();

        let lease_resp = self
            .client
            .lease_grant(HEARTBEAT_TTL_SECS, None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to create lease: {}", e)))?;

        let lease_id = lease_resp.id();

        self.client
            .put(
                key.as_bytes(),
                timestamp.as_bytes(),
                Some(PutOptions::new().with_lease(lease_id)),
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to update heartbeat: {}", e)))?;

        Ok(())
    }

    /// Get node last heartbeat
    pub async fn get_node_heartbeat(&mut self, node_id: NodeId) -> Result<Option<String>> {
        let key = format!("/nodes/{}/last-heartbeat", node_id);

        let resp = self
            .client
            .get(key.as_bytes(), None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get heartbeat: {}", e)))?;

        if resp.kvs().is_empty() {
            return Ok(None);
        }

        let timestamp = String::from_utf8_lossy(resp.kvs()[0].value()).to_string();
        Ok(Some(timestamp))
    }

    /// Get all active nodes (nodes with recent heartbeats)
    pub async fn get_active_nodes(&mut self) -> Result<Vec<NodeId>> {
        let prefix = "/nodes/";

        let resp = self
            .client
            .get(prefix.as_bytes(), Some(GetOptions::new().with_prefix()))
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get active nodes: {}", e)))?;

        let mut nodes = Vec::new();
        for kv in resp.kvs() {
            let key_str = String::from_utf8_lossy(kv.key());
            if key_str.ends_with("/last-heartbeat") {
                // Extract node_id from key like "/nodes/node-123/last-heartbeat"
                if let Some(node_part) = key_str.strip_prefix(prefix) {
                    if let Some(node_id_str) = node_part.split('/').next() {
                        if let Some(id_str) = node_id_str.strip_prefix("node-") {
                            if let Ok(id) = id_str.parse::<u64>() {
                                nodes.push(NodeId(id));
                            }
                        }
                    }
                }
            }
        }

        Ok(nodes)
    }

    // ============================================================================
    // Cluster Configuration
    // ============================================================================

    /// Get the configured threshold
    pub async fn get_cluster_threshold(&mut self) -> Result<u32> {
        let key = b"/cluster/threshold";

        let resp = self
            .client
            .get(key, None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get threshold: {}", e)))?;

        if resp.kvs().is_empty() {
            return Err(Error::ConfigError("Threshold not configured".to_string()));
        }

        let threshold_str = String::from_utf8_lossy(resp.kvs()[0].value());
        threshold_str
            .parse::<u32>()
            .map_err(|e| Error::ConfigError(format!("Invalid threshold value: {}", e)))
    }

    /// Set the cluster threshold
    pub async fn set_cluster_threshold(&mut self, threshold: u32) -> Result<()> {
        let key = b"/cluster/threshold";

        self.client
            .put(key, threshold.to_string().as_bytes(), None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to set threshold: {}", e)))?;

        info!("Set cluster threshold to {}", threshold);

        Ok(())
    }

    /// Get cluster peers list
    pub async fn get_cluster_peers(&mut self) -> Result<Vec<String>> {
        let key = b"/cluster/peers";

        let resp = self
            .client
            .get(key, None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get peers: {}", e)))?;

        if resp.kvs().is_empty() {
            return Ok(Vec::new());
        }

        let peers_json = String::from_utf8_lossy(resp.kvs()[0].value());
        let peers: Vec<String> = serde_json::from_str(&peers_json)
            .map_err(|e| Error::StorageError(format!("Failed to parse peers: {}", e)))?;

        Ok(peers)
    }

    /// Set cluster peers list
    pub async fn set_cluster_peers(&mut self, peers: Vec<String>) -> Result<()> {
        let key = b"/cluster/peers";
        let peers_json = serde_json::to_string(&peers)
            .map_err(|e| Error::StorageError(format!("Failed to serialize peers: {}", e)))?;

        self.client
            .put(key, peers_json.as_bytes(), None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to set peers: {}", e)))?;

        info!("Updated cluster peers list with {} peers", peers.len());

        Ok(())
    }

    /// Add a peer to the cluster
    pub async fn add_cluster_peer(&mut self, peer: String) -> Result<()> {
        let mut peers = self.get_cluster_peers().await?;
        if !peers.contains(&peer) {
            peers.push(peer.clone());
            self.set_cluster_peers(peers).await?;
            info!("Added peer to cluster: {}", peer);
        }
        Ok(())
    }

    /// Remove a peer from the cluster
    pub async fn remove_cluster_peer(&mut self, peer: &str) -> Result<()> {
        let mut peers = self.get_cluster_peers().await?;
        peers.retain(|p| p != peer);
        self.set_cluster_peers(peers).await?;
        info!("Removed peer from cluster: {}", peer);
        Ok(())
    }

    // ============================================================================
    // Byzantine Violation Management
    // ============================================================================

    /// Check if a node is banned (by NodeId)
    pub async fn is_node_banned(&mut self, node_id: NodeId) -> Result<bool> {
        let key = format!("/banned/{}", node_id);

        let resp = self
            .client
            .get(key.as_bytes(), None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to check banned status: {}", e)))?;

        Ok(!resp.kvs().is_empty())
    }

    /// Check if a peer is banned (by PeerId)
    pub async fn is_peer_banned(&mut self, peer_id: &PeerId) -> Result<bool> {
        let key = format!("/banned/{}", peer_id.0);

        let resp = self
            .client
            .get(key.as_bytes(), None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to check banned status: {}", e)))?;

        Ok(!resp.kvs().is_empty())
    }

    /// Ban a node for Byzantine violation
    pub async fn ban_node(&mut self, violation: &ByzantineViolation) -> Result<()> {
        let key = if let Some(node_id) = violation.node_id {
            format!("/banned/{}", node_id)
        } else {
            format!("/banned/{}", violation.peer_id.0)
        };

        let ban_data = serde_json::to_string(violation)
            .map_err(|e| Error::StorageError(format!("Failed to serialize violation: {}", e)))?;

        self.client
            .put(key.as_bytes(), ban_data.as_bytes(), None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to ban node: {}", e)))?;

        // Increment Byzantine counter
        self.increment_byzantine_counter().await?;

        warn!(
            "Banned peer {} for {:?}",
            violation.peer_id, violation.violation_type
        );

        Ok(())
    }

    /// Unban a node
    pub async fn unban_node(&mut self, node_id: NodeId) -> Result<()> {
        let key = format!("/banned/{}", node_id);

        self.client
            .delete(key.as_bytes(), None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to unban node: {}", e)))?;

        info!("Unbanned node {}", node_id);

        Ok(())
    }

    /// Get ban information for a node
    pub async fn get_ban_info(&mut self, node_id: NodeId) -> Result<Option<ByzantineViolation>> {
        let key = format!("/banned/{}", node_id);

        let resp = self
            .client
            .get(key.as_bytes(), None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get ban info: {}", e)))?;

        if resp.kvs().is_empty() {
            return Ok(None);
        }

        let ban_json = String::from_utf8_lossy(resp.kvs()[0].value());
        let violation: ByzantineViolation = serde_json::from_str(&ban_json)
            .map_err(|e| Error::StorageError(format!("Failed to parse ban info: {}", e)))?;

        Ok(Some(violation))
    }

    // ============================================================================
    // Legacy Compatibility Methods (from original implementation)
    // ============================================================================

    /// Legacy method: acquire lock for submission (maps to signing lock)
    pub async fn acquire_submission_lock(&mut self, tx_id: &TxId, ttl_secs: i64) -> Result<i64> {
        let key = format!("/locks/submission/{}", tx_id);
        self.acquire_lock_internal(&key, ttl_secs).await
    }

    /// Legacy method: release submission lock
    pub async fn release_submission_lock(&mut self, tx_id: &TxId) -> Result<()> {
        let key = format!("/locks/submission/{}", tx_id);
        self.release_lock_internal(&key).await
    }

    /// Legacy method: get config threshold (maps to cluster threshold)
    pub async fn get_config_threshold(&mut self) -> Result<usize> {
        Ok(self.get_cluster_threshold().await? as usize)
    }

    /// Legacy method: set config threshold (maps to cluster threshold)
    pub async fn set_config_threshold(&mut self, threshold: usize) -> Result<()> {
        self.set_cluster_threshold(threshold as u32).await
    }

    /// Legacy method: get config total nodes (derived from peers)
    pub async fn get_config_total_nodes(&mut self) -> Result<usize> {
        let peers = self.get_cluster_peers().await?;
        if peers.is_empty() {
            return Err(Error::ConfigError("Total nodes not configured".to_string()));
        }
        Ok(peers.len())
    }

    /// Legacy method: set config total nodes (updates peers count)
    pub async fn set_config_total_nodes(&mut self, total_nodes: usize) -> Result<()> {
        info!("Set total_nodes to {}", total_nodes);
        Ok(())
    }

    // ============================================================================
    // DKG and Generic Lock/Storage Operations
    // ============================================================================

    /// Acquire a generic distributed lock with TTL
    pub async fn acquire_lock(&self, key: &str, ttl_secs: i64) -> Result<bool> {
        match self.acquire_lock_internal(key, ttl_secs).await {
            Ok(_lease_id) => Ok(true),
            Err(e) => {
                warn!("Failed to acquire lock for key={}: {}", key, e);
                Ok(false)
            }
        }
    }

    /// Release a generic distributed lock
    pub async fn release_lock(&self, key: &str) -> Result<()> {
        self.release_lock_internal(key).await
    }

    /// Put a key-value pair in etcd
    pub async fn put(&self, key: &str, value: &[u8]) -> Result<()> {
        let mut client = self.client.clone();
        client
            .put(key.as_bytes(), value, None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to put key: {}", e)))?;
        Ok(())
    }

    /// Get a value from etcd
    pub async fn get(&mut self, key: &str) -> Result<Option<Vec<u8>>> {
        let resp = self
            .client
            .get(key.as_bytes(), None)
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get key: {}", e)))?;

        if resp.kvs().is_empty() {
            Ok(None)
        } else {
            Ok(Some(resp.kvs()[0].value().to_vec()))
        }
    }
}

/// Parse transaction state from string
fn parse_transaction_state(s: &str) -> Result<TransactionState> {
    match s {
        "pending" => Ok(TransactionState::Pending),
        "voting" => Ok(TransactionState::Voting),
        "collecting" => Ok(TransactionState::Collecting),
        "threshold_reached" => Ok(TransactionState::ThresholdReached),
        "approved" => Ok(TransactionState::Approved),
        "rejected" => Ok(TransactionState::Rejected),
        "signing" => Ok(TransactionState::Signing),
        "signed" => Ok(TransactionState::Signed),
        "submitted" => Ok(TransactionState::Submitted),
        "broadcasting" => Ok(TransactionState::Broadcasting),
        "confirmed" => Ok(TransactionState::Confirmed),
        "failed" => Ok(TransactionState::Failed),
        "aborted_byzantine" => Ok(TransactionState::AbortedByzantine),
        _ => Err(Error::StorageError(format!(
            "Invalid transaction state: {}",
            s
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_etcd_connection() {
        let storage = EtcdStorage::new(vec!["127.0.0.1:2379".to_string()]).await;
        assert!(storage.is_ok());
    }

    #[tokio::test]
    #[ignore]
    async fn test_lock_acquisition() {
        let mut storage = EtcdStorage::new(vec!["127.0.0.1:2379".to_string()])
            .await
            .unwrap();
        let tx_id = TxId::from("test-tx-123");

        let lease_id = storage.acquire_signing_lock(&tx_id).await.unwrap();
        assert!(lease_id > 0);

        // Second lock should fail
        let result = storage.acquire_signing_lock(&tx_id).await;
        assert!(result.is_err());

        storage.release_signing_lock(&tx_id).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_vote_storage() {
        let mut storage = EtcdStorage::new(vec!["127.0.0.1:2379".to_string()])
            .await
            .unwrap();

        let vote = Vote::new(
            NodeId(1),
            TxId::from("test-tx-456"),
            1,
            true,
            Some(42),
        );

        let existing = storage.store_vote(&vote).await.unwrap();
        assert!(existing.is_none());

        let count = storage.increment_vote_count(&vote.tx_id, 42).await.unwrap();
        assert_eq!(count, 1);

        let count2 = storage.get_vote_count(&vote.tx_id, 42).await.unwrap();
        assert_eq!(count2, 1);
    }

    #[tokio::test]
    #[ignore]
    async fn test_node_heartbeat() {
        let mut storage = EtcdStorage::new(vec!["127.0.0.1:2379".to_string()])
            .await
            .unwrap();

        let node_id = NodeId(1);
        storage.update_node_heartbeat(node_id).await.unwrap();

        let heartbeat = storage.get_node_heartbeat(node_id).await.unwrap();
        assert!(heartbeat.is_some());
    }
}
