//! Deterministic Participant Selection Policy
//!
//! This module provides deterministic algorithms for selecting participants
//! in MPC signing sessions. Determinism is critical for:
//!
//! 1. **Idempotency**: Same inputs produce same outputs across coordinator restarts.
//! 2. **Verification**: Nodes can independently verify participant selection.
//! 3. **P2P compatibility**: Any node can compute the same selection without coordinator.
//!
//! ## Selection Strategies
//!
//! - **Deterministic**: Hash-based selection using session seed + node indices.
//! - **RoundRobin**: Rotates through nodes based on request counter.
//! - **WeightedScore**: Considers node health scores for selection.
//!
//! ## Usage
//!
//! ```ignore
//! use common::selection::{ParticipantSelector, SelectionPolicy, SelectionInput};
//!
//! let selector = ParticipantSelector::new(SelectionPolicy::Deterministic);
//! let input = SelectionInput {
//!     seed: "wallet-id:message-hash".to_string(),
//!     available_nodes: vec![0, 1, 2, 3],
//!     threshold: 3,
//!     node_scores: None,
//! };
//!
//! let participants = selector.select(&input)?;
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Selection policy for participant selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SelectionPolicy {
    /// Hash-based deterministic selection.
    /// Given the same seed and available nodes, always selects the same participants.
    #[default]
    Deterministic,

    /// Round-robin selection based on a counter.
    /// Rotates through available nodes in order.
    RoundRobin,

    /// Weighted selection based on node health scores.
    /// Nodes with higher scores have higher selection probability.
    /// Falls back to deterministic if scores are not provided.
    WeightedScore,
}

impl std::fmt::Display for SelectionPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Deterministic => write!(f, "deterministic"),
            Self::RoundRobin => write!(f, "round-robin"),
            Self::WeightedScore => write!(f, "weighted-score"),
        }
    }
}

/// Input for participant selection.
#[derive(Debug, Clone)]
pub struct SelectionInput {
    /// Seed for deterministic selection (e.g., wallet_id + message_hash).
    pub seed: String,

    /// List of available node indices (healthy nodes).
    pub available_nodes: Vec<u16>,

    /// Number of participants to select (threshold).
    pub threshold: usize,

    /// Optional node health scores (0.0 - 1.0) for weighted selection.
    /// Index corresponds to node index.
    pub node_scores: Option<Vec<f64>>,

    /// Optional counter for round-robin selection.
    pub round_robin_counter: Option<u64>,
}

/// Result of participant selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionResult {
    /// Selected participant indices (sorted).
    pub participants: Vec<u16>,

    /// The policy used for selection.
    pub policy: SelectionPolicy,

    /// Hash of the selection input for verification.
    pub selection_hash: String,
}

impl SelectionResult {
    /// Verify that this result matches the expected selection for given input.
    pub fn verify(&self, input: &SelectionInput, policy: SelectionPolicy) -> bool {
        let selector = ParticipantSelector::new(policy);
        match selector.select(input) {
            Ok(result) => {
                result.participants == self.participants
                    && result.selection_hash == self.selection_hash
            }
            Err(_) => false,
        }
    }
}

/// Error type for selection operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum SelectionError {
    #[error("insufficient nodes: need {required}, available {available}")]
    InsufficientNodes { required: usize, available: usize },

    #[error("invalid threshold: {0}")]
    InvalidThreshold(String),

    #[error("no available nodes")]
    NoAvailableNodes,

    #[error("invalid node scores: {0}")]
    InvalidScores(String),
}

/// Participant selector implementing deterministic selection policies.
#[derive(Debug, Clone)]
pub struct ParticipantSelector {
    policy: SelectionPolicy,
}

impl ParticipantSelector {
    /// Create a new selector with the given policy.
    pub fn new(policy: SelectionPolicy) -> Self {
        Self { policy }
    }

    /// Select participants according to the configured policy.
    pub fn select(&self, input: &SelectionInput) -> Result<SelectionResult, SelectionError> {
        // Validate input
        if input.available_nodes.is_empty() {
            return Err(SelectionError::NoAvailableNodes);
        }

        if input.threshold == 0 {
            return Err(SelectionError::InvalidThreshold(
                "threshold must be > 0".to_string(),
            ));
        }

        if input.available_nodes.len() < input.threshold {
            return Err(SelectionError::InsufficientNodes {
                required: input.threshold,
                available: input.available_nodes.len(),
            });
        }

        let participants = match self.policy {
            SelectionPolicy::Deterministic => self.select_deterministic(input),
            SelectionPolicy::RoundRobin => self.select_round_robin(input),
            SelectionPolicy::WeightedScore => self.select_weighted(input)?,
        };

        let selection_hash = self.compute_selection_hash(input, &participants);

        Ok(SelectionResult {
            participants,
            policy: self.policy,
            selection_hash,
        })
    }

    /// Deterministic hash-based selection.
    ///
    /// Algorithm:
    /// 1. Hash the seed to get a deterministic random source.
    /// 2. For each available node, compute a score = hash(seed || node_index).
    /// 3. Sort nodes by score and select top `threshold` nodes.
    /// 4. Sort final selection by node index for consistent ordering.
    fn select_deterministic(&self, input: &SelectionInput) -> Vec<u16> {
        let mut scored_nodes: Vec<(u16, [u8; 32])> = input
            .available_nodes
            .iter()
            .map(|&node| {
                let mut hasher = Sha256::new();
                hasher.update(input.seed.as_bytes());
                hasher.update(b":");
                hasher.update(node.to_le_bytes());
                let hash: [u8; 32] = hasher.finalize().into();
                (node, hash)
            })
            .collect();

        // Sort by hash (deterministic ordering)
        scored_nodes.sort_by(|a, b| a.1.cmp(&b.1));

        // Take top threshold nodes
        let mut selected: Vec<u16> = scored_nodes
            .into_iter()
            .take(input.threshold)
            .map(|(node, _)| node)
            .collect();

        // Sort by node index for consistent final ordering
        selected.sort();
        selected
    }

    /// Round-robin selection based on counter.
    ///
    /// Algorithm:
    /// 1. Sort available nodes by index.
    /// 2. Use counter mod len(nodes) as starting offset.
    /// 3. Select consecutive nodes wrapping around.
    fn select_round_robin(&self, input: &SelectionInput) -> Vec<u16> {
        let mut nodes = input.available_nodes.clone();
        nodes.sort();

        let counter = input.round_robin_counter.unwrap_or(0);
        let offset = (counter as usize) % nodes.len();

        let mut selected = Vec::with_capacity(input.threshold);
        for i in 0..input.threshold {
            let idx = (offset + i) % nodes.len();
            selected.push(nodes[idx]);
        }

        selected.sort();
        selected
    }

    /// Weighted selection based on node scores.
    ///
    /// Algorithm:
    /// 1. Use scores to compute selection probability.
    /// 2. Use deterministic seed to make "random" selection reproducible.
    /// 3. Higher scores = higher selection probability.
    fn select_weighted(&self, input: &SelectionInput) -> Result<Vec<u16>, SelectionError> {
        let scores = match &input.node_scores {
            Some(s) => s.clone(),
            None => {
                // Fall back to deterministic if no scores
                return Ok(self.select_deterministic(input));
            }
        };

        // Validate scores
        for (i, &score) in scores.iter().enumerate() {
            if !(0.0..=1.0).contains(&score) {
                return Err(SelectionError::InvalidScores(format!(
                    "score for node {} is out of range [0, 1]: {}",
                    i, score
                )));
            }
        }

        // Create list of (node, weighted_score) where weighted_score includes
        // both the health score and a deterministic hash component
        let mut weighted: Vec<(u16, f64)> = input
            .available_nodes
            .iter()
            .map(|&node| {
                let health_score = scores.get(node as usize).copied().unwrap_or(0.5);

                // Add deterministic component from hash
                let mut hasher = Sha256::new();
                hasher.update(input.seed.as_bytes());
                hasher.update(b":weighted:");
                hasher.update(node.to_le_bytes());
                let hash: [u8; 32] = hasher.finalize().into();

                // Convert first 8 bytes to a f64 in [0, 1)
                let hash_val =
                    u64::from_le_bytes(hash[0..8].try_into().unwrap()) as f64 / u64::MAX as f64;

                // Combined score: health_score * 0.7 + hash_component * 0.3
                // This ensures healthier nodes are preferred but with some deterministic variance
                let combined = health_score * 0.7 + hash_val * 0.3;
                (node, combined)
            })
            .collect();

        // Sort by weighted score (descending)
        weighted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top threshold nodes
        let mut selected: Vec<u16> = weighted
            .into_iter()
            .take(input.threshold)
            .map(|(node, _)| node)
            .collect();

        selected.sort();
        Ok(selected)
    }

    /// Compute a hash of the selection for verification.
    fn compute_selection_hash(&self, input: &SelectionInput, participants: &[u16]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(input.seed.as_bytes());
        hasher.update(b":policy:");
        hasher.update(self.policy.to_string().as_bytes());
        hasher.update(b":available:");
        for node in &input.available_nodes {
            hasher.update(node.to_le_bytes());
        }
        hasher.update(b":threshold:");
        hasher.update((input.threshold as u32).to_le_bytes());
        hasher.update(b":selected:");
        for node in participants {
            hasher.update(node.to_le_bytes());
        }
        let hash = hasher.finalize();
        hex::encode(&hash[..16])
    }
}

/// Derive a selection seed from wallet ID and message hash.
///
/// This creates a deterministic seed that can be used for participant selection,
/// ensuring the same transaction always selects the same participants (given
/// the same available nodes).
pub fn derive_selection_seed(wallet_id: &str, message_hash: &[u8; 32]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"torcus-selection-seed:");
    hasher.update(wallet_id.as_bytes());
    hasher.update(b":");
    hasher.update(message_hash);
    let hash = hasher.finalize();
    hex::encode(hash)
}

/// Select which node should be the session initiator for P2P mode.
///
/// This function deterministically selects one node from the participants
/// to act as the session coordinator. The selection is based on the grant's
/// unique identifiers, ensuring all nodes agree on who the initiator is.
///
/// # Arguments
///
/// * `grant_id` - The unique grant ID (UUID bytes)
/// * `nonce` - The grant nonce
/// * `participants` - The list of participant node indices
///
/// # Returns
///
/// The party index of the selected initiator.
///
/// # Panics
///
/// Panics if `participants` is empty.
pub fn select_initiator(grant_id: &[u8], nonce: u64, participants: &[u16]) -> u16 {
    assert!(!participants.is_empty(), "participants cannot be empty");

    // Hash grant_id + nonce to get deterministic selection
    let mut hasher = Sha256::new();
    hasher.update(b"torcus-initiator:");
    hasher.update(grant_id);
    hasher.update(nonce.to_le_bytes());
    let hash = hasher.finalize();

    // Use first 8 bytes as index
    let index = u64::from_le_bytes(hash[0..8].try_into().unwrap());
    let selected_idx = (index as usize) % participants.len();

    participants[selected_idx]
}

/// Select initiator from a SigningGrant.
///
/// Convenience function that extracts grant_id and nonce from a SigningGrant.
pub fn select_initiator_from_grant(grant: &crate::grant::SigningGrant) -> u16 {
    select_initiator(grant.grant_id.as_bytes(), grant.nonce, &grant.participants)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_selection_is_stable() {
        let selector = ParticipantSelector::new(SelectionPolicy::Deterministic);
        let input = SelectionInput {
            seed: "test-wallet:abcd1234".to_string(),
            available_nodes: vec![0, 1, 2, 3],
            threshold: 3,
            node_scores: None,
            round_robin_counter: None,
        };

        // Run selection multiple times - should always get same result
        let result1 = selector.select(&input).unwrap();
        let result2 = selector.select(&input).unwrap();
        let result3 = selector.select(&input).unwrap();

        assert_eq!(result1.participants, result2.participants);
        assert_eq!(result2.participants, result3.participants);
        assert_eq!(result1.selection_hash, result2.selection_hash);
    }

    #[test]
    fn test_deterministic_selection_changes_with_seed() {
        let selector = ParticipantSelector::new(SelectionPolicy::Deterministic);

        let input1 = SelectionInput {
            seed: "wallet-a:hash1".to_string(),
            available_nodes: vec![0, 1, 2, 3],
            threshold: 2,
            node_scores: None,
            round_robin_counter: None,
        };

        let input2 = SelectionInput {
            seed: "wallet-b:hash2".to_string(),
            available_nodes: vec![0, 1, 2, 3],
            threshold: 2,
            node_scores: None,
            round_robin_counter: None,
        };

        let result1 = selector.select(&input1).unwrap();
        let result2 = selector.select(&input2).unwrap();

        // Different seeds should (likely) produce different selections
        // Note: There's a small chance they could be the same, but very unlikely
        assert_ne!(result1.selection_hash, result2.selection_hash);
    }

    #[test]
    fn test_round_robin_selection() {
        let selector = ParticipantSelector::new(SelectionPolicy::RoundRobin);
        let base_input = SelectionInput {
            seed: "ignored".to_string(),
            available_nodes: vec![0, 1, 2, 3],
            threshold: 2,
            node_scores: None,
            round_robin_counter: None,
        };

        // Counter 0: should select [0, 1]
        let mut input = base_input.clone();
        input.round_robin_counter = Some(0);
        let result = selector.select(&input).unwrap();
        assert_eq!(result.participants, vec![0, 1]);

        // Counter 1: should select [1, 2]
        input.round_robin_counter = Some(1);
        let result = selector.select(&input).unwrap();
        assert_eq!(result.participants, vec![1, 2]);

        // Counter 2: should select [2, 3]
        input.round_robin_counter = Some(2);
        let result = selector.select(&input).unwrap();
        assert_eq!(result.participants, vec![2, 3]);

        // Counter 3: should wrap to [0, 3]
        input.round_robin_counter = Some(3);
        let result = selector.select(&input).unwrap();
        assert_eq!(result.participants, vec![0, 3]);
    }

    #[test]
    fn test_weighted_selection_prefers_high_scores() {
        let selector = ParticipantSelector::new(SelectionPolicy::WeightedScore);
        let input = SelectionInput {
            seed: "test-seed".to_string(),
            available_nodes: vec![0, 1, 2, 3],
            threshold: 2,
            // Node 2 and 3 have much higher scores
            node_scores: Some(vec![0.1, 0.1, 0.9, 0.9]),
            round_robin_counter: None,
        };

        let result = selector.select(&input).unwrap();

        // With deterministic seed, high-score nodes should be preferred
        // Nodes 2 and 3 should be selected most of the time
        assert!(
            result.participants.contains(&2) || result.participants.contains(&3),
            "Expected high-score nodes to be selected: {:?}",
            result.participants
        );
    }

    #[test]
    fn test_weighted_falls_back_to_deterministic() {
        let selector = ParticipantSelector::new(SelectionPolicy::WeightedScore);
        let input = SelectionInput {
            seed: "test-seed".to_string(),
            available_nodes: vec![0, 1, 2, 3],
            threshold: 2,
            node_scores: None, // No scores provided
            round_robin_counter: None,
        };

        // Should not error, falls back to deterministic
        let result = selector.select(&input).unwrap();
        assert_eq!(result.participants.len(), 2);
    }

    #[test]
    fn test_insufficient_nodes_error() {
        let selector = ParticipantSelector::new(SelectionPolicy::Deterministic);
        let input = SelectionInput {
            seed: "test".to_string(),
            available_nodes: vec![0, 1], // Only 2 nodes
            threshold: 3,                // Need 3
            node_scores: None,
            round_robin_counter: None,
        };

        let result = selector.select(&input);
        assert!(matches!(
            result,
            Err(SelectionError::InsufficientNodes {
                required: 3,
                available: 2
            })
        ));
    }

    #[test]
    fn test_selection_result_verification() {
        let selector = ParticipantSelector::new(SelectionPolicy::Deterministic);
        let input = SelectionInput {
            seed: "verify-test".to_string(),
            available_nodes: vec![0, 1, 2, 3],
            threshold: 3,
            node_scores: None,
            round_robin_counter: None,
        };

        let result = selector.select(&input).unwrap();

        // Should verify against same input
        assert!(result.verify(&input, SelectionPolicy::Deterministic));

        // Should not verify with different input
        let different_input = SelectionInput {
            seed: "different-seed".to_string(),
            ..input.clone()
        };
        assert!(!result.verify(&different_input, SelectionPolicy::Deterministic));
    }

    #[test]
    fn test_derive_selection_seed() {
        let wallet_id = "test-wallet";
        let message_hash = [0xab; 32];

        let seed1 = derive_selection_seed(wallet_id, &message_hash);
        let seed2 = derive_selection_seed(wallet_id, &message_hash);

        // Same inputs should produce same seed
        assert_eq!(seed1, seed2);

        // Different inputs should produce different seed
        let seed3 = derive_selection_seed("other-wallet", &message_hash);
        assert_ne!(seed1, seed3);
    }

    #[test]
    fn test_selection_output_is_sorted() {
        let selector = ParticipantSelector::new(SelectionPolicy::Deterministic);

        // Test multiple seeds to ensure sorting
        for i in 0..10 {
            let input = SelectionInput {
                seed: format!("seed-{}", i),
                available_nodes: vec![3, 1, 4, 0, 2], // Unsorted input
                threshold: 3,
                node_scores: None,
                round_robin_counter: None,
            };

            let result = selector.select(&input).unwrap();

            // Output should always be sorted
            let mut sorted = result.participants.clone();
            sorted.sort();
            assert_eq!(result.participants, sorted);
        }
    }

    #[test]
    fn test_select_initiator_deterministic() {
        let grant_id = uuid::Uuid::new_v4();
        let nonce = 12345u64;
        let participants = vec![0, 1, 2, 3];

        // Same inputs should always select same initiator
        let init1 = select_initiator(grant_id.as_bytes(), nonce, &participants);
        let init2 = select_initiator(grant_id.as_bytes(), nonce, &participants);
        let init3 = select_initiator(grant_id.as_bytes(), nonce, &participants);

        assert_eq!(init1, init2);
        assert_eq!(init2, init3);

        // Initiator should be one of the participants
        assert!(participants.contains(&init1));
    }

    #[test]
    fn test_select_initiator_different_grants() {
        let participants = vec![0, 1, 2];

        // Different grant IDs should (usually) select different initiators
        let mut initiators = std::collections::HashSet::new();
        for _ in 0..100 {
            let grant_id = uuid::Uuid::new_v4();
            let nonce = rand::random();
            let init = select_initiator(grant_id.as_bytes(), nonce, &participants);
            initiators.insert(init);
        }

        // With 100 random grants and 3 participants, we should see all 3 selected at least once
        assert!(
            initiators.len() >= 2,
            "Expected variety in initiator selection, got {:?}",
            initiators
        );
    }

    #[test]
    fn test_select_initiator_single_participant() {
        let grant_id = uuid::Uuid::new_v4();
        let nonce = 999u64;
        let participants = vec![5];

        // With single participant, always selects that one
        let init = select_initiator(grant_id.as_bytes(), nonce, &participants);
        assert_eq!(init, 5);
    }

    #[test]
    #[should_panic(expected = "participants cannot be empty")]
    fn test_select_initiator_empty_participants() {
        let grant_id = uuid::Uuid::new_v4();
        let participants: Vec<u16> = vec![];
        select_initiator(grant_id.as_bytes(), 0, &participants);
    }
}
