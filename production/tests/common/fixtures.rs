//! Test data fixtures for integration tests.

use threshold_types::*;
use chrono::Utc;
use threshold_bitcoin::Utxo;

/// Create a sample vote with deterministic data
pub fn sample_vote(node_id: u64, tx_id: &str, value: u64) -> Vote {
    Vote {
        tx_id: TxId::from(tx_id),
        node_id: NodeId(node_id),
        peer_id: PeerId::from(format!("peer-{}", node_id)),
        round_id: 1,
        approve: true,
        value,
        signature: vec![0xAB; 64],
        public_key: vec![0xCD; 32],
        timestamp: Utc::now(),
    }
}

/// Create a sample transaction
pub fn sample_transaction(txid: &str) -> Transaction {
    Transaction {
        id: 0,
        txid: TxId::from(txid),
        state: TransactionState::Pending,
        unsigned_tx: vec![0x01, 0x02, 0x03],
        signed_tx: None,
        recipient: "tb1qtest".to_string(),
        amount_sats: 10_000,
        fee_sats: 500,
        metadata: Some("test metadata".to_string()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

/// Create a sample Byzantine violation
pub fn sample_byzantine_violation(
    node_id: u64,
    tx_id: &str,
    violation_type: ViolationType,
) -> ByzantineViolation {
    ByzantineViolation {
        id: None,
        peer_id: PeerId::from(format!("peer-{}", node_id)),
        node_id: Some(NodeId(node_id)),
        tx_id: TxId::from(tx_id),
        violation_type,
        evidence: serde_json::json!({
            "test": "evidence",
            "node_id": node_id,
        }),
        detected_at: Utc::now(),
    }
}

/// Create a sample voting round
pub fn sample_voting_round(tx_id: &str) -> VotingRound {
    VotingRound {
        id: 0,
        tx_id: TxId::from(tx_id),
        round_number: 1,
        total_nodes: 5,
        threshold: 4,
        votes_received: 0,
        approved: false,
        completed: false,
        started_at: Utc::now(),
        completed_at: None,
    }
}

/// Create sample UTXOs for Bitcoin transaction building
pub fn sample_utxos() -> Vec<Utxo> {
    vec![
        Utxo {
            txid: "0000000000000000000000000000000000000000000000000000000000000001".to_string(),
            vout: 0,
            value: 50_000,
            status: threshold_bitcoin::types::UtxoStatus {
                confirmed: true,
                block_height: Some(100_000),
                block_hash: Some("00000000000000000001".to_string()),
                block_time: Some(1_700_000_000),
            },
        },
        Utxo {
            txid: "0000000000000000000000000000000000000000000000000000000000000002".to_string(),
            vout: 0,
            value: 100_000,
            status: threshold_bitcoin::types::UtxoStatus {
                confirmed: true,
                block_height: Some(100_001),
                block_hash: Some("00000000000000000002".to_string()),
                block_time: Some(1_700_000_600),
            },
        },
        Utxo {
            txid: "0000000000000000000000000000000000000000000000000000000000000003".to_string(),
            vout: 1,
            value: 25_000,
            status: threshold_bitcoin::types::UtxoStatus {
                confirmed: true,
                block_height: Some(100_002),
                block_hash: Some("00000000000000000003".to_string()),
                block_time: Some(1_700_001_200),
            },
        },
    ]
}

/// Create a sample presignature usage record
pub fn sample_presignature_usage() -> PresignatureUsage {
    PresignatureUsage {
        id: 0,
        presig_id: PresignatureId::new(),
        transaction_id: 1,
        used_at: Utc::now(),
        generation_time_ms: 250,
    }
}

/// Create sample consensus configuration
pub fn sample_consensus_config() -> ConsensusConfig {
    ConsensusConfig {
        total_nodes: 5,
        threshold: 4,
        vote_timeout_secs: 30,
    }
}

/// Create sample PostgreSQL configuration
pub fn sample_postgres_config(url: String) -> PostgresConfig {
    PostgresConfig {
        url,
        max_connections: 10,
        connect_timeout_secs: 5,
    }
}

/// Create sample etcd configuration
pub fn sample_etcd_config(endpoints: Vec<String>) -> EtcdConfig {
    EtcdConfig {
        endpoints,
        connect_timeout_secs: 5,
        request_timeout_secs: 10,
    }
}

/// Create a batch of votes for consensus testing
pub fn create_vote_batch(
    tx_id: &str,
    value: u64,
    count: usize,
) -> Vec<Vote> {
    (1..=count)
        .map(|i| sample_vote(i as u64, tx_id, value))
        .collect()
}

/// Create conflicting votes (double-vote scenario)
pub fn create_conflicting_votes(
    node_id: u64,
    tx_id: &str,
) -> (Vote, Vote) {
    let mut vote1 = sample_vote(node_id, tx_id, 100);
    vote1.timestamp = Utc::now();

    let mut vote2 = sample_vote(node_id, tx_id, 200);
    vote2.timestamp = Utc::now();

    (vote1, vote2)
}

/// Create votes that reach threshold (4-of-5)
pub fn create_threshold_votes(tx_id: &str, value: u64) -> Vec<Vote> {
    create_vote_batch(tx_id, value, 4)
}

/// Create votes with different values (no consensus)
pub fn create_split_votes(tx_id: &str) -> Vec<Vote> {
    vec![
        sample_vote(1, tx_id, 100),
        sample_vote(2, tx_id, 100),
        sample_vote(3, tx_id, 200),
        sample_vote(4, tx_id, 200),
        sample_vote(5, tx_id, 300),
    ]
}
