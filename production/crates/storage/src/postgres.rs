use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
use threshold_types::*;
use chrono::Utc;
use tokio_postgres::NoTls;
use tracing::info;

pub struct PostgresStorage {
    pool: Pool,
}

impl PostgresStorage {
    pub async fn new(config: &PostgresConfig) -> Result<Self> {
        let pg_config: tokio_postgres::Config = config
            .url
            .parse()
            .map_err(|e| Error::StorageError(format!("Invalid connection string: {}", e)))?;

        let mut cfg = Config::new();
        cfg.host = pg_config.get_hosts().first().and_then(|h| match h {
            tokio_postgres::config::Host::Tcp(s) => Some(s.clone()),
            tokio_postgres::config::Host::Unix(_) => None, // Unix sockets not supported for deadpool
        });
        cfg.port = pg_config.get_ports().first().copied();
        cfg.dbname = pg_config.get_dbname().map(|s| s.to_string());
        cfg.user = pg_config.get_user().map(|s| s.to_string());
        cfg.password = pg_config
            .get_password()
            .map(|p| String::from_utf8_lossy(p).to_string());

        cfg.manager = Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        });

        let pool = cfg
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .map_err(|e| Error::StorageError(format!("Failed to create pool: {}", e)))?;

        let storage = Self { pool };

        info!("PostgreSQL storage initialized successfully");

        Ok(storage)
    }

    /// Record a vote in the database
    pub async fn record_vote(&self, vote: &Vote) -> Result<()> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        client
            .execute(
                r#"
                INSERT INTO votes (round_id, node_id, tx_id, approve, value, signature)
                VALUES (
                    (SELECT id FROM voting_rounds WHERE tx_id = $1 AND round_number = $2),
                    $3, $4, $5, $6, $7
                )
                ON CONFLICT (round_id, node_id) DO NOTHING
                "#,
                &[
                    &vote.tx_id.0,
                    &(vote.round_id as i32),
                    &(vote.node_id.0 as i64),
                    &vote.tx_id.0,
                    &vote.approve,
                    &(vote.value as i64),
                    &vote.signature,
                ],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to record vote: {}", e)))?;

        Ok(())
    }

    /// Record a Byzantine violation
    pub async fn record_byzantine_violation(&self, violation: &ByzantineViolation) -> Result<()> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        let node_id_val = violation.node_id.map(|n| n.0 as i64);

        client
            .execute(
                r#"
                INSERT INTO byzantine_violations (node_id, violation_type, round_id, tx_id, evidence)
                VALUES (
                    $1,
                    $2,
                    0,
                    $3,
                    $4
                )
                "#,
                &[
                    &node_id_val,
                    &violation.violation_type.to_string(),
                    &violation.tx_id.0,
                    &violation.evidence,
                ],
            )
            .await
            .map_err(|e| {
                Error::StorageError(format!("Failed to record Byzantine violation: {}", e))
            })?;

        info!(
            "Recorded Byzantine violation: peer_id={} type={}",
            violation.peer_id, violation.violation_type
        );

        Ok(())
    }

    /// Update node last seen timestamp
    pub async fn update_node_last_seen(&self, _peer_id: &PeerId) -> Result<()> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        let timestamp = Utc::now();

        client
            .execute(
                r#"
                INSERT INTO node_status (node_id, status, last_heartbeat)
                VALUES (0, 'active', $1)
                ON CONFLICT (node_id) DO UPDATE
                SET last_heartbeat = $1, updated_at = NOW()
                "#,
                &[&timestamp],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to update last seen: {}", e)))?;

        Ok(())
    }

    /// Get Byzantine violations for a specific node
    pub async fn get_node_violations(&self, node_id: NodeId) -> Result<Vec<ByzantineViolation>> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        let rows = client
            .query(
                r#"
                SELECT id, node_id, violation_type, round_id, evidence, detected_at
                FROM byzantine_violations
                WHERE node_id = $1
                ORDER BY detected_at DESC
                "#,
                &[&(node_id.0 as i64)],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get violations: {}", e)))?;

        let violations = rows
            .into_iter()
            .map(|row| {
                let id: i64 = row.get(0);
                let node_id_raw: i64 = row.get(1);
                let violation_type_str: String = row.get(2);
                let _round_id: i64 = row.get(3);
                let evidence: serde_json::Value = row.get(4);
                let detected_at = row.get(5);

                let violation_type = match violation_type_str.as_str() {
                    "double_vote" => ViolationType::DoubleVote,
                    "invalid_signature" => ViolationType::InvalidSignature,
                    "timeout" => ViolationType::Timeout,
                    "malformed_message" => ViolationType::MalformedMessage,
                    "minority_vote" => ViolationType::MinorityVote,
                    _ => ViolationType::MalformedMessage,
                };

                ByzantineViolation {
                    id: Some(id),
                    peer_id: PeerId::from(format!("peer-{}", node_id_raw)),
                    node_id: Some(NodeId(node_id_raw as u64)),
                    tx_id: TxId::from("unknown"),
                    violation_type,
                    evidence,
                    detected_at,
                }
            })
            .collect();

        Ok(violations)
    }

    /// Create a new transaction
    pub async fn create_transaction(&self, tx: &Transaction) -> Result<i64> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        let state_str = tx.state.to_string();

        info!("Inserting transaction: txid={}, state={}, unsigned_tx_len={}, recipient={}, amount={}, fee={}, metadata={:?}",
            tx.txid.0, state_str, tx.unsigned_tx.len(), tx.recipient, tx.amount_sats, tx.fee_sats, tx.metadata);

        let row = client
            .query_one(
                r#"
                INSERT INTO transactions (txid, state, unsigned_tx, recipient, amount_sats, fee_sats, metadata)
                VALUES ($1, CAST($2 AS transaction_state), $3, $4, $5, $6, $7)
                RETURNING id
                "#,
                &[
                    &tx.txid.0,
                    &state_str,
                    &tx.unsigned_tx[..],
                    &tx.recipient,
                    &(tx.amount_sats as i64),
                    &(tx.fee_sats as i64),
                    &tx.metadata,
                ],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to create transaction: {}", e)))?;

        let id: i64 = row.get(0);
        info!("Created transaction: id={} txid={}", id, tx.txid);

        Ok(id)
    }

    /// Update transaction state
    pub async fn update_transaction_state(
        &self,
        txid: &TxId,
        new_state: TransactionState,
    ) -> Result<()> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        let state_str = new_state.to_string();
        client
            .execute(
                r#"
                UPDATE transactions
                SET state = CAST($1 AS transaction_state), updated_at = NOW()
                WHERE txid = $2
                "#,
                &[&state_str, &txid.0],
            )
            .await
            .map_err(|e| {
                Error::StorageError(format!("Failed to update transaction state: {}", e))
            })?;

        Ok(())
    }

    /// Set signed transaction
    pub async fn set_signed_transaction(&self, txid: &TxId, signed_tx: &[u8]) -> Result<()> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        client
            .execute(
                r#"
                UPDATE transactions
                SET signed_tx = $1, state = 'signed', updated_at = NOW()
                WHERE txid = $2
                "#,
                &[&signed_tx, &txid.0],
            )
            .await
            .map_err(|e| {
                Error::StorageError(format!("Failed to set signed transaction: {}", e))
            })?;

        info!("Updated signed transaction: txid={}", txid);

        Ok(())
    }

    /// Get transaction by txid
    pub async fn get_transaction(&self, txid: &TxId) -> Result<Option<Transaction>> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        let row = client
            .query_opt(
                r#"
                SELECT id, txid, state, unsigned_tx, signed_tx, recipient,
                       amount_sats, fee_sats, metadata, created_at, updated_at
                FROM transactions
                WHERE txid = $1
                "#,
                &[&txid.0],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get transaction: {}", e)))?;

        Ok(row.map(|r| Transaction {
            id: r.get(0),
            txid: TxId(r.get(1)),
            state: parse_transaction_state(r.get(2)),
            unsigned_tx: r.get(3),
            signed_tx: r.get(4),
            recipient: r.get(5),
            amount_sats: r.get::<_, i64>(6) as u64,
            fee_sats: r.get::<_, i64>(7) as u64,
            metadata: r.get(8),
            created_at: r.get(9),
            updated_at: r.get(10),
        }))
    }

    /// Create a voting round
    pub async fn create_voting_round(&self, round: &VotingRound) -> Result<i64> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        let row = client
            .query_one(
                r#"
                INSERT INTO voting_rounds (tx_id, round_number, total_nodes, threshold, timeout_at)
                VALUES ($1, $2, $3, $4, NOW() + INTERVAL '5 minutes')
                RETURNING id
                "#,
                &[
                    &round.tx_id.0,
                    &(round.round_number as i32),
                    &(round.total_nodes as i32),
                    &(round.threshold as i32),
                ],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to create voting round: {}", e)))?;

        let id: i64 = row.get(0);
        info!("Created voting round: id={} tx_id={}", id, round.tx_id);

        Ok(id)
    }

    /// Update voting round status
    pub async fn update_voting_round(
        &self,
        round_id: i64,
        votes_received: u32,
        approved: bool,
        completed: bool,
    ) -> Result<()> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        client
            .execute(
                r#"
                UPDATE voting_rounds
                SET votes_received = $1, approved = $2, completed = $3,
                    completed_at = CASE WHEN $3 THEN NOW() ELSE NULL END
                WHERE id = $4
                "#,
                &[
                    &(votes_received as i32),
                    &approved,
                    &completed,
                    &round_id,
                ],
            )
            .await
            .map_err(|e| {
                Error::StorageError(format!("Failed to update voting round: {}", e))
            })?;

        Ok(())
    }

    /// Record presignature usage
    pub async fn record_presignature_usage(&self, usage: &PresignatureUsage) -> Result<()> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        let presig_id_str = usage.presig_id.0.to_string();

        client
            .execute(
                r#"
                INSERT INTO presignature_usage (presig_id, transaction_id, generation_time_ms, protocol)
                VALUES ($1, $2, $3, 'cggmp24')
                "#,
                &[
                    &presig_id_str,
                    &usage.transaction_id,
                    &usage.generation_time_ms,
                ],
            )
            .await
            .map_err(|e| {
                Error::StorageError(format!("Failed to record presignature usage: {}", e))
            })?;

        Ok(())
    }

    /// Update node status
    pub async fn update_node_status(
        &self,
        node_id: NodeId,
        status: &str,
        last_heartbeat: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        client
            .execute(
                r#"
                INSERT INTO node_status (node_id, status, last_heartbeat)
                VALUES ($1, $2, $3)
                ON CONFLICT (node_id) DO UPDATE
                SET status = $2, last_heartbeat = $3, updated_at = NOW()
                "#,
                &[&(node_id.0 as i64), &status, &last_heartbeat],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to update node status: {}", e)))?;

        Ok(())
    }

    /// Log audit event
    pub async fn log_audit_event(
        &self,
        event_type: &str,
        node_id: Option<NodeId>,
        tx_id: Option<&TxId>,
        details: serde_json::Value,
    ) -> Result<()> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        client
            .execute(
                r#"
                INSERT INTO audit_log (event_type, node_id, tx_id, details)
                VALUES ($1, $2, $3, $4)
                "#,
                &[
                    &event_type,
                    &node_id.map(|n| n.0 as i64),
                    &tx_id.map(|t| t.0.as_str()),
                    &details,
                ],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to log audit event: {}", e)))?;

        Ok(())
    }

    /// Get node health status
    pub async fn get_node_health(&self, node_id: NodeId) -> Result<Option<serde_json::Value>> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        let row = client
            .query_opt(
                r#"
                SELECT status, last_heartbeat, total_votes, total_violations, banned_until,
                       EXTRACT(EPOCH FROM (NOW() - last_heartbeat)) as seconds_since_heartbeat
                FROM node_status
                WHERE node_id = $1
                "#,
                &[&(node_id.0 as i64)],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get node health: {}", e)))?;

        Ok(row.map(|r| {
            serde_json::json!({
                "status": r.get::<_, String>(0),
                "last_heartbeat": r.get::<_, chrono::DateTime<chrono::Utc>>(1),
                "total_votes": r.get::<_, i64>(2),
                "total_violations": r.get::<_, i64>(3),
                "banned_until": r.get::<_, Option<chrono::DateTime<chrono::Utc>>>(4),
                "seconds_since_heartbeat": r.get::<_, f64>(5),
            })
        }))
    }
}

fn parse_transaction_state(s: String) -> TransactionState {
    match s.as_str() {
        "pending" => TransactionState::Pending,
        "voting" => TransactionState::Voting,
        "collecting" => TransactionState::Collecting,
        "threshold_reached" => TransactionState::ThresholdReached,
        "approved" => TransactionState::Approved,
        "rejected" => TransactionState::Rejected,
        "signing" => TransactionState::Signing,
        "signed" => TransactionState::Signed,
        "submitted" => TransactionState::Submitted,
        "broadcasting" => TransactionState::Broadcasting,
        "confirmed" => TransactionState::Confirmed,
        "failed" => TransactionState::Failed,
        "aborted_byzantine" => TransactionState::AbortedByzantine,
        _ => TransactionState::Failed,
    }
}
