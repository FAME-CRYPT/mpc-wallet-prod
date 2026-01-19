use crate::{EtcdStorage, PostgresStorage};
use chrono::{Duration, Utc};
use threshold_types::{Result, TransactionState};
use tokio::time;
use tracing::{error, info, warn};

/// Garbage collector for cleaning up old data from etcd and PostgreSQL
pub struct GarbageCollector {
    etcd: EtcdStorage,
    postgres: PostgresStorage,
    cleanup_interval: time::Duration,
    etcd_ttl_hours: i64,
    postgres_archive_days: i64,
}

impl GarbageCollector {
    pub fn new(
        etcd: EtcdStorage,
        postgres: PostgresStorage,
        cleanup_interval_hours: u64,
        etcd_ttl_hours: i64,
        postgres_archive_days: i64,
    ) -> Self {
        Self {
            etcd,
            postgres,
            cleanup_interval: time::Duration::from_secs(cleanup_interval_hours * 3600),
            etcd_ttl_hours,
            postgres_archive_days,
        }
    }

    /// Start the garbage collection loop
    pub async fn run(mut self) -> Result<()> {
        info!("Garbage collector started");
        info!("  Cleanup interval: {} hours", self.cleanup_interval.as_secs() / 3600);
        info!("  etcd TTL: {} hours", self.etcd_ttl_hours);
        info!("  PostgreSQL archive: {} days", self.postgres_archive_days);

        let mut interval = time::interval(self.cleanup_interval);

        loop {
            interval.tick().await;

            info!("Running garbage collection cycle...");

            if let Err(e) = self.cleanup_etcd().await {
                error!("etcd cleanup failed: {}", e);
            }

            if let Err(e) = self.cleanup_postgres().await {
                error!("PostgreSQL cleanup failed: {}", e);
            }

            info!("Garbage collection cycle completed");
        }
    }

    /// Clean up old etcd data (votes, counters, transaction status)
    async fn cleanup_etcd(&mut self) -> Result<()> {
        let cutoff_time = Utc::now() - Duration::hours(self.etcd_ttl_hours);
        info!("Cleaning etcd data older than {}", cutoff_time);

        // Get all confirmed transactions from PostgreSQL
        let confirmed_txs = self.postgres.get_confirmed_transactions_before(cutoff_time).await?;

        let mut deleted_count = 0;

        for tx_id in confirmed_txs {
            // Delete votes
            if let Err(e) = self.etcd.delete_all_votes(&tx_id).await {
                warn!("Failed to delete votes for tx_id={}: {}", tx_id, e);
                continue;
            }

            // Delete vote counters
            if let Err(e) = self.etcd.delete_all_vote_counts(&tx_id).await {
                warn!("Failed to delete vote counts for tx_id={}: {}", tx_id, e);
                continue;
            }

            // Delete transaction status
            if let Err(e) = self.etcd.delete_transaction_state(&tx_id).await {
                warn!("Failed to delete transaction state for tx_id={}: {}", tx_id, e);
                continue;
            }

            deleted_count += 1;
        }

        info!("etcd cleanup completed: {} transactions purged", deleted_count);

        Ok(())
    }

    /// Archive old PostgreSQL data
    async fn cleanup_postgres(&mut self) -> Result<()> {
        let cutoff_time = Utc::now() - Duration::days(self.postgres_archive_days);
        info!("Archiving PostgreSQL data older than {}", cutoff_time);

        // Archive old submissions
        let archived_count = self.postgres.archive_old_submissions(cutoff_time).await?;
        info!("Archived {} blockchain submissions", archived_count);

        // Clean up old vote history (keep violations forever for audit)
        let deleted_votes = self.postgres.delete_old_vote_history(cutoff_time).await?;
        info!("Deleted {} old vote history records", deleted_votes);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires running etcd and PostgreSQL
    async fn test_garbage_collection() {
        let etcd = EtcdStorage::new(vec!["127.0.0.1:2379".to_string()])
            .await
            .unwrap();
        let postgres = PostgresStorage::new("postgresql://localhost/threshold_voting")
            .await
            .unwrap();

        let gc = GarbageCollector::new(
            etcd,
            postgres,
            1,   // 1 hour interval
            24,  // 24 hour TTL
            30,  // 30 day archive
        );

        // Note: This would run forever, so we just test construction
        drop(gc);
    }
}
