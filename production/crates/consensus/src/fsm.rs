use threshold_types::{Result, TransactionId, TransactionState, VotingError};
use tracing::info;

/// Finite State Machine states for transaction lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoteState {
    /// Initial state when transaction is created
    Initial,
    /// Collecting votes from nodes
    Collecting,
    /// Threshold reached, consensus achieved
    ThresholdReached,
    /// Transaction submitted to signing
    Submitted,
    /// Transaction confirmed on blockchain
    Confirmed,
    /// Aborted due to Byzantine violation
    AbortedByzantine,
    /// Aborted due to timeout
    AbortedTimeout,
}

impl From<TransactionState> for VoteState {
    fn from(state: TransactionState) -> Self {
        match state {
            TransactionState::Pending => VoteState::Initial,
            TransactionState::Voting | TransactionState::Collecting => VoteState::Collecting,
            TransactionState::ThresholdReached | TransactionState::Approved => VoteState::ThresholdReached,
            TransactionState::Submitted | TransactionState::Signed => VoteState::Submitted,
            TransactionState::Confirmed => VoteState::Confirmed,
            TransactionState::AbortedByzantine | TransactionState::Rejected => VoteState::AbortedByzantine,
            TransactionState::Failed => VoteState::AbortedTimeout,
            _ => VoteState::Initial,
        }
    }
}

impl From<VoteState> for TransactionState {
    fn from(state: VoteState) -> Self {
        match state {
            VoteState::Initial => TransactionState::Pending,
            VoteState::Collecting => TransactionState::Collecting,
            VoteState::ThresholdReached => TransactionState::ThresholdReached,
            VoteState::Submitted => TransactionState::Submitted,
            VoteState::Confirmed => TransactionState::Confirmed,
            VoteState::AbortedByzantine => TransactionState::AbortedByzantine,
            VoteState::AbortedTimeout => TransactionState::Failed,
        }
    }
}

/// Transaction State Machine for managing transaction lifecycle
///
/// Valid transitions:
/// - Initial -> Collecting (start voting)
/// - Collecting -> ThresholdReached (consensus reached)
/// - ThresholdReached -> Submitted (transaction submitted for signing)
/// - Submitted -> Submitted (idempotent)
/// - Submitted -> Confirmed (blockchain confirmation)
/// - Initial|Collecting|ThresholdReached -> AbortedByzantine (violation detected)
/// - Initial|Collecting -> AbortedTimeout (timeout expired)
pub struct VoteFSM {
    current_state: VoteState,
    tx_id: TransactionId,
}

impl VoteFSM {
    /// Create a new FSM in Initial state
    pub fn new(tx_id: TransactionId) -> Self {
        Self {
            current_state: VoteState::Initial,
            tx_id,
        }
    }

    /// Create FSM from an existing state
    pub fn from_state(tx_id: TransactionId, state: VoteState) -> Self {
        Self {
            current_state: state,
            tx_id,
        }
    }

    /// Get current state
    pub fn current_state(&self) -> VoteState {
        self.current_state
    }

    /// Start collecting votes
    pub fn start_collecting(&mut self) -> Result<()> {
        self.transition(VoteState::Collecting, vec![VoteState::Initial])
    }

    /// Mark threshold reached
    pub fn reach_threshold(&mut self) -> Result<()> {
        self.transition(VoteState::ThresholdReached, vec![VoteState::Collecting])
    }

    /// Mark transaction submitted (idempotent)
    pub fn submit(&mut self) -> Result<()> {
        self.transition(
            VoteState::Submitted,
            vec![VoteState::ThresholdReached, VoteState::Submitted],
        )
    }

    /// Mark transaction confirmed on blockchain
    pub fn confirm(&mut self) -> Result<()> {
        self.transition(VoteState::Confirmed, vec![VoteState::Submitted])
    }

    /// Abort due to Byzantine violation
    pub fn abort_byzantine(&mut self) -> Result<()> {
        self.transition(
            VoteState::AbortedByzantine,
            vec![
                VoteState::Initial,
                VoteState::Collecting,
                VoteState::ThresholdReached,
            ],
        )
    }

    /// Abort due to timeout
    pub fn abort_timeout(&mut self) -> Result<()> {
        self.transition(
            VoteState::AbortedTimeout,
            vec![VoteState::Initial, VoteState::Collecting],
        )
    }

    /// Internal transition method with validation
    fn transition(&mut self, new_state: VoteState, allowed_from: Vec<VoteState>) -> Result<()> {
        if !allowed_from.contains(&self.current_state) {
            return Err(VotingError::StorageError(format!(
                "Invalid state transition for tx_id={}: {:?} -> {:?}. Allowed transitions from: {:?}",
                self.tx_id, self.current_state, new_state, allowed_from
            )));
        }

        info!(
            "State transition for tx_id={}: {:?} -> {:?}",
            self.tx_id, self.current_state, new_state
        );

        self.current_state = new_state;
        Ok(())
    }

    /// Check if state is terminal (no further transitions allowed)
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.current_state,
            VoteState::Confirmed | VoteState::AbortedByzantine | VoteState::AbortedTimeout
        )
    }

    /// Check if FSM can accept new votes
    pub fn can_accept_votes(&self) -> bool {
        matches!(self.current_state, VoteState::Collecting)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fsm_happy_path() {
        let tx_id = TransactionId::from("test_tx");
        let mut fsm = VoteFSM::new(tx_id);

        assert_eq!(fsm.current_state(), VoteState::Initial);

        assert!(fsm.start_collecting().is_ok());
        assert_eq!(fsm.current_state(), VoteState::Collecting);
        assert!(fsm.can_accept_votes());

        assert!(fsm.reach_threshold().is_ok());
        assert_eq!(fsm.current_state(), VoteState::ThresholdReached);
        assert!(!fsm.can_accept_votes());

        assert!(fsm.submit().is_ok());
        assert_eq!(fsm.current_state(), VoteState::Submitted);

        assert!(fsm.confirm().is_ok());
        assert_eq!(fsm.current_state(), VoteState::Confirmed);
        assert!(fsm.is_terminal());
    }

    #[test]
    fn test_fsm_byzantine_abort() {
        let tx_id = TransactionId::from("test_tx");
        let mut fsm = VoteFSM::new(tx_id);

        assert!(fsm.start_collecting().is_ok());
        assert_eq!(fsm.current_state(), VoteState::Collecting);

        assert!(fsm.abort_byzantine().is_ok());
        assert_eq!(fsm.current_state(), VoteState::AbortedByzantine);
        assert!(fsm.is_terminal());
    }

    #[test]
    fn test_fsm_invalid_transition() {
        let tx_id = TransactionId::from("test_tx");
        let mut fsm = VoteFSM::new(tx_id);

        // Cannot confirm without going through proper states
        assert!(fsm.confirm().is_err());

        assert!(fsm.start_collecting().is_ok());

        // Cannot confirm from Collecting state
        assert!(fsm.confirm().is_err());
    }

    #[test]
    fn test_fsm_idempotent_submit() {
        let tx_id = TransactionId::from("test_tx");
        let mut fsm = VoteFSM::new(tx_id);

        assert!(fsm.start_collecting().is_ok());
        assert!(fsm.reach_threshold().is_ok());
        assert!(fsm.submit().is_ok());

        // Submit again should succeed (idempotent)
        assert!(fsm.submit().is_ok());

        assert_eq!(fsm.current_state(), VoteState::Submitted);
    }

    #[test]
    fn test_fsm_timeout_abort() {
        let tx_id = TransactionId::from("test_tx");
        let mut fsm = VoteFSM::new(tx_id);

        assert!(fsm.start_collecting().is_ok());

        assert!(fsm.abort_timeout().is_ok());
        assert_eq!(fsm.current_state(), VoteState::AbortedTimeout);
        assert!(fsm.is_terminal());
    }
}
