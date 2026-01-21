//! P2P Session Coordinator
//!
//! This module provides the session coordination logic for P2P-initiated
//! signing sessions. It handles:
//!
//! - Proposing sessions to participants
//! - Collecting acknowledgments
//! - Signaling session start when threshold is reached
//! - Handling session abort
//!
//! ## Usage
//!
//! ```rust,ignore
//! use protocols::p2p::coordinator::P2pSessionCoordinator;
//!
//! // Create coordinator
//! let coordinator = P2pSessionCoordinator::new(
//!     party_index,
//!     transport,
//!     grant_verifying_key,
//! );
//!
//! // As initiator: propose and wait for session start
//! let handle = coordinator.initiate_session(grant, "cggmp24").await?;
//!
//! // As participant: handle incoming proposal
//! coordinator.handle_proposal(proposal).await?;
//! ```

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use common::grant::SigningGrant;
use common::selection::select_initiator_from_grant;
use ed25519_dalek::VerifyingKey;
use tokio::sync::{oneshot, RwLock};
use tracing::{debug, info, warn};

use super::session::{P2pSessionStatus, SessionAbort, SessionAck, SessionProposal, SessionStart};

/// Default timeout for session proposal acknowledgments.
pub const PROPOSAL_TIMEOUT_SECS: u64 = 10;

/// Default timeout for session start after threshold reached.
pub const START_TIMEOUT_SECS: u64 = 30;

/// Error types for P2P session coordination.
#[derive(Debug, Clone, thiserror::Error)]
pub enum CoordinatorError {
    #[error("not the designated initiator for this grant")]
    NotInitiator,

    #[error("invalid grant signature")]
    InvalidGrant,

    #[error("grant has expired")]
    GrantExpired,

    #[error("not a participant in this session")]
    NotParticipant,

    #[error("session already exists: {0}")]
    SessionExists(String),

    #[error("session not found: {0}")]
    SessionNotFound(String),

    #[error("timeout waiting for acknowledgments")]
    AckTimeout,

    #[error("insufficient acknowledgments: got {got}, need {need}")]
    InsufficientAcks { got: usize, need: usize },

    #[error("session aborted: {0}")]
    Aborted(String),

    #[error("transport error: {0}")]
    Transport(String),

    #[error("internal error: {0}")]
    Internal(String),
}

/// State for a pending session (waiting for acks).
#[derive(Debug)]
struct PendingSession {
    /// The grant authorizing this session.
    grant: SigningGrant,
    /// Party index of the initiator.
    initiator: u16,
    /// Protocol type (stored for future use in protocol dispatch).
    #[allow(dead_code)]
    protocol: String,
    /// Parties that have acknowledged.
    acks: HashSet<u16>,
    /// Parties that have rejected.
    rejects: HashMap<u16, String>,
    /// When the session was proposed.
    created_at: Instant,
    /// Channel to notify when threshold reached.
    threshold_notify: Option<oneshot::Sender<Vec<u16>>>,
}

/// State for an active session (protocol execution).
#[derive(Debug)]
struct ActiveSession {
    /// The grant authorizing this session.
    grant: SigningGrant,
    /// Confirmed participants.
    participants: Vec<u16>,
    /// Current status.
    status: P2pSessionStatus,
    /// When the session started (for timeout tracking).
    #[allow(dead_code)]
    started_at: Instant,
}

/// P2P Session Coordinator.
///
/// Handles the coordination of P2P signing sessions, including
/// proposal broadcasting, acknowledgment collection, and session
/// lifecycle management.
pub struct P2pSessionCoordinator {
    /// Our party index.
    party_index: u16,
    /// Grant verification key (from coordinator/policy engine).
    grant_verifying_key: VerifyingKey,
    /// Pending sessions waiting for acks.
    pending: RwLock<HashMap<String, PendingSession>>,
    /// Active sessions (started, in progress, or completed).
    active: RwLock<HashMap<String, ActiveSession>>,
    /// Configuration.
    config: CoordinatorConfig,
}

/// Configuration for the coordinator.
#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    /// Timeout for proposal acks.
    pub proposal_timeout: Duration,
    /// Timeout for session start.
    pub start_timeout: Duration,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            proposal_timeout: Duration::from_secs(PROPOSAL_TIMEOUT_SECS),
            start_timeout: Duration::from_secs(START_TIMEOUT_SECS),
        }
    }
}

/// Handle returned when initiating a session.
#[derive(Debug)]
pub struct SessionHandle {
    /// Session ID.
    pub session_id: String,
    /// Confirmed participants.
    pub participants: Vec<u16>,
    /// The grant used.
    pub grant: SigningGrant,
}

impl P2pSessionCoordinator {
    /// Create a new P2P session coordinator.
    pub fn new(party_index: u16, grant_verifying_key: VerifyingKey) -> Self {
        Self::with_config(
            party_index,
            grant_verifying_key,
            CoordinatorConfig::default(),
        )
    }

    /// Create a new coordinator with custom configuration.
    pub fn with_config(
        party_index: u16,
        grant_verifying_key: VerifyingKey,
        config: CoordinatorConfig,
    ) -> Self {
        Self {
            party_index,
            grant_verifying_key,
            pending: RwLock::new(HashMap::new()),
            active: RwLock::new(HashMap::new()),
            config,
        }
    }

    /// Get our party index.
    pub fn party_index(&self) -> u16 {
        self.party_index
    }

    /// Check if we should be the initiator for this grant.
    pub fn is_initiator(&self, grant: &SigningGrant) -> bool {
        let initiator = select_initiator_from_grant(grant);
        initiator == self.party_index
    }

    /// Validate a grant.
    pub fn validate_grant(&self, grant: &SigningGrant) -> Result<(), CoordinatorError> {
        // Verify signature
        grant
            .verify(&self.grant_verifying_key)
            .map_err(|_| CoordinatorError::InvalidGrant)?;

        // Check expiry
        if grant.is_expired() {
            return Err(CoordinatorError::GrantExpired);
        }

        // Check if we're a participant
        if !grant.participants.contains(&self.party_index) {
            return Err(CoordinatorError::NotParticipant);
        }

        Ok(())
    }

    /// Prepare a session proposal (for initiator).
    ///
    /// This validates the grant and creates a proposal, but does NOT
    /// broadcast it. The caller is responsible for broadcasting.
    ///
    /// Returns the proposal and a channel that will receive confirmed
    /// participants when threshold is reached.
    pub async fn prepare_proposal(
        &self,
        grant: SigningGrant,
        protocol: &str,
    ) -> Result<(SessionProposal, oneshot::Receiver<Vec<u16>>), CoordinatorError> {
        // Validate grant
        self.validate_grant(&grant)?;

        // Check if we're the designated initiator
        if !self.is_initiator(&grant) {
            return Err(CoordinatorError::NotInitiator);
        }

        let session_id = grant.session_id();

        // Check if session already exists
        {
            let pending = self.pending.read().await;
            if pending.contains_key(&session_id) {
                return Err(CoordinatorError::SessionExists(session_id));
            }
        }
        {
            let active = self.active.read().await;
            if active.contains_key(&session_id) {
                return Err(CoordinatorError::SessionExists(session_id));
            }
        }

        // Create proposal
        let proposal = SessionProposal::new(grant.clone(), self.party_index, protocol);

        // Create notification channel
        let (tx, rx) = oneshot::channel();

        // Store pending session
        let pending_session = PendingSession {
            grant,
            initiator: self.party_index,
            protocol: protocol.to_string(),
            acks: HashSet::new(),
            rejects: HashMap::new(),
            created_at: Instant::now(),
            threshold_notify: Some(tx),
        };

        {
            let mut pending = self.pending.write().await;
            pending.insert(session_id.clone(), pending_session);
        }

        info!(
            "Prepared session proposal: session_id={}, protocol={}",
            session_id, protocol
        );

        Ok((proposal, rx))
    }

    /// Handle an incoming session acknowledgment.
    ///
    /// Called when we receive a SessionAck message (we're the initiator).
    /// If threshold is reached, moves session to active and notifies via channel.
    pub async fn handle_ack(&self, ack: SessionAck) -> Result<(), CoordinatorError> {
        let session_id = &ack.session_id;

        let mut pending = self.pending.write().await;
        let session = pending
            .get_mut(session_id)
            .ok_or_else(|| CoordinatorError::SessionNotFound(session_id.clone()))?;

        // Only initiator processes acks
        if session.initiator != self.party_index {
            warn!(
                "Received ack but we're not initiator: session_id={}",
                session_id
            );
            return Ok(());
        }

        if ack.accepted {
            session.acks.insert(ack.party_index);
            debug!(
                "Received ack from party {}: session_id={}, total_acks={}",
                ack.party_index,
                session_id,
                session.acks.len()
            );
        } else {
            session.rejects.insert(
                ack.party_index,
                ack.error.unwrap_or_else(|| "unknown".to_string()),
            );
            warn!(
                "Received reject from party {}: session_id={}, reason={:?}",
                ack.party_index, session_id, session.rejects[&ack.party_index]
            );
        }

        // Check if threshold reached (include ourselves)
        let threshold = session.grant.threshold as usize;
        let mut confirmed: Vec<u16> = session.acks.iter().copied().collect();
        confirmed.push(self.party_index); // Initiator is implicitly confirmed
        confirmed.sort();
        confirmed.dedup();

        if confirmed.len() >= threshold {
            info!(
                "Threshold reached for session {}: {} participants",
                session_id,
                confirmed.len()
            );

            // Notify via channel
            if let Some(tx) = session.threshold_notify.take() {
                let _ = tx.send(confirmed.clone());
            }

            // Move to active
            let pending_session = pending.remove(session_id).unwrap();
            drop(pending);

            let active_session = ActiveSession {
                grant: pending_session.grant,
                participants: confirmed,
                status: P2pSessionStatus::Starting,
                started_at: Instant::now(),
            };

            let mut active = self.active.write().await;
            active.insert(session_id.clone(), active_session);
        }

        Ok(())
    }

    /// Handle an incoming session proposal (as participant, not initiator).
    ///
    /// Validates the grant and returns an ack/reject response.
    pub async fn handle_proposal(
        &self,
        proposal: &SessionProposal,
    ) -> Result<SessionAck, CoordinatorError> {
        let session_id = proposal.session_id();

        // Validate the grant
        if let Err(e) = self.validate_grant(&proposal.grant) {
            return Ok(SessionAck::reject(
                session_id,
                self.party_index,
                e.to_string(),
            ));
        }

        // Check that proposer is the correct initiator
        let expected_initiator = select_initiator_from_grant(&proposal.grant);
        if proposal.initiator_party != expected_initiator {
            return Ok(SessionAck::reject(
                session_id,
                self.party_index,
                format!(
                    "wrong initiator: expected {}, got {}",
                    expected_initiator, proposal.initiator_party
                ),
            ));
        }

        // Check if we already have this session
        {
            let pending = self.pending.read().await;
            if pending.contains_key(&session_id) {
                return Ok(SessionAck::reject(
                    session_id,
                    self.party_index,
                    "session already pending".to_string(),
                ));
            }
        }
        {
            let active = self.active.read().await;
            if active.contains_key(&session_id) {
                // Already active - send accept anyway (idempotency)
                return Ok(SessionAck::accept(session_id, self.party_index));
            }
        }

        // Store as pending (waiting for SessionStart)
        let pending_session = PendingSession {
            grant: proposal.grant.clone(),
            initiator: proposal.initiator_party,
            protocol: proposal.protocol.clone(),
            acks: HashSet::new(),
            rejects: HashMap::new(),
            created_at: Instant::now(),
            threshold_notify: None,
        };

        {
            let mut pending = self.pending.write().await;
            pending.insert(session_id.clone(), pending_session);
        }

        info!(
            "Accepted session proposal: session_id={}, initiator={}",
            session_id, proposal.initiator_party
        );

        Ok(SessionAck::accept(session_id, self.party_index))
    }

    /// Handle a session start signal.
    ///
    /// Called when we receive SessionStart (we're a participant, not initiator).
    pub async fn handle_start(&self, start: &SessionStart) -> Result<(), CoordinatorError> {
        let session_id = &start.session_id;

        // Move from pending to active
        let pending_session = {
            let mut pending = self.pending.write().await;
            pending.remove(session_id)
        };

        let pending_session =
            pending_session.ok_or_else(|| CoordinatorError::SessionNotFound(session_id.clone()))?;

        let active_session = ActiveSession {
            grant: pending_session.grant,
            participants: start.participants.clone(),
            status: P2pSessionStatus::InProgress,
            started_at: Instant::now(),
        };

        {
            let mut active = self.active.write().await;
            active.insert(session_id.clone(), active_session);
        }

        info!(
            "Session started: session_id={}, participants={:?}",
            session_id, start.participants
        );

        Ok(())
    }

    /// Handle a session abort.
    pub async fn handle_abort(&self, abort: &SessionAbort) -> Result<(), CoordinatorError> {
        let session_id = &abort.session_id;

        // Remove from pending if exists
        {
            let mut pending = self.pending.write().await;
            if pending.remove(session_id).is_some() {
                warn!(
                    "Session aborted (pending): session_id={}, reason={}",
                    session_id, abort.reason
                );
                return Ok(());
            }
        }

        // Mark as aborted in active
        {
            let mut active = self.active.write().await;
            if let Some(session) = active.get_mut(session_id) {
                session.status = P2pSessionStatus::Aborted;
                warn!(
                    "Session aborted (active): session_id={}, reason={}",
                    session_id, abort.reason
                );
                return Ok(());
            }
        }

        // Session not found - may have already completed/been cleaned up
        debug!(
            "Abort received for unknown session: session_id={}",
            session_id
        );
        Ok(())
    }

    /// Mark a session as completed.
    pub async fn complete_session(&self, session_id: &str) -> Result<(), CoordinatorError> {
        let mut active = self.active.write().await;
        let session = active
            .get_mut(session_id)
            .ok_or_else(|| CoordinatorError::SessionNotFound(session_id.to_string()))?;

        session.status = P2pSessionStatus::Completed;
        info!("Session completed: session_id={}", session_id);

        Ok(())
    }

    /// Get the status of a session.
    pub async fn get_session_status(&self, session_id: &str) -> Option<P2pSessionStatus> {
        // Check pending first
        {
            let pending = self.pending.read().await;
            if pending.contains_key(session_id) {
                return Some(P2pSessionStatus::Proposed);
            }
        }

        // Check active
        {
            let active = self.active.read().await;
            if let Some(session) = active.get(session_id) {
                return Some(session.status);
            }
        }

        None
    }

    /// Get the grant for a session.
    pub async fn get_session_grant(&self, session_id: &str) -> Option<SigningGrant> {
        // Check pending
        {
            let pending = self.pending.read().await;
            if let Some(session) = pending.get(session_id) {
                return Some(session.grant.clone());
            }
        }

        // Check active
        {
            let active = self.active.read().await;
            if let Some(session) = active.get(session_id) {
                return Some(session.grant.clone());
            }
        }

        None
    }

    /// Get participants for an active session.
    pub async fn get_session_participants(&self, session_id: &str) -> Option<Vec<u16>> {
        let active = self.active.read().await;
        active.get(session_id).map(|s| s.participants.clone())
    }

    /// Clean up expired pending sessions.
    pub async fn cleanup_expired(&self) {
        let now = Instant::now();

        let mut pending = self.pending.write().await;
        let expired: Vec<String> = pending
            .iter()
            .filter(|(_, session)| {
                now.duration_since(session.created_at) > self.config.proposal_timeout
            })
            .map(|(id, _)| id.clone())
            .collect();

        for session_id in expired {
            if let Some(session) = pending.remove(&session_id) {
                warn!(
                    "Cleaned up expired pending session: session_id={}, age={:?}",
                    session_id,
                    now.duration_since(session.created_at)
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn create_test_keys() -> (SigningKey, VerifyingKey) {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        (signing_key, verifying_key)
    }

    fn create_test_grant(signing_key: &SigningKey, participants: Vec<u16>) -> SigningGrant {
        SigningGrant::new(
            "test-wallet".to_string(),
            [0u8; 32],
            (participants.len() as u16).min(3),
            participants,
            signing_key,
        )
    }

    #[tokio::test]
    async fn test_validate_grant() {
        let (signing_key, verifying_key) = create_test_keys();
        let grant = create_test_grant(&signing_key, vec![0, 1, 2]);

        let coordinator = P2pSessionCoordinator::new(0, verifying_key);

        // Should validate successfully
        assert!(coordinator.validate_grant(&grant).is_ok());
    }

    #[tokio::test]
    async fn test_validate_grant_wrong_key() {
        let (signing_key, _) = create_test_keys();
        let (_, wrong_verifying_key) = create_test_keys();

        let grant = create_test_grant(&signing_key, vec![0, 1, 2]);
        let coordinator = P2pSessionCoordinator::new(0, wrong_verifying_key);

        // Should fail validation
        let result = coordinator.validate_grant(&grant);
        assert!(matches!(result, Err(CoordinatorError::InvalidGrant)));
    }

    #[tokio::test]
    async fn test_validate_grant_not_participant() {
        let (signing_key, verifying_key) = create_test_keys();
        let grant = create_test_grant(&signing_key, vec![1, 2, 3]); // Party 0 not included

        let coordinator = P2pSessionCoordinator::new(0, verifying_key);

        let result = coordinator.validate_grant(&grant);
        assert!(matches!(result, Err(CoordinatorError::NotParticipant)));
    }

    #[tokio::test]
    async fn test_is_initiator() {
        let (signing_key, verifying_key) = create_test_keys();
        let grant = create_test_grant(&signing_key, vec![0, 1, 2]);

        let expected_initiator = select_initiator_from_grant(&grant);

        let coord_match = P2pSessionCoordinator::new(expected_initiator, verifying_key);
        let coord_nomatch = P2pSessionCoordinator::new((expected_initiator + 1) % 3, verifying_key);

        assert!(coord_match.is_initiator(&grant));
        assert!(!coord_nomatch.is_initiator(&grant));
    }

    #[tokio::test]
    async fn test_prepare_proposal() {
        let (signing_key, verifying_key) = create_test_keys();
        let grant = create_test_grant(&signing_key, vec![0, 1, 2]);

        let initiator = select_initiator_from_grant(&grant);
        let coordinator = P2pSessionCoordinator::new(initiator, verifying_key);

        let result = coordinator.prepare_proposal(grant.clone(), "cggmp24").await;
        assert!(result.is_ok());

        let (proposal, _rx) = result.unwrap();
        assert_eq!(proposal.initiator_party, initiator);
        assert_eq!(proposal.protocol, "cggmp24");
        assert_eq!(proposal.session_id(), grant.session_id());
    }

    #[tokio::test]
    async fn test_prepare_proposal_not_initiator() {
        let (signing_key, verifying_key) = create_test_keys();
        let grant = create_test_grant(&signing_key, vec![0, 1, 2]);

        let initiator = select_initiator_from_grant(&grant);
        let wrong_party = if initiator == 0 { 1 } else { 0 };

        let coordinator = P2pSessionCoordinator::new(wrong_party, verifying_key);

        let result = coordinator.prepare_proposal(grant, "cggmp24").await;
        assert!(matches!(result, Err(CoordinatorError::NotInitiator)));
    }

    #[tokio::test]
    async fn test_handle_proposal_as_participant() {
        let (signing_key, verifying_key) = create_test_keys();
        let grant = create_test_grant(&signing_key, vec![0, 1, 2]);

        let initiator = select_initiator_from_grant(&grant);
        let participant = if initiator == 0 { 1 } else { 0 };

        let coordinator = P2pSessionCoordinator::new(participant, verifying_key);

        let proposal = SessionProposal::new(grant.clone(), initiator, "cggmp24");
        let ack = coordinator.handle_proposal(&proposal).await.unwrap();

        assert!(ack.accepted);
        assert_eq!(ack.party_index, participant);
    }

    #[tokio::test]
    async fn test_handle_ack_reaches_threshold() {
        let (signing_key, verifying_key) = create_test_keys();
        let grant = create_test_grant(&signing_key, vec![0, 1, 2]);

        let initiator = select_initiator_from_grant(&grant);
        let coordinator = P2pSessionCoordinator::new(initiator, verifying_key);

        // Prepare proposal
        let (proposal, rx) = coordinator
            .prepare_proposal(grant.clone(), "cggmp24")
            .await
            .unwrap();

        let session_id = proposal.session_id();

        // Send acks from other participants
        for party in &grant.participants {
            if *party != initiator {
                let ack = SessionAck::accept(session_id.clone(), *party);
                coordinator.handle_ack(ack).await.unwrap();
            }
        }

        // Threshold notification should have been sent
        let confirmed = rx.await.unwrap();
        assert!(confirmed.len() >= grant.threshold as usize);
    }

    #[tokio::test]
    async fn test_session_status_tracking() {
        let (signing_key, verifying_key) = create_test_keys();
        let grant = create_test_grant(&signing_key, vec![0, 1, 2]);

        let initiator = select_initiator_from_grant(&grant);
        let coordinator = P2pSessionCoordinator::new(initiator, verifying_key);

        let (_proposal, _rx) = coordinator
            .prepare_proposal(grant.clone(), "cggmp24")
            .await
            .unwrap();

        let session_id = grant.session_id();

        // Should be in Proposed state
        let status = coordinator.get_session_status(&session_id).await;
        assert_eq!(status, Some(P2pSessionStatus::Proposed));
    }
}
