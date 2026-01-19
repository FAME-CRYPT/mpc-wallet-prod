use async_trait::async_trait;
use etcd_client::{Client, Compare, CompareOp, DeleteOptions, PutOptions, Txn, TxnOp, TxnOpResponse};
use serde_json;
use std::collections::HashMap;
use std::time::Duration;
use threshold_types::{
    ByzantineViolation, NodeId, PeerId, Result, TransactionId, TransactionState, Vote, VoteCount,
    VotingError,
};
use tracing::{error, info, warn};

pub struct EtcdStorage {
    client: Client,
}

impl EtcdStorage {
    pub async fn new(endpoints: Vec<String>) -> Result<Self> {
        let client = Client::connect(endpoints, None)
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to connect to etcd: {}", e)))?;

        Ok(Self { client })
    }

    pub async fn get_vote_count(&mut self, tx_id: &TransactionId, value: u64) -> Result<u64> {
        let key = format!("/vote_counts/{}/{}", tx_id, value);

        let resp = self
            .client
            .get(key.as_bytes(), None)
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to get vote count: {}", e)))?;

        if resp.kvs().is_empty() {
            return Ok(0);
        }

        let count_str = String::from_utf8_lossy(resp.kvs()[0].value());
        count_str
            .parse::<u64>()
            .map_err(|e| VotingError::StorageError(format!("Failed to parse count: {}", e)))
    }

    pub async fn increment_vote_count(&mut self, tx_id: &TransactionId, value: u64) -> Result<u64> {
        let key = format!("/vote_counts/{}/{}", tx_id, value);

        let get_resp = self
            .client
            .get(key.as_bytes(), None)
            .await
            .map_err(|e| {
                VotingError::StorageError(format!("Failed to get current count: {}", e))
            })?;

        let current_count = if get_resp.kvs().is_empty() {
            0u64
        } else {
            let count_str = String::from_utf8_lossy(get_resp.kvs()[0].value());
            count_str.parse::<u64>().unwrap_or(0)
        };

        let new_count = current_count + 1;

        self.client
            .put(
                key.as_bytes(),
                new_count.to_string().as_bytes(),
                None,
            )
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to increment count: {}", e)))?;

        info!(
            "Incremented vote count for tx_id={} value={}: {} -> {}",
            tx_id, value, current_count, new_count
        );

        Ok(new_count)
    }

    pub async fn get_all_vote_counts(&mut self, tx_id: &TransactionId) -> Result<HashMap<u64, u64>> {
        let prefix = format!("/vote_counts/{}/", tx_id);

        let resp = self
            .client
            .get(prefix.as_bytes(), Some(etcd_client::GetOptions::new().with_prefix()))
            .await
            .map_err(|e| {
                VotingError::StorageError(format!("Failed to get all vote counts: {}", e))
            })?;

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

    pub async fn store_vote(&mut self, vote: &Vote) -> Result<Option<Vote>> {
        let key = format!("/votes/{}/{}", vote.tx_id, vote.node_id);

        let get_resp = self
            .client
            .get(key.as_bytes(), None)
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to check existing vote: {}", e)))?;

        if !get_resp.kvs().is_empty() {
            let existing_vote_json = String::from_utf8_lossy(get_resp.kvs()[0].value());
            let existing_vote: Vote = serde_json::from_str(&existing_vote_json)
                .map_err(|e| VotingError::StorageError(format!("Failed to parse existing vote: {}", e)))?;
            return Ok(Some(existing_vote));
        }

        let vote_json = serde_json::to_string(vote)
            .map_err(|e| VotingError::StorageError(format!("Failed to serialize vote: {}", e)))?;

        self.client
            .put(key.as_bytes(), vote_json.as_bytes(), None)
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to store vote: {}", e)))?;

        info!("Stored vote for tx_id={} node_id={}", vote.tx_id, vote.node_id);

        Ok(None)
    }

    pub async fn get_transaction_state(&mut self, tx_id: &TransactionId) -> Result<TransactionState> {
        let key = format!("/transaction_status/{}", tx_id);

        let resp = self
            .client
            .get(key.as_bytes(), None)
            .await
            .map_err(|e| {
                VotingError::StorageError(format!("Failed to get transaction state: {}", e))
            })?;

        if resp.kvs().is_empty() {
            return Ok(TransactionState::Collecting);
        }

        let state_str = String::from_utf8_lossy(resp.kvs()[0].value());
        TransactionState::from_str(&state_str).ok_or_else(|| {
            VotingError::StorageError(format!("Invalid transaction state: {}", state_str))
        })
    }

    pub async fn set_transaction_state(
        &mut self,
        tx_id: &TransactionId,
        state: TransactionState,
    ) -> Result<()> {
        let key = format!("/transaction_status/{}", tx_id);

        self.client
            .put(key.as_bytes(), state.to_string().as_bytes(), None)
            .await
            .map_err(|e| {
                VotingError::StorageError(format!("Failed to set transaction state: {}", e))
            })?;

        info!("Set transaction state for tx_id={} to {}", tx_id, state);

        Ok(())
    }

    pub async fn acquire_lock(&mut self, tx_id: &TransactionId, ttl_secs: i64) -> Result<i64> {
        let key = format!("/locks/submission/{}", tx_id);

        let lease_resp = self
            .client
            .lease_grant(ttl_secs, None)
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to create lease: {}", e)))?;

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

        let txn_resp = self
            .client
            .txn(txn)
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to acquire lock: {}", e)))?;

        if !txn_resp.succeeded() {
            return Err(VotingError::StorageError(
                "Failed to acquire lock: already locked".to_string(),
            ));
        }

        info!("Acquired lock for tx_id={} with lease_id={}", tx_id, lease_id);

        Ok(lease_id)
    }

    pub async fn release_lock(&mut self, tx_id: &TransactionId) -> Result<()> {
        let key = format!("/locks/submission/{}", tx_id);

        self.client
            .delete(key.as_bytes(), None)
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to release lock: {}", e)))?;

        info!("Released lock for tx_id={}", tx_id);

        Ok(())
    }

    pub async fn is_node_banned(&mut self, peer_id: &PeerId) -> Result<bool> {
        let key = format!("/banned/{}", peer_id);

        let resp = self
            .client
            .get(key.as_bytes(), None)
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to check banned status: {}", e)))?;

        Ok(!resp.kvs().is_empty())
    }

    pub async fn ban_node(&mut self, violation: &ByzantineViolation) -> Result<()> {
        let key = format!("/banned/{}", violation.peer_id);

        let ban_data = serde_json::to_string(violation)
            .map_err(|e| VotingError::StorageError(format!("Failed to serialize violation: {}", e)))?;

        self.client
            .put(key.as_bytes(), ban_data.as_bytes(), None)
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to ban node: {}", e)))?;

        warn!("Banned node {} for {:?}", violation.peer_id, violation.violation_type);

        Ok(())
    }

    pub async fn get_config_threshold(&mut self) -> Result<usize> {
        let key = b"/config/threshold";

        let resp = self
            .client
            .get(key, None)
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to get threshold: {}", e)))?;

        if resp.kvs().is_empty() {
            return Err(VotingError::ConfigError("Threshold not configured".to_string()));
        }

        let threshold_str = String::from_utf8_lossy(resp.kvs()[0].value());
        threshold_str
            .parse::<usize>()
            .map_err(|e| VotingError::ConfigError(format!("Invalid threshold value: {}", e)))
    }

    pub async fn set_config_threshold(&mut self, threshold: usize) -> Result<()> {
        let key = b"/config/threshold";

        self.client
            .put(key, threshold.to_string().as_bytes(), None)
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to set threshold: {}", e)))?;

        info!("Set threshold to {}", threshold);

        Ok(())
    }

    pub async fn get_config_total_nodes(&mut self) -> Result<usize> {
        let key = b"/config/total_nodes";

        let resp = self
            .client
            .get(key, None)
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to get total_nodes: {}", e)))?;

        if resp.kvs().is_empty() {
            return Err(VotingError::ConfigError("Total nodes not configured".to_string()));
        }

        let total_nodes_str = String::from_utf8_lossy(resp.kvs()[0].value());
        total_nodes_str
            .parse::<usize>()
            .map_err(|e| VotingError::ConfigError(format!("Invalid total_nodes value: {}", e)))
    }

    pub async fn set_config_total_nodes(&mut self, total_nodes: usize) -> Result<()> {
        let key = b"/config/total_nodes";

        self.client
            .put(key, total_nodes.to_string().as_bytes(), None)
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to set total_nodes: {}", e)))?;

        info!("Set total_nodes to {}", total_nodes);

        Ok(())
    }

    // === Garbage Collection Methods ===

    /// Delete all votes for a transaction
    pub async fn delete_all_votes(&mut self, tx_id: &TransactionId) -> Result<()> {
        let prefix = format!("/votes/{}/", tx_id);

        self.client
            .delete(prefix.as_bytes(), Some(DeleteOptions::new().with_prefix()))
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to delete votes: {}", e)))?;

        Ok(())
    }

    /// Delete all vote counts for a transaction
    pub async fn delete_all_vote_counts(&mut self, tx_id: &TransactionId) -> Result<()> {
        let prefix = format!("/vote_counts/{}/", tx_id);

        self.client
            .delete(prefix.as_bytes(), Some(DeleteOptions::new().with_prefix()))
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to delete vote counts: {}", e)))?;

        Ok(())
    }

    /// Delete transaction state
    pub async fn delete_transaction_state(&mut self, tx_id: &TransactionId) -> Result<()> {
        let key = format!("/transaction_status/{}", tx_id);

        self.client
            .delete(key.as_bytes(), None)
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to delete transaction state: {}", e)))?;

        Ok(())
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
}
