//! Common test utilities and infrastructure for integration tests.

pub mod fixtures;
pub mod mock_services;
pub mod test_containers;

pub use fixtures::*;
pub use mock_services::*;
pub use test_containers::*;

use threshold_types::{NodeId, PeerId, TxId, Vote};
use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;

/// Test context that holds all necessary infrastructure for tests
pub struct TestContext {
    pub postgres: PostgresContainer,
    pub etcd: EtcdContainer,
    pub bitcoin_mock: BitcoinMockServer,
}

impl TestContext {
    /// Create a new test context with all infrastructure
    pub async fn new() -> Self {
        let postgres = PostgresContainer::new().await;
        let etcd = EtcdContainer::new().await;
        let bitcoin_mock = BitcoinMockServer::new().await;

        Self {
            postgres,
            etcd,
            bitcoin_mock,
        }
    }

    /// Get PostgreSQL connection string
    pub fn postgres_url(&self) -> String {
        self.postgres.connection_string()
    }

    /// Get etcd endpoints
    pub fn etcd_endpoints(&self) -> Vec<String> {
        self.etcd.endpoints()
    }

    /// Get Bitcoin mock server URL
    pub fn bitcoin_url(&self) -> String {
        self.bitcoin_mock.url()
    }
}

/// Create a test vote with valid signature
pub fn create_signed_vote(
    node_id: NodeId,
    tx_id: TxId,
    round_id: u64,
    approve: bool,
    value: u64,
) -> (Vote, SigningKey) {
    let signing_key = SigningKey::generate(&mut OsRng);
    let public_key = signing_key.verifying_key();

    // Create message to sign: tx_id||value
    let message = format!("{}||{}", tx_id, value);
    let signature = signing_key.sign(message.as_bytes());

    let vote = Vote {
        tx_id,
        node_id,
        peer_id: PeerId::from(format!("peer-{}", node_id.0)),
        round_id,
        approve,
        value,
        signature: signature.to_bytes().to_vec(),
        public_key: public_key.to_bytes().to_vec(),
        timestamp: Utc::now(),
    };

    (vote, signing_key)
}

/// Create a test vote with invalid signature
pub fn create_invalid_signature_vote(
    node_id: NodeId,
    tx_id: TxId,
    round_id: u64,
    approve: bool,
    value: u64,
) -> Vote {
    Vote {
        tx_id,
        node_id,
        peer_id: PeerId::from(format!("peer-{}", node_id.0)),
        round_id,
        approve,
        value,
        signature: vec![0u8; 64], // Invalid signature
        public_key: vec![1u8; 32], // Random public key
        timestamp: Utc::now(),
    }
}

/// Wait for condition with timeout
pub async fn wait_for_condition<F, Fut>(
    mut condition: F,
    timeout_secs: u64,
) -> Result<(), String>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);

    while start.elapsed() < timeout {
        if condition().await {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    Err(format!("Condition not met within {} seconds", timeout_secs))
}

/// Generate deterministic test transaction ID
pub fn test_tx_id(suffix: &str) -> TxId {
    TxId::from(format!("test_tx_{}", suffix))
}

/// Generate deterministic test node ID
pub fn test_node_id(id: u64) -> NodeId {
    NodeId(id)
}
