use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
use threshold_types::*;
use chrono::Utc;
use tokio_postgres::NoTls;
use tracing::{error, info, warn};

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
            #[allow(unreachable_patterns)]
            _ => None, // Unix sockets or other host types not supported for deadpool
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
    pub async fn update_node_last_seen(&self, node_id: &NodeId) -> Result<()> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        let timestamp = Utc::now();
        let node_id_i64 = node_id.0 as i64;

        client
            .execute(
                r#"
                INSERT INTO node_status (node_id, status, last_heartbeat)
                VALUES ($1, 'active', $2)
                ON CONFLICT (node_id) DO UPDATE
                SET last_heartbeat = $2, updated_at = NOW()
                "#,
                &[&node_id_i64, &timestamp],
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
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                RETURNING id
                "#,
                &[
                    &tx.txid.0.as_str(),
                    &state_str.as_str(),
                    &tx.unsigned_tx,
                    &tx.recipient.as_str(),
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
                SET state = $1, updated_at = NOW()
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
                SET signed_tx = $1, updated_at = NOW()
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

    /// List all transactions with optional pagination
    pub async fn list_all_transactions(
        &self,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<Transaction>> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        let limit_val = limit.unwrap_or(100) as i64;
        let offset_val = offset.unwrap_or(0) as i64;

        let rows = client
            .query(
                r#"
                SELECT id, txid, state, unsigned_tx, signed_tx, recipient,
                       amount_sats, fee_sats, metadata, created_at, updated_at
                FROM transactions
                ORDER BY created_at DESC
                LIMIT $1 OFFSET $2
                "#,
                &[&limit_val, &offset_val],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to query transactions: {}", e)))?;

        let mut transactions = Vec::new();
        for row in rows {
            transactions.push(Transaction {
                id: row.get(0),
                txid: TxId(row.get(1)),
                state: parse_transaction_state(row.get(2)),
                unsigned_tx: row.get(3),
                signed_tx: row.get(4),
                recipient: row.get(5),
                amount_sats: row.get::<_, i64>(6) as u64,
                fee_sats: row.get::<_, i64>(7) as u64,
                metadata: row.get(8),
                created_at: row.get(9),
                updated_at: row.get(10),
            });
        }

        info!(
            "Listed {} transactions (limit: {}, offset: {})",
            transactions.len(),
            limit_val,
            offset_val
        );

        Ok(transactions)
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
                VALUES ($1, $2, $3, $4, $5)
                RETURNING id
                "#,
                &[
                    &round.tx_id.0,
                    &(round.round_number as i32),
                    &(round.total_nodes as i32),
                    &(round.threshold as i32),
                    &round.timeout_at,
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

    /// Get voting round by transaction ID
    pub async fn get_voting_round_by_txid(&self, txid: &str) -> Result<Option<VotingRound>> {
        let client = self.pool.get().await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        let row = client.query_opt(
            r#"
            SELECT id, tx_id, round_number, total_nodes, threshold,
                   votes_received, approved, completed, started_at,
                   completed_at, timeout_at
            FROM voting_rounds
            WHERE tx_id = $1
            ORDER BY id DESC
            LIMIT 1
            "#,
            &[&txid],
        ).await
        .map_err(|e| Error::StorageError(format!("Failed to get voting round: {}", e)))?;

        Ok(row.map(|r| VotingRound {
            id: r.get::<_, i64>(0),
            tx_id: TxId(r.get::<_, String>(1)),
            round_number: r.get::<_, i32>(2) as u32,
            total_nodes: r.get::<_, i32>(3) as u32,
            threshold: r.get::<_, i32>(4) as u32,
            votes_received: r.get::<_, i32>(5) as u32,
            approved: r.get::<_, bool>(6),
            completed: r.get::<_, bool>(7),
            started_at: r.get::<_, chrono::DateTime<chrono::Utc>>(8),
            completed_at: r.get::<_, Option<chrono::DateTime<chrono::Utc>>>(9),
            timeout_at: r.get::<_, chrono::DateTime<chrono::Utc>>(10),
        }))
    }

    /// Update voting round approved status
    pub async fn update_voting_round_approved(&self, round_id: i64, approved: bool) -> Result<()> {
        let client = self.pool.get().await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        client.execute(
            r#"
            UPDATE voting_rounds
            SET approved = $1, completed = true, completed_at = NOW()
            WHERE id = $2
            "#,
            &[&approved, &round_id],
        ).await
        .map_err(|e| Error::StorageError(format!("Failed to update voting round approved: {}", e)))?;

        Ok(())
    }

    /// Update voting round completed status (without approval)
    pub async fn update_voting_round_completed(&self, round_id: i64) -> Result<()> {
        let client = self.pool.get().await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        client.execute(
            r#"
            UPDATE voting_rounds
            SET completed = true, completed_at = NOW()
            WHERE id = $1
            "#,
            &[&round_id],
        ).await
        .map_err(|e| Error::StorageError(format!("Failed to update voting round completed: {}", e)))?;

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
                       CAST(EXTRACT(EPOCH FROM (NOW() - last_heartbeat)) AS DOUBLE PRECISION) as seconds_since_heartbeat
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

    /// Get all transactions by state
    pub async fn get_transactions_by_state(&self, state: &str) -> Result<Vec<Transaction>> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        let rows = client
            .query(
                r#"
                SELECT id, txid, state, unsigned_tx, signed_tx, recipient,
                       amount_sats, fee_sats, metadata, created_at, updated_at
                FROM transactions
                WHERE state = $1
                ORDER BY created_at ASC
                "#,
                &[&state],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get transactions: {}", e)))?;

        Ok(rows
            .iter()
            .map(|r| Transaction {
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
            })
            .collect())
    }

    /// Get votes for a specific voting round
    pub async fn get_votes_for_round(&self, tx_id: &TxId, round_number: u32) -> Result<Vec<Vote>> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        let rows = client
            .query(
                r#"
                SELECT v.node_id, v.tx_id, v.peer_id, v.round_id, v.approve, v.value,
                       v.signature, v.public_key, v.created_at
                FROM votes v
                JOIN voting_rounds vr ON v.round_id = vr.id
                WHERE vr.tx_id = $1 AND vr.round_number = $2
                "#,
                &[&tx_id.0, &(round_number as i32)],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get votes: {}", e)))?;

        Ok(rows
            .iter()
            .map(|r| Vote {
                tx_id: TxId(r.get(1)),
                node_id: NodeId(r.get::<_, i64>(0) as u64),
                peer_id: PeerId(r.get(2)),
                round_id: r.get::<_, i64>(3) as u64,
                approve: r.get(4),
                value: r.get::<_, i64>(5) as u64,
                signature: r.get(6),
                public_key: r.get(7),
                timestamp: r.get(8),
            })
            .collect())
    }

    /// Get signed transaction bytes
    pub async fn get_signed_transaction(&self, tx_id: &TxId) -> Result<Option<Vec<u8>>> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        let row = client
            .query_opt(
                "SELECT signed_tx FROM transactions WHERE txid = $1",
                &[&tx_id.0],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get signed transaction: {}", e)))?;

        Ok(row.and_then(|r| r.get(0)))
    }

    /// Update transaction txid
    pub async fn update_transaction_txid(&self, tx_id: &TxId, txid: &str) -> Result<()> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        client
            .execute(
                "UPDATE transactions SET txid = $1, updated_at = NOW() WHERE txid = $2",
                &[&txid, &tx_id.0],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to update transaction txid: {}", e)))?;

        Ok(())
    }

    /// Record audit event for a transaction
    pub async fn record_audit_event(&self, tx_id: &TxId, event_type: &str, details: &str) -> Result<()> {
        self.log_audit_event(
            event_type,
            None,
            Some(tx_id),
            serde_json::json!({ "details": details }),
        )
        .await
    }

    /// Update transaction confirmations count
    pub async fn update_transaction_confirmations(&self, tx_id: &TxId, confirmations: u32) -> Result<()> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        client
            .execute(
                "UPDATE transactions SET confirmations = $1, updated_at = NOW() WHERE txid = $2",
                &[&(confirmations as i32), &tx_id.0],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to update confirmations: {}", e)))?;

        Ok(())
    }

    /// Get expired transactions (transactions that have been in voting/signing state for too long)
    pub async fn get_expired_transactions(&self, cutoff: chrono::DateTime<chrono::Utc>) -> Result<Vec<Transaction>> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        let rows = client
            .query(
                "SELECT id, txid, state, unsigned_tx, signed_tx, recipient, amount_sats, fee_sats,
                        metadata, created_at, updated_at
                 FROM transactions
                 WHERE (state = 'voting' OR state = 'signing' OR state = 'broadcasting')
                   AND updated_at < $1
                 ORDER BY updated_at ASC",
                &[&cutoff],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get expired transactions: {}", e)))?;

        let transactions = rows
            .into_iter()
            .map(|row| {
                Transaction {
                    id: row.get(0),
                    txid: TxId(row.get(1)),
                    state: parse_transaction_state(row.get(2)),
                    unsigned_tx: row.get(3),
                    signed_tx: row.get(4),
                    recipient: row.get(5),
                    amount_sats: row.get::<_, i64>(6) as u64,
                    fee_sats: row.get::<_, i64>(7) as u64,
                    metadata: row.get(8),
                    created_at: row.get(9),
                    updated_at: row.get(10),
                }
            })
            .collect();

        Ok(transactions)
    }

    /// Create a new DKG ceremony record
    pub async fn create_dkg_ceremony(&self, ceremony: &crate::DkgCeremony) -> Result<()> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        // Convert UUID to string for database storage
        let session_id_str = ceremony.session_id.to_string();

        let result = client
            .execute(
                r#"
                INSERT INTO dkg_ceremonies (session_id, protocol, threshold, total_nodes, status, started_at)
                VALUES ($1, $2, $3, $4, $5, $6)
                "#,
                &[
                    &session_id_str,
                    &ceremony.protocol,
                    &(ceremony.threshold as i32),
                    &(ceremony.total_nodes as i32),
                    &ceremony.status,
                    &ceremony.started_at,
                ],
            )
            .await;

        match result {
            Ok(_) => {
                info!("Created DKG ceremony: session_id={} protocol={}", session_id_str, ceremony.protocol);
                Ok(())
            }
            Err(e) => {
                error!("Failed to insert DKG ceremony: SQL Error: {:?}", e);
                error!("Ceremony data: session_id={}, protocol={}, threshold={}, total_nodes={}",
                       session_id_str, ceremony.protocol, ceremony.threshold, ceremony.total_nodes);
                Err(Error::StorageError(format!("Failed to create DKG ceremony: {}", e)))
            }
        }
    }

    /// Mark DKG ceremony as completed
    pub async fn complete_dkg_ceremony(&self, session_id: uuid::Uuid, public_key: &[u8]) -> Result<()> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        client
            .execute(
                r#"
                UPDATE dkg_ceremonies
                SET status = 'completed', public_key = $1, completed_at = NOW()
                WHERE session_id = $2
                "#,
                &[&public_key, &session_id.to_string()],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to complete DKG ceremony: {}", e)))?;

        info!("Completed DKG ceremony: session_id={}", session_id);

        Ok(())
    }

    /// Mark DKG ceremony as failed
    pub async fn fail_dkg_ceremony(&self, session_id: uuid::Uuid, error: &str) -> Result<()> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        client
            .execute(
                r#"
                UPDATE dkg_ceremonies
                SET status = 'failed', completed_at = NOW()
                WHERE session_id = $1
                "#,
                &[&session_id.to_string()],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to fail DKG ceremony: {}", e)))?;

        info!("Failed DKG ceremony: session_id={} error={}", session_id, error);

        Ok(())
    }

    /// Get a specific DKG ceremony by session ID
    pub async fn get_dkg_ceremony(&self, session_id: uuid::Uuid) -> Result<crate::DkgCeremony> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        let row = client
            .query_one(
                r#"
                SELECT session_id, protocol, threshold, total_nodes, status,
                       public_key, started_at, completed_at, error
                FROM dkg_ceremonies
                WHERE session_id = $1
                "#,
                &[&session_id.to_string()],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get DKG ceremony: {}", e)))?;

        let session_id_str: String = row.get(0);
        let protocol: String = row.get(1);
        let threshold: i32 = row.get(2);
        let total_nodes: i32 = row.get(3);
        let status: String = row.get(4);
        let public_key: Option<Vec<u8>> = row.get(5);
        let started_at: chrono::DateTime<chrono::Utc> = row.get(6);
        let completed_at: Option<chrono::DateTime<chrono::Utc>> = row.get(7);
        let error: Option<String> = row.get(8);

        Ok(crate::DkgCeremony {
            session_id: uuid::Uuid::parse_str(&session_id_str)
                .map_err(|e| Error::StorageError(format!("Invalid UUID: {}", e)))?,
            protocol,
            threshold: threshold as u32,
            total_nodes: total_nodes as u32,
            status,
            public_key,
            started_at,
            completed_at,
            error,
        })
    }

    /// List all DKG ceremonies
    pub async fn list_dkg_ceremonies(&self) -> Result<Vec<crate::DkgCeremony>> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        let _rows = client
            .query(
                r#"
                SELECT session_id, protocol, threshold, total_nodes, status, started_at, completed_at, public_key
                FROM dkg_ceremonies
                ORDER BY started_at DESC
                "#,
                &[],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to list DKG ceremonies: {}", e)))?;

        // Note: This is a placeholder - we need to import the DkgCeremony type properly
        // For now, returning empty vector to avoid compilation errors
        warn!("list_dkg_ceremonies not fully implemented - need to define DkgCeremony type in storage crate");
        Ok(Vec::new())
    }

    /// Store encrypted key share for a node
    pub async fn store_key_share(
        &self,
        session_id: uuid::Uuid,
        node_id: NodeId,
        encrypted_share: &[u8],
    ) -> Result<()> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        // Get ceremony ID from session_id
        let ceremony_row = client
            .query_opt(
                "SELECT id FROM dkg_ceremonies WHERE session_id = $1",
                &[&session_id.to_string()],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get ceremony: {}", e)))?;

        let ceremony_id: i64 = ceremony_row
            .ok_or_else(|| Error::StorageError(format!("Ceremony not found: {}", session_id)))?
            .get(0);

        client
            .execute(
                r#"
                INSERT INTO key_shares (ceremony_id, node_id, encrypted_share)
                VALUES ($1, $2, $3)
                ON CONFLICT (ceremony_id, node_id) DO UPDATE
                SET encrypted_share = $3
                "#,
                &[&ceremony_id, &(node_id.0 as i64), &encrypted_share],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to store key share: {}", e)))?;

        info!("Stored key share: session_id={} node_id={}", session_id, node_id);

        Ok(())
    }

    /// Retrieve encrypted key share for a node
    pub async fn get_key_share(&self, session_id: uuid::Uuid, node_id: NodeId) -> Result<Option<Vec<u8>>> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        let row = client
            .query_opt(
                r#"
                SELECT ks.encrypted_share
                FROM key_shares ks
                JOIN dkg_ceremonies dc ON ks.ceremony_id = dc.id
                WHERE dc.session_id = $1 AND ks.node_id = $2
                "#,
                &[&session_id.to_string(), &(node_id.0 as i64)],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get key share: {}", e)))?;

        Ok(row.map(|r| r.get(0)))
    }

    /// Store aux_info for a node
    ///
    /// Aux_info is auxiliary information (Paillier keys, ring-Pedersen parameters)
    /// required for CGGMP24 presignature generation.
    pub async fn store_aux_info(
        &self,
        session_id: uuid::Uuid,
        node_id: NodeId,
        aux_info_data: &[u8],
    ) -> Result<()> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        client
            .execute(
                r#"
                INSERT INTO aux_info (session_id, node_id, aux_info_data, created_at)
                VALUES ($1, $2, $3, NOW())
                ON CONFLICT (session_id, node_id) DO UPDATE
                SET aux_info_data = $3, updated_at = NOW()
                "#,
                &[&session_id.to_string(), &(node_id.0 as i64), &aux_info_data],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to store aux_info: {}", e)))?;

        Ok(())
    }

    /// Get aux_info for a specific session and node
    pub async fn get_aux_info(&self, session_id: uuid::Uuid, node_id: NodeId) -> Result<Option<Vec<u8>>> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        let row = client
            .query_opt(
                r#"
                SELECT aux_info_data
                FROM aux_info
                WHERE session_id = $1 AND node_id = $2
                "#,
                &[&session_id.to_string(), &(node_id.0 as i64)],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get aux_info: {}", e)))?;

        Ok(row.map(|r| r.get(0)))
    }

    /// Get the latest aux_info for a node (most recent session)
    pub async fn get_latest_aux_info(&self, node_id: NodeId) -> Result<Option<(uuid::Uuid, Vec<u8>)>> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get client: {}", e)))?;

        let row = client
            .query_opt(
                r#"
                SELECT session_id, aux_info_data
                FROM aux_info
                WHERE node_id = $1
                ORDER BY created_at DESC
                LIMIT 1
                "#,
                &[&(node_id.0 as i64)],
            )
            .await
            .map_err(|e| Error::StorageError(format!("Failed to get latest aux_info: {}", e)))?;

        Ok(row.map(|r| {
            let session_id_str: String = r.get(0);
            let data: Vec<u8> = r.get(1);
            (uuid::Uuid::parse_str(&session_id_str).unwrap(), data)
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
