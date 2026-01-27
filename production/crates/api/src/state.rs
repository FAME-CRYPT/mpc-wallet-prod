//! Shared application state for the API server

use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use threshold_bitcoin::BitcoinClient;
use threshold_orchestrator::{DkgService, AuxInfoService};
use threshold_storage::{EtcdStorage, PostgresStorage};
use threshold_types::VoteRequest;

/// Shared application state passed to all handlers
#[derive(Clone)]
pub struct AppState {
    /// PostgreSQL storage for transaction and node data
    pub postgres: Arc<PostgresStorage>,
    /// etcd storage for distributed state (with interior mutability)
    pub etcd: Arc<Mutex<EtcdStorage>>,
    /// Bitcoin client for blockchain operations
    pub bitcoin: Arc<BitcoinClient>,
    /// DKG service for distributed key generation
    pub dkg_service: Arc<DkgService>,
    /// Aux info service for auxiliary information generation
    pub aux_info_service: Arc<AuxInfoService>,
    /// Channel to trigger automatic voting
    pub vote_trigger: mpsc::Sender<VoteRequest>,
}

impl AppState {
    /// Create new application state
    pub fn new(
        postgres: PostgresStorage,
        etcd: EtcdStorage,
        bitcoin: BitcoinClient,
        dkg_service: DkgService,
        aux_info_service: AuxInfoService,
        vote_trigger: mpsc::Sender<VoteRequest>,
    ) -> Self {
        Self {
            postgres: Arc::new(postgres),
            etcd: Arc::new(Mutex::new(etcd)),
            bitcoin: Arc::new(bitcoin),
            dkg_service: Arc::new(dkg_service),
            aux_info_service: Arc::new(aux_info_service),
            vote_trigger,
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
