pub mod etcd;
pub mod postgres;

pub use etcd::EtcdStorage;
pub use postgres::PostgresStorage;

/// DKG ceremony status
#[derive(Debug, Clone)]
pub struct DkgCeremony {
    pub session_id: uuid::Uuid,
    pub protocol: String,
    pub threshold: u32,
    pub total_nodes: u32,
    pub status: String,
    pub public_key: Option<Vec<u8>>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub error: Option<String>,
}

/// Aux info ceremony status
#[derive(Debug, Clone)]
pub struct AuxInfoCeremony {
    pub session_id: uuid::Uuid,
    pub num_parties: u16,
    pub status: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub error: Option<String>,
}
