//! Shared application state for the API server

use std::sync::Arc;
use threshold_bitcoin::BitcoinClient;
use threshold_storage::{EtcdStorage, PostgresStorage};

/// Shared application state passed to all handlers
#[derive(Clone)]
pub struct AppState {
    /// PostgreSQL storage for transaction and node data
    pub postgres: Arc<PostgresStorage>,
    /// etcd storage for distributed state
    pub etcd: Arc<EtcdStorage>,
    /// Bitcoin client for blockchain operations
    pub bitcoin: Arc<BitcoinClient>,
}

impl AppState {
    /// Create new application state
    pub fn new(
        postgres: PostgresStorage,
        etcd: EtcdStorage,
        bitcoin: BitcoinClient,
    ) -> Self {
        Self {
            postgres: Arc::new(postgres),
            etcd: Arc::new(etcd),
            bitcoin: Arc::new(bitcoin),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        // This is only for testing purposes and should never be used in production
        // It will panic if any of the fields are accessed
        panic!("AppState::default() should not be used in production. Create proper instances with AppState::new()")
    }
}
