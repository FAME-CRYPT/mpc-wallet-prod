//! Message Relay Handlers for MPC Protocols
//!
//! The coordinator acts as a central message relay for MPC protocol execution.
//! This simplifies networking compared to peer-to-peer approaches like libp2p.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::handlers::reject_mainnet;
use crate::state::CoordinatorState;
use common::{SigningGrant, StoredRelaySession, NUM_PARTIES, THRESHOLD};

/// Protocol message for relay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayMessage {
    pub session_id: String,
    pub protocol: String,
    pub sender: u16,
    pub recipient: Option<u16>,
    pub round: u16,
    pub payload: Vec<u8>,
    pub seq: u64,
    pub timestamp: u64,
}

/// Request to submit a protocol message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitMessageRequest {
    pub message: RelayMessage,
}

/// Response with pending messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollMessagesResponse {
    pub messages: Vec<RelayMessage>,
    pub session_active: bool,
}

/// Protocol session info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolSessionInfo {
    pub session_id: String,
    pub protocol: String,
    pub num_parties: u16,
    pub started_at: u64,
    pub status: String,
    pub parties_ready: Vec<u16>,
    pub parties_completed: Vec<u16>,
}

/// Start aux_info generation request (from CLI/external)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartAuxInfoRequest {
    pub session_id: Option<String>,
}

/// Start aux_info generation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartAuxInfoResponse {
    pub success: bool,
    pub session_id: String,
    pub message: String,
}

/// Aux info generation completion notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxInfoCompleteRequest {
    pub session_id: String,
    pub party_index: u16,
    pub success: bool,
    pub aux_info_size: usize,
    pub error: Option<String>,
}

/// Maximum time-to-live for a relay session (30 minutes)
pub const SESSION_TTL_SECS: u64 = 30 * 60;

/// Maximum messages per party queue
pub const MAX_MESSAGES_PER_PARTY: usize = 1000;

/// Maximum number of concurrent relay sessions
pub const MAX_RELAY_SESSIONS: usize = 100;

/// Relay session state
#[derive(Debug, Clone)]
pub struct RelaySession {
    pub session_id: String,
    pub protocol: String,
    pub num_parties: u16,
    /// The actual party IDs participating in this session
    pub parties: Vec<u16>,
    pub started_at: u64,
    /// Last activity timestamp (for TTL enforcement)
    pub last_activity: u64,
    /// Messages queued for each party
    pub message_queues: HashMap<u16, Vec<RelayMessage>>,
    /// Parties that have joined
    pub parties_ready: Vec<u16>,
    /// Parties that have completed
    pub parties_completed: Vec<u16>,
    /// Whether session is active
    pub active: bool,
    /// Error message if failed
    pub error: Option<String>,
}

/// Error type for add_message failures
#[derive(Debug)]
pub enum AddMessageError {
    InvalidSender(u16),
    InvalidRecipient(u16),
    QueueFull(u16),
}

impl RelaySession {
    /// Create a new relay session for the given parties.
    ///
    /// # Arguments
    /// * `session_id` - Unique session identifier
    /// * `protocol` - Protocol name (e.g., "signing", "keygen")
    /// * `parties` - List of unique party IDs participating in this session
    ///
    /// # Errors
    /// Returns an error if `parties` is empty or contains duplicates.
    pub fn new(session_id: String, protocol: String, parties: Vec<u16>) -> Result<Self, String> {
        if parties.is_empty() {
            return Err("Cannot create relay session with empty party set".to_string());
        }

        // Check for duplicates
        let mut seen = std::collections::HashSet::new();
        for &party in &parties {
            if !seen.insert(party) {
                return Err(format!("Duplicate party ID: {}", party));
            }
        }

        let mut message_queues = HashMap::new();
        for &party_id in &parties {
            message_queues.insert(party_id, Vec::new());
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(Self {
            session_id,
            protocol,
            num_parties: parties.len() as u16,
            parties,
            started_at: now,
            last_activity: now,
            message_queues,
            parties_ready: Vec::new(),
            parties_completed: Vec::new(),
            active: true,
            error: None,
        })
    }

    /// Check if a party is a valid participant in this session
    pub fn is_valid_party(&self, party: u16) -> bool {
        self.parties.contains(&party)
    }

    /// Check if this session has expired based on TTL
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.last_activity) > SESSION_TTL_SECS
    }

    /// Update the last activity timestamp
    pub fn touch(&mut self) {
        self.last_activity = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// Add a message to the relay.
    /// Returns Ok(()) on success, or an error indicating invalid sender/recipient/queue full.
    /// Note: Only successful enqueues update the session's last_activity timestamp.
    /// Rejected messages (invalid sender/recipient, full queues) do not extend session TTL.
    pub fn add_message(&mut self, msg: RelayMessage) -> Result<(), AddMessageError> {
        // Validate sender is a session participant
        if !self.is_valid_party(msg.sender) {
            warn!(
                "Rejecting message from invalid sender {} (valid parties: {:?})",
                msg.sender, self.parties
            );
            return Err(AddMessageError::InvalidSender(msg.sender));
        }

        if let Some(recipient) = msg.recipient {
            // Validate recipient is a session participant
            if !self.is_valid_party(recipient) {
                warn!(
                    "Rejecting message to invalid recipient {} (valid parties: {:?})",
                    recipient, self.parties
                );
                return Err(AddMessageError::InvalidRecipient(recipient));
            }

            // P2P message - only to recipient
            if let Some(queue) = self.message_queues.get_mut(&recipient) {
                // Check queue capacity
                if queue.len() >= MAX_MESSAGES_PER_PARTY {
                    warn!(
                        "Queue full for party {} (limit: {})",
                        recipient, MAX_MESSAGES_PER_PARTY
                    );
                    return Err(AddMessageError::QueueFull(recipient));
                }
                debug!(
                    "Queuing P2P message from {} to {}, seq={}",
                    msg.sender, recipient, msg.seq
                );
                queue.push(msg);
            }
        } else {
            // Broadcast - to all except sender
            let sender = msg.sender;

            // Pre-check all queues before enqueueing to avoid partial delivery
            for (party, queue) in self.message_queues.iter() {
                if *party != sender && queue.len() >= MAX_MESSAGES_PER_PARTY {
                    warn!(
                        "Queue full for party {} during broadcast pre-check (limit: {})",
                        party, MAX_MESSAGES_PER_PARTY
                    );
                    return Err(AddMessageError::QueueFull(*party));
                }
            }

            // All queues have capacity, now enqueue
            for (party, queue) in self.message_queues.iter_mut() {
                if *party != sender {
                    debug!(
                        "Queuing broadcast from {} to {}, seq={}",
                        sender, party, msg.seq
                    );
                    queue.push(msg.clone());
                }
            }
        }

        // Only update activity timestamp after successful enqueue
        // Rejected messages should not extend session TTL
        self.touch();

        Ok(())
    }

    /// Get and clear messages for a party.
    /// Returns None if the party is not a valid participant.
    pub fn pop_messages(&mut self, party: u16) -> Option<Vec<RelayMessage>> {
        if !self.is_valid_party(party) {
            warn!(
                "Rejecting poll from invalid party {} (valid parties: {:?})",
                party, self.parties
            );
            return None;
        }

        // Update activity timestamp
        self.touch();

        Some(
            self.message_queues
                .get_mut(&party)
                .map(std::mem::take)
                .unwrap_or_default(),
        )
    }

    /// Mark a party as ready
    pub fn mark_ready(&mut self, party: u16) {
        if !self.parties_ready.contains(&party) {
            self.parties_ready.push(party);
            self.parties_ready.sort();
        }
    }

    /// Mark a party as completed
    pub fn mark_completed(&mut self, party: u16) {
        if !self.parties_completed.contains(&party) {
            self.parties_completed.push(party);
            self.parties_completed.sort();
        }
    }

    /// Check if all parties are ready
    #[allow(dead_code)]
    pub fn all_ready(&self) -> bool {
        self.parties_ready.len() >= self.num_parties as usize
    }

    /// Check if all parties have completed
    pub fn all_completed(&self) -> bool {
        self.parties_completed.len() >= self.num_parties as usize
    }

    /// Convert to storable format for persistence.
    pub fn to_stored(&self) -> StoredRelaySession {
        // Serialize message queues to JSON
        let message_queues_json =
            serde_json::to_string(&self.message_queues).unwrap_or_else(|_| "{}".to_string());

        StoredRelaySession {
            session_id: self.session_id.clone(),
            protocol: self.protocol.clone(),
            parties: self.parties.clone(),
            started_at: self.started_at,
            last_activity: self.last_activity,
            message_queues_json,
            parties_ready: self.parties_ready.clone(),
            parties_completed: self.parties_completed.clone(),
            active: self.active,
            error: self.error.clone(),
        }
    }

    /// Restore from stored format.
    pub fn from_stored(stored: StoredRelaySession) -> Result<Self, String> {
        // Deserialize message queues from JSON
        let message_queues: HashMap<u16, Vec<RelayMessage>> =
            serde_json::from_str(&stored.message_queues_json)
                .map_err(|e| format!("Failed to deserialize message queues: {}", e))?;

        Ok(Self {
            session_id: stored.session_id,
            protocol: stored.protocol,
            num_parties: stored.parties.len() as u16,
            parties: stored.parties,
            started_at: stored.started_at,
            last_activity: stored.last_activity,
            message_queues,
            parties_ready: stored.parties_ready,
            parties_completed: stored.parties_completed,
            active: stored.active,
            error: stored.error,
        })
    }
}

/// Submit a message to the relay
///
/// POST /relay/submit
pub async fn submit_message(
    State(state): State<Arc<RwLock<CoordinatorState>>>,
    Json(request): Json<SubmitMessageRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let msg = request.message;
    debug!(
        "Relay submit: session={}, sender={}, recipient={:?}, seq={}",
        msg.session_id, msg.sender, msg.recipient, msg.seq
    );

    let mut s = state.write().await;

    // Clean up expired sessions periodically
    s.cleanup_expired_sessions();

    // Check if session exists
    let session_exists = s.relay_sessions.contains_key(&msg.session_id);

    if !session_exists {
        // Check session limit before creating new session
        if let Err(e) = s.can_create_session() {
            return Err((StatusCode::SERVICE_UNAVAILABLE, e));
        }
        info!("Creating new relay session: {}", msg.session_id);
        // Default to all parties (0..NUM_PARTIES) when session is auto-created
        let default_parties: Vec<u16> = (0..NUM_PARTIES).collect();
        // Safe: 0..NUM_PARTIES is always unique
        let session = RelaySession::new(
            msg.session_id.clone(),
            msg.protocol.clone(),
            default_parties,
        )
        .expect("default parties are always valid");
        // Persist the new session
        s.persist_session(&session);
        s.relay_sessions.insert(msg.session_id.clone(), session);
    }

    // Check if session is expired and remove immediately if so
    let is_expired = s
        .relay_sessions
        .get(&msg.session_id)
        .map(|session| session.is_expired())
        .unwrap_or(false);

    if is_expired {
        s.relay_sessions.remove(&msg.session_id);
        info!("Removed expired session {} during submit", msg.session_id);
        return Err((StatusCode::GONE, "Session has expired".to_string()));
    }

    let session = s.relay_sessions.get_mut(&msg.session_id).unwrap();

    if !session.active {
        return Err((StatusCode::BAD_REQUEST, "Session is not active".to_string()));
    }

    // Validate sender/recipient and add message
    if let Err(e) = session.add_message(msg.clone()) {
        let error_msg = match e {
            AddMessageError::InvalidSender(id) => {
                format!("Invalid sender {} for this session", id)
            }
            AddMessageError::InvalidRecipient(id) => {
                format!("Invalid recipient {} for this session", id)
            }
            AddMessageError::QueueFull(id) => {
                format!(
                    "Message queue full for party {} (limit: {})",
                    id, MAX_MESSAGES_PER_PARTY
                )
            }
        };
        return Err((StatusCode::BAD_REQUEST, error_msg));
    }

    // Mark sender as ready
    session.mark_ready(msg.sender);

    // Clone session data for persistence
    let stored = session.to_stored();

    // Persist session state after message added
    if let Err(e) = s.session_store.save_session(&stored) {
        warn!("Failed to persist session: {}", e);
    }

    Ok(Json(serde_json::json!({
        "success": true
    })))
}

/// Poll for pending messages
///
/// GET /relay/poll/{session_id}/{party_index}
pub async fn poll_messages(
    State(state): State<Arc<RwLock<CoordinatorState>>>,
    Path((session_id, party_index)): Path<(String, u16)>,
) -> Result<Json<PollMessagesResponse>, (StatusCode, String)> {
    debug!("Relay poll: session={}, party={}", session_id, party_index);

    let mut s = state.write().await;

    // Check if session exists and if it's expired
    let is_expired = s
        .relay_sessions
        .get(&session_id)
        .map(|session| session.is_expired())
        .unwrap_or(false);

    if is_expired {
        // Remove the expired session immediately
        s.relay_sessions.remove(&session_id);
        info!("Removed expired session {} during poll", session_id);
        return Err((StatusCode::GONE, "Session has expired".to_string()));
    }

    let session = s.relay_sessions.get_mut(&session_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("Session not found: {}", session_id),
        )
    })?;

    // Validate party and get messages
    let messages = session.pop_messages(party_index).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid party {} for this session", party_index),
        )
    })?;

    // Mark as ready
    session.mark_ready(party_index);

    let session_active = session.active;

    // Clone session data for persistence
    let stored = session.to_stored();

    // Persist session state after messages polled
    if let Err(e) = s.session_store.save_session(&stored) {
        warn!("Failed to persist session: {}", e);
    }

    Ok(Json(PollMessagesResponse {
        messages,
        session_active,
    }))
}

/// Get session status
///
/// GET /relay/session/{session_id}
pub async fn get_session(
    State(state): State<Arc<RwLock<CoordinatorState>>>,
    Path(session_id): Path<String>,
) -> Result<Json<ProtocolSessionInfo>, (StatusCode, String)> {
    let s = state.read().await;

    let session = s.relay_sessions.get(&session_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("Session not found: {}", session_id),
        )
    })?;

    Ok(Json(ProtocolSessionInfo {
        session_id: session.session_id.clone(),
        protocol: session.protocol.clone(),
        num_parties: session.num_parties,
        started_at: session.started_at,
        status: if session.all_completed() {
            "completed".to_string()
        } else if session.active {
            "running".to_string()
        } else {
            "failed".to_string()
        },
        parties_ready: session.parties_ready.clone(),
        parties_completed: session.parties_completed.clone(),
    }))
}

/// List all relay sessions
///
/// GET /relay/sessions
pub async fn list_sessions(
    State(state): State<Arc<RwLock<CoordinatorState>>>,
) -> Json<Vec<ProtocolSessionInfo>> {
    let s = state.read().await;

    let sessions: Vec<_> = s
        .relay_sessions
        .values()
        .map(|session| ProtocolSessionInfo {
            session_id: session.session_id.clone(),
            protocol: session.protocol.clone(),
            num_parties: session.num_parties,
            started_at: session.started_at,
            status: if session.all_completed() {
                "completed".to_string()
            } else if session.active {
                "running".to_string()
            } else {
                "failed".to_string()
            },
            parties_ready: session.parties_ready.clone(),
            parties_completed: session.parties_completed.clone(),
        })
        .collect();

    Json(sessions)
}

/// Start aux_info generation across all nodes
///
/// POST /aux-info/start
///
/// Triggers aux_info generation on all nodes. The nodes will run the
/// MPC protocol in the background using the coordinator as a relay.
pub async fn start_aux_info_gen(
    State(state): State<Arc<RwLock<CoordinatorState>>>,
    Json(request): Json<StartAuxInfoRequest>,
) -> Result<Json<StartAuxInfoResponse>, (StatusCode, String)> {
    let session_id = request
        .session_id
        .unwrap_or_else(|| format!("aux-info-{}", uuid::Uuid::new_v4()));

    info!("========================================");
    info!("  STARTING AUX INFO GENERATION");
    info!("========================================");
    info!("Session ID: {}", session_id);

    // Create relay session for message passing
    {
        let mut s = state.write().await;
        // Check session limit
        if let Err(e) = s.can_create_session() {
            return Err((StatusCode::SERVICE_UNAVAILABLE, e));
        }
        // Safe: hardcoded unique party IDs
        let session =
            RelaySession::new(session_id.clone(), "aux_info".to_string(), vec![0, 1, 2, 3])
                .expect("hardcoded parties are valid");
        s.persist_session(&session);
        s.relay_sessions.insert(session_id.clone(), session);
    }

    // Notify all nodes to start aux_info generation
    let client = reqwest::Client::new();
    let node_urls = {
        let s = state.read().await;
        s.nodes.clone()
    };

    let mut errors = Vec::new();

    for (i, url) in node_urls.iter().enumerate() {
        let start_url = format!("{}/aux-info/start", url);
        info!("Notifying node {} to start aux_info generation", i);

        match client
            .post(&start_url)
            .json(&serde_json::json!({
                "session_id": session_id,
                "num_parties": 4
            }))
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) => {
                if !resp.status().is_success() {
                    let err = format!("Node {} returned error: {}", i, resp.status());
                    error!("{}", err);
                    errors.push(err);
                } else {
                    info!("Node {} acknowledged aux_info start", i);
                }
            }
            Err(e) => {
                let err = format!("Failed to reach node {}: {}", i, e);
                error!("{}", err);
                errors.push(err);
            }
        }
    }

    if errors.is_empty() {
        Ok(Json(StartAuxInfoResponse {
            success: true,
            session_id,
            message: "Aux info generation started on all nodes. Use /relay/session/{session_id} to check status.".to_string(),
        }))
    } else {
        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Some nodes failed to start: {:?}", errors),
        ))
    }
}

/// Handle aux_info completion notification from a node
///
/// POST /relay/aux-info/complete
pub async fn aux_info_complete(
    State(state): State<Arc<RwLock<CoordinatorState>>>,
    Json(request): Json<AuxInfoCompleteRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    info!(
        "Aux info complete notification: session={}, party={}, success={}",
        request.session_id, request.party_index, request.success
    );

    let mut s = state.write().await;

    let session = s
        .relay_sessions
        .get_mut(&request.session_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Session not found: {}", request.session_id),
            )
        })?;

    session.mark_completed(request.party_index);

    if !request.success {
        session.active = false;
        session.error = request.error.clone();
        error!(
            "Party {} failed aux_info generation: {:?}",
            request.party_index, request.error
        );
    }

    let all_done = session.all_completed();
    if all_done {
        info!("========================================");
        info!("  AUX INFO GENERATION COMPLETE");
        info!("========================================");
        info!("All {} parties completed", session.num_parties);
        session.active = false;
    }

    // Clone session data for persistence
    let stored = session.to_stored();

    // Persist session state after completion update
    if let Err(e) = s.session_store.save_session(&stored) {
        warn!("Failed to persist session: {}", e);
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "all_completed": all_done
    })))
}

// ============================================================================
// CGGMP24 Keygen and Signing Endpoints
// ============================================================================

/// Request to start CGGMP24 keygen
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartCggmp24KeygenRequest {
    pub session_id: Option<String>,
    pub wallet_id: String,
    pub threshold: Option<u16>,
}

/// Response for CGGMP24 keygen start
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartCggmp24KeygenResponse {
    pub success: bool,
    pub session_id: String,
    pub wallet_id: String,
    pub message: String,
}

/// Start CGGMP24 key generation across all nodes
///
/// POST /cggmp24/keygen/start
pub async fn start_cggmp24_keygen(
    State(state): State<Arc<RwLock<CoordinatorState>>>,
    Json(request): Json<StartCggmp24KeygenRequest>,
) -> Result<Json<StartCggmp24KeygenResponse>, (StatusCode, String)> {
    reject_mainnet()?;
    let session_id = request
        .session_id
        .unwrap_or_else(|| format!("keygen-{}", uuid::Uuid::new_v4()));
    let threshold = request.threshold.unwrap_or(3); // Default 3-of-4

    info!("========================================");
    info!("  STARTING CGGMP24 KEY GENERATION");
    info!("========================================");
    info!("Session ID: {}", session_id);
    info!("Wallet ID: {}", request.wallet_id);
    info!("Threshold: {}-of-4", threshold);

    // Create relay session for message passing
    {
        let mut s = state.write().await;
        // Check session limit
        if let Err(e) = s.can_create_session() {
            return Err((StatusCode::SERVICE_UNAVAILABLE, e));
        }
        // Safe: hardcoded unique party IDs
        let session = RelaySession::new(session_id.clone(), "keygen".to_string(), vec![0, 1, 2, 3])
            .expect("hardcoded parties are valid");
        s.persist_session(&session);
        s.relay_sessions.insert(session_id.clone(), session);
    }

    // Notify all nodes to start keygen
    let client = reqwest::Client::new();
    let node_urls = {
        let s = state.read().await;
        s.nodes.clone()
    };

    let mut errors = Vec::new();

    for (i, url) in node_urls.iter().enumerate() {
        let start_url = format!("{}/cggmp24/keygen/start", url);
        info!("Notifying node {} to start CGGMP24 keygen", i);

        match client
            .post(&start_url)
            .json(&serde_json::json!({
                "session_id": session_id,
                "wallet_id": request.wallet_id,
                "num_parties": 4,
                "threshold": threshold
            }))
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) => {
                if !resp.status().is_success() {
                    let err = format!("Node {} returned error: {}", i, resp.status());
                    error!("{}", err);
                    errors.push(err);
                } else {
                    info!("Node {} acknowledged CGGMP24 keygen start", i);
                }
            }
            Err(e) => {
                let err = format!("Failed to reach node {}: {}", i, e);
                error!("{}", err);
                errors.push(err);
            }
        }
    }

    if errors.is_empty() {
        Ok(Json(StartCggmp24KeygenResponse {
            success: true,
            session_id,
            wallet_id: request.wallet_id,
            message: "CGGMP24 keygen started on all nodes. Use /relay/session/{session_id} to check status.".to_string(),
        }))
    } else {
        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Some nodes failed to start keygen: {:?}", errors),
        ))
    }
}

/// Check if a node is healthy by calling its /health endpoint.
async fn check_node_health(client: &reqwest::Client, node_url: &str) -> bool {
    let health_url = format!("{}/health", node_url);
    match client
        .get(&health_url)
        .timeout(Duration::from_secs(2))
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

/// Get healthy nodes from the provided list.
/// Returns a vector of (index, url) tuples for healthy nodes.
async fn get_healthy_nodes(client: &reqwest::Client, nodes: &[String]) -> Vec<(u16, String)> {
    let mut healthy = Vec::new();

    // Check all nodes in parallel
    let futures: Vec<_> = nodes
        .iter()
        .enumerate()
        .map(|(i, url)| {
            let client = client.clone();
            let url = url.clone();
            async move {
                let is_healthy = check_node_health(&client, &url).await;
                (i as u16, url, is_healthy)
            }
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    for (index, url, is_healthy) in results {
        if is_healthy {
            healthy.push((index, url));
        } else {
            warn!("Node {} ({}) is unhealthy", index, url);
        }
    }

    healthy
}

/// Select parties from healthy nodes using the specified policy.
///
/// By default uses deterministic selection based on the seed, which ensures
/// the same transaction always selects the same participants (given the same
/// available nodes). This is important for idempotency and replay detection.
///
/// Returns the selected party indices, or an error if not enough healthy nodes.
pub async fn select_parties_with_policy(
    client: &reqwest::Client,
    nodes: &[String],
    count: usize,
    seed: &str,
    policy: common::SelectionPolicy,
) -> Result<common::SelectionResult, String> {
    let healthy = get_healthy_nodes(client, nodes).await;

    if healthy.len() < count {
        return Err(format!(
            "Not enough healthy nodes: need {}, only {} available (healthy: {:?})",
            count,
            healthy.len(),
            healthy.iter().map(|(i, _)| i).collect::<Vec<_>>()
        ));
    }

    let available_nodes: Vec<u16> = healthy.iter().map(|(i, _)| *i).collect();

    let input = common::SelectionInput {
        seed: seed.to_string(),
        available_nodes,
        threshold: count,
        node_scores: None,
        round_robin_counter: None,
    };

    let selector = common::ParticipantSelector::new(policy);
    let result = selector.select(&input).map_err(|e| e.to_string())?;

    info!(
        "Selected parties {:?} from {} healthy nodes (policy: {}, hash: {})",
        result.participants,
        healthy.len(),
        result.policy,
        result.selection_hash
    );

    Ok(result)
}

/// Select parties using deterministic policy (convenience wrapper).
async fn select_parties_deterministic(
    client: &reqwest::Client,
    nodes: &[String],
    count: usize,
    seed: &str,
) -> Result<Vec<u16>, String> {
    let result = select_parties_with_policy(
        client,
        nodes,
        count,
        seed,
        common::SelectionPolicy::Deterministic,
    )
    .await?;
    Ok(result.participants)
}

/// Request to start CGGMP24 signing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartCggmp24SigningRequest {
    pub session_id: Option<String>,
    pub wallet_id: String,
    pub message_hash: String,      // hex-encoded 32-byte hash
    pub parties: Option<Vec<u16>>, // participating party indices (if None, selects randomly from healthy nodes)
}

/// Response for CGGMP24 signing start
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartCggmp24SigningResponse {
    pub success: bool,
    pub session_id: String,
    pub wallet_id: String,
    pub message: String,
}

/// Start CGGMP24 threshold signing across selected nodes
///
/// POST /cggmp24/signing/start
///
/// If `parties` is not provided, uses deterministic selection based on wallet_id
/// and message_hash to select THRESHOLD nodes from healthy ones.
pub async fn start_cggmp24_signing(
    State(state): State<Arc<RwLock<CoordinatorState>>>,
    Json(request): Json<StartCggmp24SigningRequest>,
) -> Result<Json<StartCggmp24SigningResponse>, (StatusCode, String)> {
    reject_mainnet()?;

    // Parse message hash from hex first (needed for deterministic selection)
    let message_hash_bytes: [u8; 32] = hex::decode(&request.message_hash)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("Invalid message_hash hex: {}", e),
            )
        })?
        .try_into()
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "message_hash must be exactly 32 bytes".to_string(),
            )
        })?;

    // Get node URLs from state
    let node_urls = {
        let s = state.read().await;
        s.nodes.clone()
    };

    let client = reqwest::Client::new();

    // Determine parties: use provided list or select deterministically from healthy nodes
    let parties = match request.parties {
        Some(p) => {
            // Validate provided parties list
            if p.is_empty() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Parties list cannot be empty".to_string(),
                ));
            }

            // Check for duplicate party IDs
            {
                let mut seen = std::collections::HashSet::new();
                for &party in &p {
                    if !seen.insert(party) {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            format!("Duplicate party ID: {}", party),
                        ));
                    }
                }
            }

            // Validate all party indices
            for party_index in &p {
                if *party_index as usize >= node_urls.len() {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        format!(
                            "Invalid party index: {} (must be 0-{})",
                            party_index,
                            node_urls.len() - 1
                        ),
                    ));
                }
            }

            p
        }
        None => {
            // Select parties deterministically from healthy nodes
            // This ensures the same transaction always selects the same participants
            info!(
                "No parties specified, selecting {} nodes deterministically",
                THRESHOLD
            );
            let seed = common::derive_selection_seed(&request.wallet_id, &message_hash_bytes);
            select_parties_deterministic(&client, &node_urls, THRESHOLD as usize, &seed)
                .await
                .map_err(|e| (StatusCode::SERVICE_UNAVAILABLE, e))?
        }
    };

    // Create signing grant
    let grant = {
        let s = state.read().await;
        SigningGrant::new(
            request.wallet_id.clone(),
            message_hash_bytes,
            THRESHOLD,
            parties.clone(),
            s.grant_signing_key(),
        )
    };

    // Use grant-derived session ID for determinism
    let session_id = grant.session_id();

    info!("========================================");
    info!("  STARTING CGGMP24 THRESHOLD SIGNING");
    info!("========================================");
    info!("Grant ID: {}", grant.grant_id);
    info!("Session ID: {}", session_id);
    info!("Wallet ID: {}", request.wallet_id);
    info!("Message hash: {}", request.message_hash);
    info!("Signing parties: {:?}", parties);
    info!("Grant expires at: {}", grant.expires_at);

    // Create relay session for message passing (after validation)
    {
        let mut s = state.write().await;
        // Check session limit
        if let Err(e) = s.can_create_session() {
            return Err((StatusCode::SERVICE_UNAVAILABLE, e));
        }
        // Safe: parties validated above (either explicit or from healthy nodes)
        let session = RelaySession::new(session_id.clone(), "signing".to_string(), parties.clone())
            .expect("parties already validated");
        s.persist_session(&session);
        s.relay_sessions.insert(session_id.clone(), session);
    }

    // Notify selected nodes to start signing
    let mut errors = Vec::new();

    for party_index in &parties {
        let url = &node_urls[*party_index as usize];

        let start_url = format!("{}/cggmp24/signing/start", url);
        info!("Notifying node {} to start CGGMP24 signing", party_index);

        match client
            .post(&start_url)
            .json(&serde_json::json!({
                "session_id": session_id,
                "wallet_id": request.wallet_id,
                "message_hash": request.message_hash,
                "parties": parties,
                "grant": grant
            }))
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) => {
                if !resp.status().is_success() {
                    let err = format!("Node {} returned error: {}", party_index, resp.status());
                    error!("{}", err);
                    errors.push(err);
                } else {
                    info!("Node {} acknowledged CGGMP24 signing start", party_index);
                }
            }
            Err(e) => {
                let err = format!("Failed to reach node {}: {}", party_index, e);
                error!("{}", err);
                errors.push(err);
            }
        }
    }

    if errors.is_empty() {
        Ok(Json(StartCggmp24SigningResponse {
            success: true,
            session_id,
            wallet_id: request.wallet_id,
            message: format!(
                "CGGMP24 signing started on {} nodes. Use /relay/session/{{session_id}} to check status.",
                parties.len()
            ),
        }))
    } else {
        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Some nodes failed to start signing: {:?}", errors),
        ))
    }
}

// ============================================================================
// FROST (Taproot/Schnorr) Endpoints
// ============================================================================

/// Request to start FROST keygen.
#[derive(Debug, Deserialize)]
pub struct StartFrostKeygenRequest {
    pub wallet_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub threshold: Option<u16>,
}

/// Response for FROST keygen start.
#[derive(Debug, Serialize)]
pub struct StartFrostKeygenResponse {
    pub success: bool,
    pub session_id: String,
    pub wallet_id: String,
    pub message: String,
}

/// POST /frost/keygen/start
///
/// Start FROST distributed key generation for Taproot wallets.
pub async fn start_frost_keygen(
    State(state): State<Arc<RwLock<CoordinatorState>>>,
    Json(request): Json<StartFrostKeygenRequest>,
) -> Result<Json<StartFrostKeygenResponse>, (StatusCode, String)> {
    reject_mainnet()?;
    let session_id = request
        .session_id
        .unwrap_or_else(|| format!("frost-keygen-{}", uuid::Uuid::new_v4()));
    let threshold = request.threshold.unwrap_or(3); // Default 3-of-4

    info!("========================================");
    info!("  STARTING FROST KEY GENERATION (Taproot)");
    info!("========================================");
    info!("Session ID: {}", session_id);
    info!("Wallet ID: {}", request.wallet_id);
    info!("Threshold: {}-of-4", threshold);

    // Create relay session for message passing
    {
        let mut s = state.write().await;
        // Check session limit
        if let Err(e) = s.can_create_session() {
            return Err((StatusCode::SERVICE_UNAVAILABLE, e));
        }
        // Safe: hardcoded unique party IDs
        let session = RelaySession::new(
            session_id.clone(),
            "frost_keygen".to_string(),
            vec![0, 1, 2, 3],
        )
        .expect("hardcoded parties are valid");
        s.persist_session(&session);
        s.relay_sessions.insert(session_id.clone(), session);
    }

    // Notify all nodes to start FROST keygen
    let client = reqwest::Client::new();
    let node_urls = {
        let s = state.read().await;
        s.nodes.clone()
    };

    let mut errors = Vec::new();

    for (i, url) in node_urls.iter().enumerate() {
        let start_url = format!("{}/frost/keygen/start", url);
        info!("Notifying node {} to start FROST keygen", i);

        match client
            .post(&start_url)
            .json(&serde_json::json!({
                "session_id": session_id,
                "wallet_id": request.wallet_id,
                "num_parties": 4,
                "threshold": threshold
            }))
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) => {
                if !resp.status().is_success() {
                    let err = format!("Node {} returned error: {}", i, resp.status());
                    error!("{}", err);
                    errors.push(err);
                } else {
                    info!("Node {} acknowledged FROST keygen start", i);
                }
            }
            Err(e) => {
                let err = format!("Failed to reach node {}: {}", i, e);
                error!("{}", err);
                errors.push(err);
            }
        }
    }

    if errors.is_empty() {
        Ok(Json(StartFrostKeygenResponse {
            success: true,
            session_id,
            wallet_id: request.wallet_id,
            message: "FROST keygen started on all nodes (Taproot). Use /relay/session/{session_id} to check status.".to_string(),
        }))
    } else {
        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Some nodes failed to start FROST keygen: {:?}", errors),
        ))
    }
}
