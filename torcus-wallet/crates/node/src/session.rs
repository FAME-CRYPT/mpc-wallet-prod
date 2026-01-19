//! Session state management for MPC signing operations.
//!
//! This module implements the node-side session state machine that tracks
//! active, completed, and failed signing sessions. It enforces invariants
//! like replay protection, session uniqueness, and timeout handling.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::RwLock;
use tracing::{debug, info, trace, warn};
use uuid::Uuid;

use crate::state::NodeState;

/// Maximum number of concurrent sessions per wallet.
pub const MAX_SESSIONS_PER_WALLET: usize = 3;

/// Maximum total concurrent sessions.
pub const MAX_TOTAL_SESSIONS: usize = 10;

/// Session timeout (total).
pub const SESSION_TIMEOUT_SECS: u64 = 120;

/// How long to retain completed/failed session records for replay protection.
pub const SESSION_RETENTION_SECS: u64 = 600; // 10 minutes

/// Maximum entries in the replay cache.
pub const MAX_REPLAY_CACHE_ENTRIES: usize = 1000;

/// Interval between cleanup runs (in seconds).
pub const CLEANUP_INTERVAL_SECS: u64 = 30;

/// Per-round timeout (in seconds).
pub const ROUND_TIMEOUT_SECS: u64 = 30;

/// Idle timeout - no messages received (in seconds).
pub const IDLE_TIMEOUT_SECS: u64 = 60;

/// Tracks the state of a single protocol round.
#[derive(Debug, Clone)]
pub struct RoundState {
    /// Round number.
    pub round: u16,
    /// When this round started.
    pub started_at: Instant,
    /// Parties that have sent messages in this round.
    pub messages_from: std::collections::HashSet<u16>,
    /// Expected number of messages (threshold or num_parties depending on protocol phase).
    pub expected_messages: usize,
}

impl RoundState {
    /// Create a new round state.
    pub fn new(round: u16, expected_messages: usize) -> Self {
        Self {
            round,
            started_at: Instant::now(),
            messages_from: std::collections::HashSet::new(),
            expected_messages,
        }
    }

    /// Check if this round has timed out.
    pub fn is_timed_out(&self) -> bool {
        self.started_at.elapsed() > Duration::from_secs(ROUND_TIMEOUT_SECS)
    }

    /// Check if we've received a message from this party in this round.
    pub fn has_message_from(&self, party: u16) -> bool {
        self.messages_from.contains(&party)
    }

    /// Record a message from a party. Returns false if duplicate.
    pub fn record_message(&mut self, party: u16) -> bool {
        self.messages_from.insert(party)
    }

    /// Check if we have all expected messages for this round.
    #[allow(dead_code)]
    pub fn is_complete(&self) -> bool {
        self.messages_from.len() >= self.expected_messages
    }
}

/// Session state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionState {
    /// Session is actively running.
    InProgress,
    /// Session completed successfully.
    Completed,
    /// Session failed.
    Failed { reason: String },
}

/// A signing session tracked by the node.
#[derive(Debug, Clone)]
pub struct SigningSession {
    /// Session ID (deterministic from grant).
    pub session_id: String,
    /// Grant ID that authorized this session.
    #[allow(dead_code)]
    pub grant_id: Uuid,
    /// Wallet being signed for.
    pub wallet_id: String,
    /// Protocol type (cggmp24, frost).
    #[allow(dead_code)]
    pub protocol: String,
    /// Current state.
    pub state: SessionState,
    /// When the session was created.
    pub created_at: Instant,
    /// When the session state last changed.
    pub updated_at: Instant,
    /// When the session completed (for retention timing).
    /// Set when state transitions to Completed or Failed.
    pub completed_at: Option<Instant>,
    /// Cached signature (if completed successfully).
    pub signature: Option<Vec<u8>>,
    /// Number of participants in this session.
    pub num_participants: usize,
    /// Current round state (if in progress).
    pub current_round: Option<RoundState>,
    /// Last time a message was received (for idle timeout).
    pub last_message_at: Instant,
}

impl SigningSession {
    /// Create a new in-progress session.
    pub fn new(
        session_id: String,
        grant_id: Uuid,
        wallet_id: String,
        protocol: String,
        num_participants: usize,
    ) -> Self {
        let now = Instant::now();
        Self {
            session_id,
            grant_id,
            wallet_id,
            protocol,
            state: SessionState::InProgress,
            created_at: now,
            updated_at: now,
            completed_at: None,
            signature: None,
            num_participants,
            current_round: Some(RoundState::new(0, num_participants)),
            last_message_at: now,
        }
    }

    /// Check if the session has timed out (total session timeout).
    pub fn is_timed_out(&self) -> bool {
        self.created_at.elapsed() > Duration::from_secs(SESSION_TIMEOUT_SECS)
    }

    /// Check if the session is idle (no messages received recently).
    pub fn is_idle(&self) -> bool {
        self.last_message_at.elapsed() > Duration::from_secs(IDLE_TIMEOUT_SECS)
    }

    /// Check if the current round has timed out.
    pub fn is_round_timed_out(&self) -> bool {
        self.current_round
            .as_ref()
            .map(|r| r.is_timed_out())
            .unwrap_or(false)
    }

    /// Check if the session is still in progress.
    pub fn is_in_progress(&self) -> bool {
        matches!(self.state, SessionState::InProgress)
    }

    /// Check if the session should be retained (for replay protection).
    ///
    /// For completed/failed sessions, retention is measured from completion time.
    /// For in-progress sessions, retention is measured from creation time.
    /// This ensures long-running sessions get full retention after completion.
    pub fn should_retain(&self) -> bool {
        let elapsed = match self.completed_at {
            Some(completed) => completed.elapsed(),
            None => self.created_at.elapsed(),
        };
        elapsed < Duration::from_secs(SESSION_RETENTION_SECS)
    }

    /// Get the current round number.
    pub fn current_round_number(&self) -> Option<u16> {
        self.current_round.as_ref().map(|r| r.round)
    }

    /// Record a message received from a party.
    ///
    /// Returns:
    /// - `Ok(true)` if this is a new message (not a duplicate)
    /// - `Ok(false)` if this is a duplicate message from the same party in the same round
    /// - `Err(reason)` if the message is invalid (wrong round, etc.)
    pub fn record_message(&mut self, from_party: u16, round: u16) -> Result<bool, String> {
        // Update last message time
        self.last_message_at = Instant::now();

        let current = self
            .current_round
            .as_mut()
            .ok_or_else(|| "No active round".to_string())?;

        // Check round number
        if round < current.round {
            // Message from a past round - ignore but don't error
            debug!(
                "Ignoring message from past round {} (current: {})",
                round, current.round
            );
            return Ok(false);
        }

        if round > current.round {
            // Message from a future round - this shouldn't happen in normal operation
            // but we'll buffer it by advancing the round
            warn!(
                "Received message from future round {} (current: {}), advancing",
                round, current.round
            );
            self.advance_to_round(round);
            return self.record_message(from_party, round);
        }

        // Same round - check for duplicates
        if current.has_message_from(from_party) {
            warn!(
                "Duplicate message from party {} in round {}",
                from_party, round
            );
            return Ok(false);
        }

        // Record the message
        current.record_message(from_party);
        debug!(
            "Recorded message from party {} in round {} ({}/{} messages)",
            from_party,
            round,
            current.messages_from.len(),
            current.expected_messages
        );

        Ok(true)
    }

    /// Advance to a new round.
    pub fn advance_to_round(&mut self, round: u16) {
        info!(
            "Advancing session {} to round {} (was {:?})",
            self.session_id,
            round,
            self.current_round_number()
        );
        self.current_round = Some(RoundState::new(round, self.num_participants));
        self.updated_at = Instant::now();
    }

    /// Advance to the next round.
    #[allow(dead_code)]
    pub fn advance_round(&mut self) {
        let next_round = self.current_round_number().map(|r| r + 1).unwrap_or(0);
        self.advance_to_round(next_round);
    }

    /// Check if the current round is complete (all expected messages received).
    #[allow(dead_code)]
    pub fn is_round_complete(&self) -> bool {
        self.current_round
            .as_ref()
            .map(|r| r.is_complete())
            .unwrap_or(false)
    }

    /// Mark session as completed with signature.
    pub fn complete(&mut self, signature: Vec<u8>) {
        let now = Instant::now();
        self.state = SessionState::Completed;
        self.signature = Some(signature);
        self.current_round = None;
        self.updated_at = now;
        self.completed_at = Some(now);
    }

    /// Mark session as failed.
    pub fn fail(&mut self, reason: String) {
        let now = Instant::now();
        self.state = SessionState::Failed { reason };
        self.current_round = None;
        self.updated_at = now;
        self.completed_at = Some(now);
    }
}

/// Session manager for tracking signing sessions and replay protection.
#[derive(Debug)]
pub struct SessionManager {
    /// Active and recently completed sessions.
    sessions: HashMap<String, SigningSession>,
    /// Replay cache: grant_id -> expiry timestamp.
    replay_cache: HashMap<Uuid, u64>,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    /// Create a new session manager.
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            replay_cache: HashMap::new(),
        }
    }

    /// Check if a grant has already been used (replay protection).
    pub fn is_grant_replayed(&self, grant_id: &Uuid) -> bool {
        if let Some(&expiry) = self.replay_cache.get(grant_id) {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            // Grant is replayed if it's in cache and hasn't expired yet
            expiry > now
        } else {
            false
        }
    }

    /// Record a grant as used (for replay protection).
    pub fn record_grant(&mut self, grant_id: Uuid, expires_at: u64) {
        // Clean up old entries if we're at capacity
        if self.replay_cache.len() >= MAX_REPLAY_CACHE_ENTRIES {
            self.cleanup_replay_cache();
        }
        self.replay_cache.insert(grant_id, expires_at);
    }

    /// Check if a session already exists.
    #[allow(dead_code)]
    pub fn session_exists(&self, session_id: &str) -> bool {
        self.sessions.contains_key(session_id)
    }

    /// Get an existing session.
    pub fn get_session(&self, session_id: &str) -> Option<&SigningSession> {
        self.sessions.get(session_id)
    }

    /// Get a mutable reference to an existing session.
    #[allow(dead_code)]
    pub fn get_session_mut(&mut self, session_id: &str) -> Option<&mut SigningSession> {
        self.sessions.get_mut(session_id)
    }

    /// Count active sessions for a wallet.
    pub fn active_sessions_for_wallet(&self, wallet_id: &str) -> usize {
        self.sessions
            .values()
            .filter(|s| s.wallet_id == wallet_id && s.is_in_progress())
            .count()
    }

    /// Count total active sessions.
    pub fn total_active_sessions(&self) -> usize {
        self.sessions
            .values()
            .filter(|s| s.is_in_progress())
            .count()
    }

    /// Check if we can create a new session for a wallet.
    pub fn can_create_session(&self, wallet_id: &str) -> Result<(), SessionError> {
        let wallet_sessions = self.active_sessions_for_wallet(wallet_id);
        if wallet_sessions >= MAX_SESSIONS_PER_WALLET {
            return Err(SessionError::TooManyWalletSessions {
                wallet_id: wallet_id.to_string(),
                count: wallet_sessions,
                max: MAX_SESSIONS_PER_WALLET,
            });
        }

        let total_sessions = self.total_active_sessions();
        if total_sessions >= MAX_TOTAL_SESSIONS {
            return Err(SessionError::TooManySessions {
                count: total_sessions,
                max: MAX_TOTAL_SESSIONS,
            });
        }

        Ok(())
    }

    /// Create a new session.
    ///
    /// Returns an error if:
    /// - Grant has already been used (replay)
    /// - Session already exists
    /// - Too many active sessions
    pub fn create_session(
        &mut self,
        session_id: String,
        grant_id: Uuid,
        grant_expires_at: u64,
        wallet_id: String,
        protocol: String,
        num_participants: usize,
    ) -> Result<&SigningSession, SessionError> {
        // Check replay
        if self.is_grant_replayed(&grant_id) {
            warn!("Replay detected for grant {}", grant_id);
            return Err(SessionError::GrantReplayed { grant_id });
        }

        // Check if session already exists
        if let Some(existing) = self.sessions.get(&session_id) {
            info!(
                "Session {} already exists in state {:?}",
                session_id, existing.state
            );
            return Err(SessionError::SessionExists {
                session_id,
                state: existing.state.clone(),
            });
        }

        // Check session limits
        self.can_create_session(&wallet_id)?;

        // Record grant for replay protection
        self.record_grant(grant_id, grant_expires_at);

        // Create the session
        let session = SigningSession::new(
            session_id.clone(),
            grant_id,
            wallet_id,
            protocol,
            num_participants,
        );
        self.sessions.insert(session_id.clone(), session);

        debug!(
            "Created session {} with {} participants",
            session_id, num_participants
        );
        Ok(self.sessions.get(&session_id).unwrap())
    }

    /// Mark a session as completed.
    pub fn complete_session(
        &mut self,
        session_id: &str,
        signature: Vec<u8>,
    ) -> Result<(), SessionError> {
        let session =
            self.sessions
                .get_mut(session_id)
                .ok_or_else(|| SessionError::SessionNotFound {
                    session_id: session_id.to_string(),
                })?;

        if !session.is_in_progress() {
            return Err(SessionError::InvalidStateTransition {
                session_id: session_id.to_string(),
                from: session.state.clone(),
                to: SessionState::Completed,
            });
        }

        session.complete(signature);
        info!("Session {} completed", session_id);
        Ok(())
    }

    /// Mark a session as failed.
    pub fn fail_session(&mut self, session_id: &str, reason: String) -> Result<(), SessionError> {
        let session =
            self.sessions
                .get_mut(session_id)
                .ok_or_else(|| SessionError::SessionNotFound {
                    session_id: session_id.to_string(),
                })?;

        if !session.is_in_progress() {
            return Err(SessionError::InvalidStateTransition {
                session_id: session_id.to_string(),
                from: session.state.clone(),
                to: SessionState::Failed {
                    reason: reason.clone(),
                },
            });
        }

        session.fail(reason);
        info!("Session {} failed", session_id);
        Ok(())
    }

    /// Record a message received for a session.
    ///
    /// This tracks round progress and detects duplicates/out-of-order messages.
    /// Returns Ok(true) if this is a new valid message, Ok(false) if duplicate/ignored,
    /// or Err if the session doesn't exist.
    pub fn record_message(
        &mut self,
        session_id: &str,
        from_party: u16,
        round: u16,
    ) -> Result<bool, SessionError> {
        let session =
            self.sessions
                .get_mut(session_id)
                .ok_or_else(|| SessionError::SessionNotFound {
                    session_id: session_id.to_string(),
                })?;

        if !session.is_in_progress() {
            // Session not in progress, ignore message
            return Ok(false);
        }

        match session.record_message(from_party, round) {
            Ok(is_new) => Ok(is_new),
            Err(e) => {
                warn!("Message recording error for session {}: {}", session_id, e);
                Ok(false)
            }
        }
    }

    /// Advance a session to the next round.
    #[allow(dead_code)]
    pub fn advance_round(&mut self, session_id: &str) -> Result<u16, SessionError> {
        let session =
            self.sessions
                .get_mut(session_id)
                .ok_or_else(|| SessionError::SessionNotFound {
                    session_id: session_id.to_string(),
                })?;

        session.advance_round();
        Ok(session.current_round_number().unwrap_or(0))
    }

    /// Check if the current round is complete for a session.
    #[allow(dead_code)]
    pub fn is_round_complete(&self, session_id: &str) -> Result<bool, SessionError> {
        let session =
            self.sessions
                .get(session_id)
                .ok_or_else(|| SessionError::SessionNotFound {
                    session_id: session_id.to_string(),
                })?;

        Ok(session.is_round_complete())
    }

    /// Clean up expired sessions and replay cache entries.
    pub fn cleanup(&mut self) {
        let before_sessions = self.sessions.len();
        let before_cache = self.replay_cache.len();

        // Remove sessions that have timed out and shouldn't be retained
        self.sessions.retain(|id, session| {
            let retain = session.should_retain() || session.is_in_progress();
            if !retain {
                debug!("Removing expired session {}", id);
            }
            retain
        });

        // Time out any in-progress sessions that have exceeded various timeouts
        for (id, session) in self.sessions.iter_mut() {
            if session.is_in_progress() {
                if session.is_timed_out() {
                    warn!("Session {} timed out (total session timeout)", id);
                    session.fail("Session timed out".to_string());
                } else if session.is_idle() {
                    warn!("Session {} timed out (idle - no messages received)", id);
                    session.fail("Session idle timeout - no messages received".to_string());
                } else if session.is_round_timed_out() {
                    let round = session.current_round_number().unwrap_or(0);
                    warn!("Session {} round {} timed out", id, round);
                    session.fail(format!("Round {} timed out", round));
                }
            }
        }

        // Clean up replay cache
        self.cleanup_replay_cache();

        let removed_sessions = before_sessions - self.sessions.len();
        let removed_cache = before_cache - self.replay_cache.len();

        if removed_sessions > 0 || removed_cache > 0 {
            info!(
                "Cleanup: removed {} sessions, {} replay cache entries",
                removed_sessions, removed_cache
            );
        }
    }

    /// Clean up expired replay cache entries.
    fn cleanup_replay_cache(&mut self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        self.replay_cache.retain(|_, &mut expiry| expiry > now);
    }

    /// Get session statistics.
    pub fn stats(&self) -> SessionStats {
        let mut in_progress = 0;
        let mut completed = 0;
        let mut failed = 0;

        for session in self.sessions.values() {
            match session.state {
                SessionState::InProgress => in_progress += 1,
                SessionState::Completed => completed += 1,
                SessionState::Failed { .. } => failed += 1,
            }
        }

        SessionStats {
            in_progress,
            completed,
            failed,
            replay_cache_size: self.replay_cache.len(),
        }
    }
}

/// Session statistics.
#[derive(Debug, Clone)]
pub struct SessionStats {
    pub in_progress: usize,
    pub completed: usize,
    pub failed: usize,
    pub replay_cache_size: usize,
}

/// Session errors.
#[derive(Debug, Clone)]
pub enum SessionError {
    /// Grant has already been used.
    GrantReplayed { grant_id: Uuid },
    /// Session already exists.
    SessionExists {
        session_id: String,
        state: SessionState,
    },
    /// Session not found.
    SessionNotFound { session_id: String },
    /// Too many active sessions for wallet.
    TooManyWalletSessions {
        wallet_id: String,
        count: usize,
        max: usize,
    },
    /// Too many total active sessions.
    TooManySessions { count: usize, max: usize },
    /// Invalid state transition.
    InvalidStateTransition {
        session_id: String,
        from: SessionState,
        to: SessionState,
    },
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GrantReplayed { grant_id } => {
                write!(f, "Grant {} has already been used", grant_id)
            }
            Self::SessionExists { session_id, state } => {
                write!(
                    f,
                    "Session {} already exists in state {:?}",
                    session_id, state
                )
            }
            Self::SessionNotFound { session_id } => {
                write!(f, "Session {} not found", session_id)
            }
            Self::TooManyWalletSessions {
                wallet_id,
                count,
                max,
            } => {
                write!(
                    f,
                    "Too many active sessions for wallet {}: {} >= {}",
                    wallet_id, count, max
                )
            }
            Self::TooManySessions { count, max } => {
                write!(f, "Too many total active sessions: {} >= {}", count, max)
            }
            Self::InvalidStateTransition {
                session_id,
                from,
                to,
            } => {
                write!(
                    f,
                    "Invalid state transition for session {}: {:?} -> {:?}",
                    session_id, from, to
                )
            }
        }
    }
}

impl std::error::Error for SessionError {}

/// Spawn a background task that periodically cleans up expired sessions.
///
/// This task runs every `CLEANUP_INTERVAL_SECS` seconds and:
/// - Times out in-progress sessions that have exceeded `SESSION_TIMEOUT_SECS`
/// - Removes completed/failed sessions older than `SESSION_RETENTION_SECS`
/// - Cleans up expired entries from the replay cache
///
/// The task runs until the application shuts down.
pub fn spawn_cleanup_task(state: Arc<RwLock<NodeState>>) -> tokio::task::JoinHandle<()> {
    info!(
        "Starting session cleanup task (interval: {}s)",
        CLEANUP_INTERVAL_SECS
    );

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(CLEANUP_INTERVAL_SECS));

        // The first tick completes immediately, so skip it
        interval.tick().await;

        loop {
            interval.tick().await;

            trace!("Running session cleanup...");

            let stats_before = {
                let s = state.read().await;
                s.session_manager.stats()
            };

            {
                let mut s = state.write().await;
                s.session_manager.cleanup();
            }

            let stats_after = {
                let s = state.read().await;
                s.session_manager.stats()
            };

            // Log if anything changed
            if stats_before.in_progress != stats_after.in_progress
                || stats_before.completed != stats_after.completed
                || stats_before.failed != stats_after.failed
                || stats_before.replay_cache_size != stats_after.replay_cache_size
            {
                info!(
                    "Session cleanup: in_progress={}->{}, completed={}->{}, failed={}->{}, replay_cache={}->{}",
                    stats_before.in_progress, stats_after.in_progress,
                    stats_before.completed, stats_after.completed,
                    stats_before.failed, stats_after.failed,
                    stats_before.replay_cache_size, stats_after.replay_cache_size
                );
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_session() {
        let mut manager = SessionManager::new();
        let grant_id = Uuid::new_v4();
        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 300;

        let result = manager.create_session(
            "session-1".to_string(),
            grant_id,
            expires_at,
            "wallet-1".to_string(),
            "cggmp24".to_string(),
            3, // num_participants
        );

        assert!(result.is_ok());
        assert!(manager.session_exists("session-1"));

        // Verify round state is initialized
        let session = manager.get_session("session-1").unwrap();
        assert_eq!(session.num_participants, 3);
        assert_eq!(session.current_round_number(), Some(0));
    }

    #[test]
    fn test_replay_protection() {
        let mut manager = SessionManager::new();
        let grant_id = Uuid::new_v4();
        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 300;

        // First use should succeed
        let result = manager.create_session(
            "session-1".to_string(),
            grant_id,
            expires_at,
            "wallet-1".to_string(),
            "cggmp24".to_string(),
            3,
        );
        assert!(result.is_ok());

        // Second use with same grant should fail
        let result = manager.create_session(
            "session-2".to_string(),
            grant_id,
            expires_at,
            "wallet-1".to_string(),
            "cggmp24".to_string(),
            3,
        );
        assert!(matches!(result, Err(SessionError::GrantReplayed { .. })));
    }

    #[test]
    fn test_session_limits() {
        let mut manager = SessionManager::new();
        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 300;

        // Create max sessions for a wallet
        for i in 0..MAX_SESSIONS_PER_WALLET {
            let result = manager.create_session(
                format!("session-{}", i),
                Uuid::new_v4(),
                expires_at,
                "wallet-1".to_string(),
                "cggmp24".to_string(),
                3,
            );
            assert!(result.is_ok());
        }

        // Next session should fail
        let result = manager.create_session(
            "session-overflow".to_string(),
            Uuid::new_v4(),
            expires_at,
            "wallet-1".to_string(),
            "cggmp24".to_string(),
            3,
        );
        assert!(matches!(
            result,
            Err(SessionError::TooManyWalletSessions { .. })
        ));
    }

    #[test]
    fn test_session_completion() {
        let mut manager = SessionManager::new();
        let grant_id = Uuid::new_v4();
        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 300;

        manager
            .create_session(
                "session-1".to_string(),
                grant_id,
                expires_at,
                "wallet-1".to_string(),
                "cggmp24".to_string(),
                3,
            )
            .unwrap();

        let signature = vec![1, 2, 3, 4];
        let result = manager.complete_session("session-1", signature.clone());
        assert!(result.is_ok());

        let session = manager.get_session("session-1").unwrap();
        assert_eq!(session.state, SessionState::Completed);
        assert_eq!(session.signature, Some(signature));
        assert!(session.current_round.is_none()); // Round cleared on completion
    }

    #[test]
    fn test_round_tracking() {
        let mut session = SigningSession::new(
            "test-session".to_string(),
            Uuid::new_v4(),
            "wallet-1".to_string(),
            "cggmp24".to_string(),
            3,
        );

        // Initial state
        assert_eq!(session.current_round_number(), Some(0));
        assert!(!session.is_round_complete());

        // Record messages from parties
        assert!(session.record_message(0, 0).unwrap()); // First message from party 0
        assert!(!session.record_message(0, 0).unwrap()); // Duplicate - returns false
        assert!(session.record_message(1, 0).unwrap()); // First message from party 1
        assert!(session.record_message(2, 0).unwrap()); // First message from party 2

        // Round should be complete now (3 of 3 messages)
        assert!(session.is_round_complete());

        // Advance to next round
        session.advance_round();
        assert_eq!(session.current_round_number(), Some(1));
        assert!(!session.is_round_complete());
    }

    #[test]
    fn test_round_advancement_on_future_message() {
        let mut session = SigningSession::new(
            "test-session".to_string(),
            Uuid::new_v4(),
            "wallet-1".to_string(),
            "cggmp24".to_string(),
            3,
        );

        assert_eq!(session.current_round_number(), Some(0));

        // Receive message from future round - should auto-advance
        assert!(session.record_message(0, 2).unwrap());
        assert_eq!(session.current_round_number(), Some(2));
    }

    #[test]
    fn test_past_round_message_ignored() {
        let mut session = SigningSession::new(
            "test-session".to_string(),
            Uuid::new_v4(),
            "wallet-1".to_string(),
            "cggmp24".to_string(),
            3,
        );

        // Advance to round 2
        session.advance_to_round(2);
        assert_eq!(session.current_round_number(), Some(2));

        // Message from past round should be ignored (returns false, no error)
        assert!(!session.record_message(0, 1).unwrap());
        assert!(!session.record_message(0, 0).unwrap());
    }
}
