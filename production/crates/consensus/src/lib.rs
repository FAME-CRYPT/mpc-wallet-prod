//! Consensus layer for MPC threshold voting
//!
//! This crate implements Byzantine fault-tolerant consensus for threshold voting
//! on Bitcoin transactions. It provides:
//!
//! - **Byzantine Detection**: Detects and handles 4 types of Byzantine violations:
//!   1. DoubleVote: Same node votes differently on same transaction
//!   2. InvalidSignature: Vote signature verification fails
//!   3. MinorityVote: Node votes against consensus after threshold reached
//!   4. Timeout: Node doesn't respond within timeout (handled by network layer)
//!
//! - **Finite State Machine**: Manages transaction lifecycle:
//!   Initial -> Collecting -> ThresholdReached -> Submitted -> Confirmed
//!   with Byzantine and Timeout abort paths
//!
//! - **Vote Processing**: Orchestrates the consensus process:
//!   - Receives votes from network
//!   - Validates with ByzantineDetector
//!   - Counts votes using etcd
//!   - Updates FSM based on results
//!   - Records to PostgreSQL
//!
//! # Example
//!
//! ```no_run
//! use threshold_consensus::VoteProcessor;
//! use threshold_storage::{EtcdStorage, PostgresStorage};
//! use threshold_types::{Vote, NodeId, TransactionId};
//!
//! # async fn example() -> threshold_types::Result<()> {
//! let etcd = EtcdStorage::new(vec!["127.0.0.1:2379".to_string()]).await?;
//! let postgres = PostgresStorage::new(&threshold_types::PostgresConfig {
//!     url: "postgresql://localhost/mpc_wallet".to_string(),
//!     max_connections: 10,
//!     connect_timeout_secs: 5,
//! }).await?;
//!
//! let processor = VoteProcessor::new(etcd, postgres);
//!
//! // Process incoming vote from network
//! // let result = processor.process_vote(vote).await?;
//! # Ok(())
//! # }
//! ```

pub mod byzantine;
pub mod fsm;
pub mod vote_processor;

// Re-export main types
pub use byzantine::{ByzantineCheckResult, ByzantineDetector};
pub use fsm::{VoteFSM, VoteState};
pub use vote_processor::{VoteProcessingResult, VoteProcessor};
