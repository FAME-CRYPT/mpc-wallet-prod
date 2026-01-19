use threshold_types::{Result, TransactionId, TransactionState, VotingError};
use tracing::{info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoteState {
    Initial,
    Collecting,
    ThresholdReached,
    Submitted,
    Confirmed,
    AbortedByzantine,
    AbortedTimeout,
}

impl From<TransactionState> for VoteState {
    fn from(state: TransactionState) -> Self {
        match state {
            TransactionState::Collecting => VoteState::Collecting,
            TransactionState::ThresholdReached => VoteState::ThresholdReached,
            TransactionState::Submitted => VoteState::Submitted,
            TransactionState::Confirmed => VoteState::Confirmed,
            TransactionState::AbortedByzantine => VoteState::AbortedByzantine,
        }
    }
}

impl From<VoteState> for TransactionState {
    fn from(state: VoteState) -> Self {
        match state {
            VoteState::Initial | VoteState::Collecting => TransactionState::Collecting,
            VoteState::ThresholdReached => TransactionState::ThresholdReached,
            VoteState::Submitted => TransactionState::Submitted,
            VoteState::Confirmed => TransactionState::Confirmed,
            VoteState::AbortedByzantine | VoteState::AbortedTimeout => {
                TransactionState::AbortedByzantine
            }
        }
    }
}

pub struct VoteFSM {
    current_state: VoteState,
    tx_id: TransactionId,
}

impl VoteFSM {
    pub fn new(tx_id: TransactionId) -> Self {
        Self {
            current_state: VoteState::Initial,
            tx_id,
        }
    }

    pub fn from_state(tx_id: TransactionId, state: VoteState) -> Self {
        Self {
            current_state: state,
            tx_id,
        }
    }

    pub fn current_state(&self) -> VoteState {
        self.current_state
    }

    pub fn start_collecting(&mut self) -> Result<()> {
        self.transition(VoteState::Collecting, vec![VoteState::Initial])
    }

    pub fn reach_threshold(&mut self) -> Result<()> {
        self.transition(VoteState::ThresholdReached, vec![VoteState::Collecting])
    }

    pub fn submit(&mut self) -> Result<()> {
        self.transition(
            VoteState::Submitted,
            vec![VoteState::ThresholdReached, VoteState::Submitted],
        )
    }

    pub fn confirm(&mut self) -> Result<()> {
        self.transition(VoteState::Confirmed, vec![VoteState::Submitted])
    }

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

    pub fn abort_timeout(&mut self) -> Result<()> {
        self.transition(
            VoteState::AbortedTimeout,
            vec![VoteState::Initial, VoteState::Collecting],
        )
    }

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

    pub fn is_terminal(&self) -> bool {
        matches!(
            self.current_state,
            VoteState::Confirmed | VoteState::AbortedByzantine | VoteState::AbortedTimeout
        )
    }

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

        assert!(fsm.confirm().is_err());

        assert!(fsm.start_collecting().is_ok());

        assert!(fsm.confirm().is_err());
    }

    #[test]
    fn test_fsm_idempotent_submit() {
        let tx_id = TransactionId::from("test_tx");
        let mut fsm = VoteFSM::new(tx_id);

        assert!(fsm.start_collecting().is_ok());
        assert!(fsm.reach_threshold().is_ok());
        assert!(fsm.submit().is_ok());

        assert!(fsm.submit().is_ok());

        assert_eq!(fsm.current_state(), VoteState::Submitted);
    }
}
