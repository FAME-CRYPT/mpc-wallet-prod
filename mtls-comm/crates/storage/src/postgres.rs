use async_trait::async_trait;
use deadpool_postgres::{Config, Manager, ManagerConfig, Pool, RecyclingMethod, Runtime};
use serde_json;
use threshold_types::{
    ByzantineViolation, ByzantineViolationType, NodeId, PeerId, Result, TransactionId,
    TransactionState, Vote, VotingError,
};
use tokio_postgres::{NoTls, Row};
use tracing::{error, info};

pub struct PostgresStorage {
    pool: Pool,
}

impl PostgresStorage {
    pub async fn new(connection_string: &str) -> Result<Self> {
        let config: tokio_postgres::Config = connection_string
            .parse()
            .map_err(|e| VotingError::StorageError(format!("Invalid connection string: {}", e)))?;

        let mut cfg = Config::new();
        cfg.host = config.get_hosts().first().and_then(|h| match h {
            tokio_postgres::config::Host::Tcp(s) => Some(s.clone()),
            _ => None,
        });
        cfg.port = config.get_ports().first().copied();
        cfg.dbname = config.get_dbname().map(|s| s.to_string());
        cfg.user = config.get_user().map(|s| s.to_string());
        cfg.password = config.get_password().map(|p| String::from_utf8_lossy(p).to_string());

        cfg.manager = Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        });

        let pool = cfg
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .map_err(|e| VotingError::StorageError(format!("Failed to create pool: {}", e)))?;

        let storage = Self { pool };

        storage.init_schema().await?;

        Ok(storage)
    }

    async fn init_schema(&self) -> Result<()> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to get client: {}", e)))?;

        client
            .batch_execute(
                r#"
                CREATE TABLE IF NOT EXISTS blockchain_submissions (
                    tx_id TEXT PRIMARY KEY,
                    value BIGINT NOT NULL,
                    state TEXT NOT NULL CHECK (state IN
                        ('PENDING', 'CONFIRMED', 'ABORTED_BYZANTINE')),
                    nonce BIGINT NOT NULL,
                    tx_hash BYTEA,
                    submitted_at TIMESTAMP,
                    confirmed_at TIMESTAMP,
                    block_number BIGINT,
                    UNIQUE (nonce)
                );

                CREATE TABLE IF NOT EXISTS byzantine_violations (
                    id SERIAL PRIMARY KEY,
                    peer_id TEXT NOT NULL,
                    node_id TEXT,
                    tx_id TEXT NOT NULL,
                    violation_type TEXT NOT NULL CHECK (violation_type IN
                        ('DOUBLE_VOTING', 'MINORITY_VOTE', 'INVALID_SIGNATURE', 'SILENT_FAILURE')),
                    evidence_json JSONB NOT NULL,
                    detected_at TIMESTAMP NOT NULL DEFAULT NOW()
                );

                CREATE TABLE IF NOT EXISTS vote_history (
                    tx_id TEXT NOT NULL,
                    node_id TEXT NOT NULL,
                    peer_id TEXT NOT NULL,
                    value BIGINT NOT NULL,
                    signature BYTEA NOT NULL,
                    received_at TIMESTAMP NOT NULL DEFAULT NOW(),
                    PRIMARY KEY (tx_id, node_id)
                );

                CREATE TABLE IF NOT EXISTS node_reputation (
                    peer_id TEXT PRIMARY KEY,
                    reputation_score DOUBLE PRECISION NOT NULL
                        DEFAULT 1.0 CHECK (reputation_score BETWEEN 0.0 AND 1.0),
                    total_votes BIGINT NOT NULL DEFAULT 0,
                    violations_count INT NOT NULL DEFAULT 0,
                    last_seen TIMESTAMP NOT NULL DEFAULT NOW()
                );

                CREATE INDEX IF NOT EXISTS idx_byzantine_violations_peer_id
                    ON byzantine_violations(peer_id);
                CREATE INDEX IF NOT EXISTS idx_byzantine_violations_tx_id
                    ON byzantine_violations(tx_id);
                CREATE INDEX IF NOT EXISTS idx_vote_history_tx_id
                    ON vote_history(tx_id);
                "#,
            )
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to init schema: {}", e)))?;

        info!("PostgreSQL schema initialized");

        Ok(())
    }

    pub async fn record_vote(&self, vote: &Vote) -> Result<()> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to get client: {}", e)))?;

        client
            .execute(
                r#"
                INSERT INTO vote_history (tx_id, node_id, peer_id, value, signature)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT (tx_id, node_id) DO NOTHING
                "#,
                &[
                    &vote.tx_id.0,
                    &(vote.node_id.0 as i64),
                    &vote.peer_id.0,
                    &(vote.value as i64),
                    &vote.signature,
                ],
            )
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to record vote: {}", e)))?;

        Ok(())
    }

    pub async fn record_byzantine_violation(&self, violation: &ByzantineViolation) -> Result<()> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to get client: {}", e)))?;

        client
            .execute(
                r#"
                INSERT INTO byzantine_violations (peer_id, node_id, tx_id, violation_type, evidence_json)
                VALUES ($1, $2, $3, $4, $5)
                "#,
                &[
                    &violation.peer_id.0,
                    &violation.node_id.as_ref().map(|n| n.0 as i64),
                    &violation.tx_id.0,
                    &violation.violation_type.to_string(),
                    &violation.evidence,
                ],
            )
            .await
            .map_err(|e| {
                VotingError::StorageError(format!("Failed to record Byzantine violation: {}", e))
            })?;

        self.increment_node_violations(&violation.peer_id).await?;

        info!(
            "Recorded Byzantine violation: peer_id={} type={:?}",
            violation.peer_id, violation.violation_type
        );

        Ok(())
    }

    async fn increment_node_violations(&self, peer_id: &PeerId) -> Result<()> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to get client: {}", e)))?;

        client
            .execute(
                r#"
                INSERT INTO node_reputation (peer_id, violations_count, total_votes)
                VALUES ($1, 1, 0)
                ON CONFLICT (peer_id) DO UPDATE
                SET violations_count = node_reputation.violations_count + 1,
                    reputation_score = GREATEST(0.0, node_reputation.reputation_score - 0.1),
                    last_seen = NOW()
                "#,
                &[&peer_id.0],
            )
            .await
            .map_err(|e| {
                VotingError::StorageError(format!("Failed to increment violations: {}", e))
            })?;

        Ok(())
    }

    pub async fn get_node_violations(&self, peer_id: &PeerId) -> Result<Vec<ByzantineViolation>> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to get client: {}", e)))?;

        let rows = client
            .query(
                r#"
                SELECT peer_id, node_id, tx_id, violation_type, evidence_json, detected_at
                FROM byzantine_violations
                WHERE peer_id = $1
                ORDER BY detected_at DESC
                "#,
                &[&peer_id.0],
            )
            .await
            .map_err(|e| {
                VotingError::StorageError(format!("Failed to get violations: {}", e))
            })?;

        let violations = rows
            .into_iter()
            .filter_map(|row| {
                let peer_id = PeerId::from(row.get::<_, String>(0));
                let node_id: Option<String> = row.get(1);
                let tx_id = TransactionId::from(row.get::<_, String>(2));
                let violation_type_str: String = row.get(3);
                let evidence: serde_json::Value = row.get(4);
                let detected_at = row.get(5);

                let violation_type = match violation_type_str.as_str() {
                    "DOUBLE_VOTING" => ByzantineViolationType::DoubleVoting,
                    "MINORITY_VOTE" => ByzantineViolationType::MinorityVote,
                    "INVALID_SIGNATURE" => ByzantineViolationType::InvalidSignature,
                    "SILENT_FAILURE" => ByzantineViolationType::SilentFailure,
                    _ => return None,
                };

                Some(ByzantineViolation {
                    peer_id,
                    node_id: node_id.map(NodeId::from),
                    tx_id,
                    violation_type,
                    evidence,
                    detected_at,
                })
            })
            .collect();

        Ok(violations)
    }

    pub async fn update_node_last_seen(&self, peer_id: &PeerId) -> Result<()> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to get client: {}", e)))?;

        client
            .execute(
                r#"
                INSERT INTO node_reputation (peer_id, total_votes)
                VALUES ($1, 1)
                ON CONFLICT (peer_id) DO UPDATE
                SET total_votes = node_reputation.total_votes + 1,
                    last_seen = NOW()
                "#,
                &[&peer_id.0],
            )
            .await
            .map_err(|e| {
                VotingError::StorageError(format!("Failed to update last seen: {}", e))
            })?;

        Ok(())
    }

    pub async fn get_reputation_score(&self, peer_id: &PeerId) -> Result<f64> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to get client: {}", e)))?;

        let row = client
            .query_opt(
                r#"
                SELECT reputation_score FROM node_reputation WHERE peer_id = $1
                "#,
                &[&peer_id.0],
            )
            .await
            .map_err(|e| {
                VotingError::StorageError(format!("Failed to get reputation: {}", e))
            })?;

        Ok(row.map(|r| r.get(0)).unwrap_or(1.0))
    }

    // === Garbage Collection Methods ===

    /// Get confirmed transactions before a cutoff time
    pub async fn get_confirmed_transactions_before(
        &self,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<TransactionId>> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to get client: {}", e)))?;

        let rows = client
            .query(
                r#"
                SELECT tx_id FROM blockchain_submissions
                WHERE state = 'CONFIRMED' AND confirmed_at < $1
                "#,
                &[&cutoff],
            )
            .await
            .map_err(|e| {
                VotingError::StorageError(format!("Failed to get confirmed transactions: {}", e))
            })?;

        Ok(rows.into_iter().map(|r| TransactionId::from(r.get::<_, String>(0))).collect())
    }

    /// Archive old submissions to archive table
    pub async fn archive_old_submissions(
        &self,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> Result<u64> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to get client: {}", e)))?;

        let result = client
            .execute(
                r#"
                WITH moved AS (
                    DELETE FROM blockchain_submissions
                    WHERE state = 'CONFIRMED' AND confirmed_at < $1
                    RETURNING *
                )
                INSERT INTO blockchain_submissions_archive SELECT * FROM moved
                "#,
                &[&cutoff],
            )
            .await
            .map_err(|e| {
                VotingError::StorageError(format!("Failed to archive submissions: {}", e))
            })?;

        Ok(result)
    }

    /// Delete old vote history
    pub async fn delete_old_vote_history(
        &self,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> Result<u64> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| VotingError::StorageError(format!("Failed to get client: {}", e)))?;

        let result = client
            .execute(
                r#"
                DELETE FROM vote_history
                WHERE received_at < $1
                "#,
                &[&cutoff],
            )
            .await
            .map_err(|e| {
                VotingError::StorageError(format!("Failed to delete vote history: {}", e))
            })?;

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_postgres_connection() {
        let storage = PostgresStorage::new("postgresql://localhost/threshold_voting").await;
        assert!(storage.is_ok());
    }
}
