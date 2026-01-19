//! Health and status handlers for the MPC node.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use tokio::sync::RwLock;
use tracing::{error, info, trace};

use crate::state::NodeState;

/// Health check endpoint.
///
/// GET /health
pub async fn health_check(State(state): State<Arc<RwLock<NodeState>>>) -> Json<serde_json::Value> {
    trace!("Health check endpoint called");

    let state = state.read().await;

    let response = serde_json::json!({
        "status": "healthy",
        "service": "mpc-node",
        "node_id": state.node_id.to_string(),
        "party_index": state.party_index + 1,
        "cggmp24_keyshares": state.cggmp24_key_shares.len(),
        "cggmp24_ready": state.is_cggmp24_ready()
    });

    Json(response)
}

/// Get pregenerated primes status.
///
/// GET /primes/status
pub async fn primes_status(State(state): State<Arc<RwLock<NodeState>>>) -> Json<serde_json::Value> {
    trace!("Primes status check");

    let s = state.read().await;
    let has_primes = s.has_primes();
    let primes_size = s.get_primes().map(|p| p.len()).unwrap_or(0);

    Json(serde_json::json!({
        "party_index": s.party_index,
        "has_primes": has_primes,
        "primes_size_bytes": primes_size,
        "cggmp24_ready": has_primes
    }))
}

/// Regenerate pregenerated primes.
///
/// POST /primes/regenerate
///
/// Generates new primes and saves them to disk. This is computationally expensive
/// and may take 30-120 seconds.
pub async fn regenerate_primes(
    State(state): State<Arc<RwLock<NodeState>>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let party_index = {
        let s = state.read().await;
        s.party_index
    };

    info!("========================================");
    info!("  REGENERATE PRIMES REQUEST");
    info!("========================================");
    info!("Party index: {}", party_index);

    // Generate new primes (this is slow - 30-120 seconds)
    let start = std::time::Instant::now();

    use protocols::cggmp24::primes::{initialize_primes, PrimeInitResult};
    match initialize_primes(party_index, None, true) {
        PrimeInitResult::Generated(data) => {
            let elapsed = start.elapsed();
            let primes_size = data.len();

            // Update state with new primes
            {
                let mut s = state.write().await;
                s.set_primes(data);
            }

            info!(
                "Primes regenerated successfully in {:.2}s",
                elapsed.as_secs_f64()
            );

            Ok(Json(serde_json::json!({
                "success": true,
                "party_index": party_index,
                "primes_size_bytes": primes_size,
                "generation_time_secs": elapsed.as_secs_f64(),
                "message": "Primes regenerated and saved to disk"
            })))
        }
        PrimeInitResult::Loaded(_) => {
            // This shouldn't happen with force=true, but handle it anyway
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unexpected: loaded primes instead of generating".to_string(),
            ))
        }
        PrimeInitResult::Failed(e) => {
            error!("Failed to regenerate primes: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to regenerate primes: {}", e),
            ))
        }
    }
}

/// Get auxiliary info status.
///
/// GET /aux-info/status
pub async fn aux_info_status(
    State(state): State<Arc<RwLock<NodeState>>>,
) -> Json<serde_json::Value> {
    trace!("Aux info status check");

    let s = state.read().await;
    let status = protocols::cggmp24::aux_info::get_aux_info_status(s.party_index);

    Json(serde_json::json!({
        "party_index": status.party_index,
        "has_aux_info": status.has_aux_info,
        "aux_info_size_bytes": status.aux_info_size_bytes,
        "session_id": status.session_id,
        "generated_at": status.generated_at,
        "has_primes": s.has_primes(),
        "cggmp24_ready": s.is_cggmp24_ready()
    }))
}

/// Get full MPC node status (primes + aux_info + readiness).
///
/// GET /status
pub async fn full_status(State(state): State<Arc<RwLock<NodeState>>>) -> Json<serde_json::Value> {
    let s = state.read().await;
    let aux_status = protocols::cggmp24::aux_info::get_aux_info_status(s.party_index);

    Json(serde_json::json!({
        "node_id": s.node_id.to_string(),
        "party_index": s.party_index,
        "primes": {
            "available": s.has_primes(),
            "size_bytes": s.get_primes().map(|p| p.len()).unwrap_or(0)
        },
        "aux_info": {
            "available": aux_status.has_aux_info,
            "size_bytes": aux_status.aux_info_size_bytes,
            "session_id": aux_status.session_id,
            "generated_at": aux_status.generated_at
        },
        "cggmp24_key_shares": s.cggmp24_key_shares.len(),
        "cggmp24_ready": s.is_cggmp24_ready()
    }))
}

/// Request to start aux_info generation.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct StartAuxInfoRequest {
    pub session_id: String,
    pub num_parties: u16,
}

/// Start aux_info generation on this node.
///
/// POST /aux-info/start
pub async fn start_aux_info(
    State(state): State<Arc<RwLock<NodeState>>>,
    Json(request): Json<StartAuxInfoRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let (party_index, primes_data, has_aux_info) = {
        let s = state.read().await;
        (
            s.party_index,
            s.get_primes().map(|p| p.to_vec()),
            s.has_aux_info(),
        )
    };

    info!("========================================");
    info!("  AUX INFO GENERATION REQUEST");
    info!("========================================");
    info!("Session ID: {}", request.session_id);
    info!("Party index: {}", party_index);
    info!("Num parties: {}", request.num_parties);

    // Check if we already have aux_info
    if has_aux_info {
        info!("Aux info already exists, skipping generation");
        return Ok(Json(serde_json::json!({
            "success": true,
            "party_index": party_index,
            "message": "Aux info already exists",
            "already_exists": true
        })));
    }

    // Check if we have primes
    let primes = match primes_data {
        Some(p) => p,
        None => {
            error!("Cannot generate aux_info: primes not available");
            return Err((
                StatusCode::PRECONDITION_FAILED,
                "Primes not generated. Call POST /primes/regenerate first.".to_string(),
            ));
        }
    };

    info!("Starting aux_info generation in background...");

    // Clone what we need for the background task
    let session_id = request.session_id.clone();
    let num_parties = request.num_parties;
    let state_clone = state.clone();

    // Spawn background task to run the protocol
    tokio::spawn(async move {
        run_aux_info_protocol(state_clone, session_id, party_index, num_parties, primes).await;
    });

    Ok(Json(serde_json::json!({
        "success": true,
        "party_index": party_index,
        "session_id": request.session_id,
        "message": "Aux info generation started"
    })))
}

/// Background task to run the aux_info protocol.
async fn run_aux_info_protocol(
    state: Arc<RwLock<NodeState>>,
    session_id: String,
    party_index: u16,
    num_parties: u16,
    primes_data: Vec<u8>,
) {
    use protocols::cggmp24::aux_info::{self, StoredAuxInfo};
    use protocols::relay::{RelayClient, RelayMessage};

    info!("Aux info protocol task started for session {}", session_id);

    let coordinator_url = "http://mpc-coordinator:3000";
    let client = RelayClient::new(coordinator_url, party_index);

    // Create channels for the protocol
    let (outgoing_tx, outgoing_rx) =
        async_channel::bounded::<protocols::cggmp24::runner::ProtocolMessage>(1000);
    let (incoming_tx, incoming_rx) =
        async_channel::bounded::<protocols::cggmp24::runner::ProtocolMessage>(1000);

    // Spawn task to send outgoing messages to coordinator
    let outgoing_handle = tokio::spawn(async move {
        while let Ok(msg) = outgoing_rx.recv().await {
            let relay_msg = RelayMessage {
                session_id: msg.session_id.clone(),
                protocol: "aux_info".to_string(),
                sender: msg.sender,
                recipient: msg.recipient,
                round: msg.round,
                payload: msg.payload,
                seq: msg.seq,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            };

            if let Err(e) = client.submit_message(relay_msg).await {
                error!("Failed to submit message: {}", e);
            }
        }
    });

    // Spawn task to poll incoming messages from coordinator
    let session_id_for_poll = session_id.clone();
    let poll_client = RelayClient::new(coordinator_url, party_index);

    let polling_handle = tokio::spawn(async move {
        loop {
            match poll_client.poll_messages(&session_id_for_poll).await {
                Ok(response) => {
                    if !response.session_active {
                        info!("Session no longer active, stopping poll");
                        break;
                    }

                    for msg in response.messages {
                        let protocol_msg = protocols::cggmp24::runner::ProtocolMessage {
                            session_id: msg.session_id,
                            sender: msg.sender,
                            recipient: msg.recipient,
                            round: msg.round,
                            payload: msg.payload,
                            seq: msg.seq,
                        };

                        if let Err(e) = incoming_tx.send(protocol_msg).await {
                            error!("Failed to forward message: {}", e);
                            break;
                        }
                    }
                }
                Err(e) => {
                    tracing::debug!("Poll error (may be expected during startup): {}", e);
                }
            }

            // Poll interval
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    });

    // Run the actual protocol
    let result = protocols::cggmp24::runner::run_aux_info_gen(
        party_index,
        num_parties,
        &session_id,
        &primes_data,
        incoming_rx,
        outgoing_tx,
    )
    .await;

    // Let the outgoing queue drain before stopping
    info!("Protocol completed, waiting for message queue to drain...");
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Stop tasks
    polling_handle.abort();
    outgoing_handle.abort();

    // Handle result
    let notify_client = RelayClient::new(coordinator_url, party_index);

    if result.success {
        if let Some(aux_info_data) = result.aux_info_data {
            // Save to disk
            let stored = StoredAuxInfo {
                version: StoredAuxInfo::CURRENT_VERSION,
                party_index,
                num_parties,
                aux_info_data: aux_info_data.clone(),
                session_id: session_id.clone(),
                generated_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            };

            if let Err(e) =
                aux_info::save_aux_info(&aux_info::default_aux_info_path(party_index), &stored)
            {
                error!("Failed to save aux_info: {}", e);
            }

            // Update state
            {
                let mut s = state.write().await;
                s.set_aux_info(aux_info_data.clone());
            }

            info!("Aux info saved successfully");

            // Notify coordinator
            let _ = notify_client
                .notify_aux_info_complete(&session_id, true, aux_info_data.len(), None)
                .await;
        }
    } else {
        error!("Aux info generation failed: {:?}", result.error);
        let _ = notify_client
            .notify_aux_info_complete(&session_id, false, 0, result.error)
            .await;
    }
}
