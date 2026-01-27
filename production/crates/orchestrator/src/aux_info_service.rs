//! Aux Info Generation Service
//!
//! This service manages the generation and storage of auxiliary information
//! required for CGGMP24 threshold signing. Aux info includes Paillier keys
//! and ring-Pedersen parameters generated through a multi-party protocol.
//!
//! ## Architecture
//!
//! 1. **Primes Generation**: First-time setup generates pregenerated primes (30-120s)
//! 2. **Aux Info Protocol**: Multi-party protocol using primes to generate aux_info
//! 3. **Storage**: Aux info is stored in PostgreSQL for reuse across presignature generations
//!
//! ## Usage Flow
//!
//! ```text
//! 1. Check if primes exist for node → If not, generate (slow, one-time)
//! 2. Check if aux_info exists for session → If not, run MPC protocol
//! 3. Store aux_info in PostgreSQL
//! 4. Use aux_info + incomplete_key_share for presignature generation
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use tracing::{error, info, warn};
use uuid::Uuid;

use threshold_network::QuicEngine;
use threshold_storage::{EtcdStorage, PostgresStorage};
use threshold_types::{DkgMessage, NetworkMessage, NodeId};

use crate::message_router::{MessageRouter, ProtocolMessage as RouterProtocolMessage, ProtocolType as RouterProtocolType};
use protocols::cggmp24::{primes, runner::ProtocolMessage};

/// Result of aux_info generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxInfoResult {
    /// Session ID for the aux_info ceremony
    pub session_id: Uuid,
    /// Party index
    pub party_index: u16,
    /// Number of parties
    pub num_parties: u16,
    /// Whether generation succeeded
    pub success: bool,
    /// Serialized aux_info data (if successful)
    pub aux_info_data: Option<Vec<u8>>,
    /// Error message (if failed)
    pub error: Option<String>,
}

/// Status of aux_info generation ceremony
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AuxInfoStatus {
    /// Ceremony not yet started
    Pending,
    /// Primes generation in progress
    GeneratingPrimes,
    /// Protocol execution in progress
    Running,
    /// Completed successfully
    Completed,
    /// Failed with error
    Failed(String),
}

/// Aux_info ceremony state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxInfoCeremony {
    pub session_id: Uuid,
    pub party_index: u16,
    pub num_parties: u16,
    pub participants: Vec<NodeId>,
    pub status: AuxInfoStatus,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub aux_info_data: Option<Vec<u8>>,
    pub error: Option<String>,
}

/// Aux Info Generation Service
///
/// Manages the lifecycle of aux_info generation ceremonies including:
/// - Primes generation and caching
/// - Multi-party aux_info generation protocol
/// - Storage in PostgreSQL
pub struct AuxInfoService {
    /// PostgreSQL storage
    postgres: Arc<PostgresStorage>,
    /// etcd storage for distributed coordination (with interior mutability)
    etcd: Arc<Mutex<EtcdStorage>>,
    /// QUIC engine for network communication
    quic: Arc<QuicEngine>,
    /// Message router for protocol communication
    message_router: Arc<MessageRouter>,
    /// Current node ID
    node_id: NodeId,
    /// Active aux_info ceremonies (session_id -> ceremony state)
    active_ceremonies: Arc<RwLock<HashMap<Uuid, AuxInfoCeremony>>>,
    /// Message buffer for protocol messages
    message_buffer: Arc<Mutex<HashMap<Uuid, Vec<ProtocolMessage>>>>,
}

impl AuxInfoService {
    /// Create new aux info service
    pub fn new(
        postgres: Arc<PostgresStorage>,
        etcd: Arc<Mutex<EtcdStorage>>,
        quic: Arc<QuicEngine>,
        message_router: Arc<MessageRouter>,
        node_id: NodeId,
    ) -> Self {
        Self {
            postgres,
            etcd,
            quic,
            message_router,
            node_id,
            active_ceremonies: Arc::new(RwLock::new(HashMap::new())),
            message_buffer: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Initiate aux_info generation ceremony
    ///
    /// This will:
    /// 1. Check if primes exist, generate if needed
    /// 2. Create ceremony state in etcd
    /// 3. Run multi-party aux_info generation protocol
    /// 4. Store result in PostgreSQL
    pub async fn initiate_aux_info_gen(
        &self,
        num_parties: u16,
        participants: Vec<NodeId>,
    ) -> Result<AuxInfoResult, String> {
        let session_id = Uuid::new_v4();
        let party_index = self.node_id.0 as u16;

        info!(
            "Initiating aux_info generation ceremony {} with {} parties",
            session_id, num_parties
        );

        // Create ceremony state
        let ceremony = AuxInfoCeremony {
            session_id,
            party_index,
            num_parties,
            participants: participants.clone(),
            status: AuxInfoStatus::Pending,
            started_at: chrono::Utc::now(),
            completed_at: None,
            aux_info_data: None,
            error: None,
        };

        // Store in active ceremonies
        {
            let mut ceremonies = self.active_ceremonies.write().await;
            ceremonies.insert(session_id, ceremony.clone());
        }

        // Step 1: Ensure primes are available
        let primes_data = match self.ensure_primes_available(party_index).await {
            Ok(data) => data,
            Err(e) => {
                error!("Failed to get primes: {}", e);
                self.update_ceremony_status(session_id, AuxInfoStatus::Failed(e.clone()))
                    .await;
                return Err(e);
            }
        };

        // Step 2: Run aux_info generation protocol
        self.update_ceremony_status(session_id, AuxInfoStatus::Running)
            .await;

        let result = self
            .run_aux_info_protocol(session_id, party_index, num_parties, &primes_data, participants.clone())
            .await;

        match result {
            Ok(aux_info_data) => {
                // Step 3: Store aux_info in PostgreSQL
                if let Err(e) = self
                    .store_aux_info(session_id, party_index, &aux_info_data)
                    .await
                {
                    error!("Failed to store aux_info: {}", e);
                    self.update_ceremony_status(
                        session_id,
                        AuxInfoStatus::Failed(format!("Storage failed: {}", e)),
                    )
                    .await;
                    return Err(format!("Failed to store aux_info: {}", e));
                }

                // Update ceremony as completed
                self.update_ceremony_status(session_id, AuxInfoStatus::Completed)
                    .await;
                {
                    let mut ceremonies = self.active_ceremonies.write().await;
                    if let Some(ceremony) = ceremonies.get_mut(&session_id) {
                        ceremony.aux_info_data = Some(aux_info_data.clone());
                        ceremony.completed_at = Some(chrono::Utc::now());
                    }
                }

                Ok(AuxInfoResult {
                    session_id,
                    party_index,
                    num_parties,
                    success: true,
                    aux_info_data: Some(aux_info_data),
                    error: None,
                })
            }
            Err(e) => {
                error!("Aux_info generation failed: {}", e);
                self.update_ceremony_status(session_id, AuxInfoStatus::Failed(e.clone()))
                    .await;
                Ok(AuxInfoResult {
                    session_id,
                    party_index,
                    num_parties,
                    success: false,
                    aux_info_data: None,
                    error: Some(e),
                })
            }
        }
    }

    /// Ensure primes are available (load from disk or generate)
    async fn ensure_primes_available(&self, party_index: u16) -> Result<Vec<u8>, String> {
        info!("Checking for pregenerated primes for party {}", party_index);

        // Try to load from PostgreSQL first (TODO: add PostgreSQL storage for primes)
        // For now, use file-based primes as fallback
        let primes_path = primes::default_primes_path(party_index);

        if primes::primes_exist(&primes_path) {
            info!("Found existing primes file");
            match primes::load_primes(&primes_path) {
                Ok(stored) => {
                    info!("Loaded primes: {} bytes", stored.primes_data.len());
                    return Ok(stored.primes_data);
                }
                Err(e) => {
                    warn!("Failed to load existing primes: {}", e);
                    // Fall through to generation
                }
            }
        }

        // Generate new primes
        info!("No valid primes found, generating new primes (this may take 30-120 seconds)...");
        self.update_ceremony_status(Uuid::nil(), AuxInfoStatus::GeneratingPrimes)
            .await;

        let stored_primes = primes::generate_primes(party_index)
            .map_err(|e| format!("Primes generation failed: {}", e))?;

        // Save to disk
        if let Err(e) = primes::save_primes(&primes_path, &stored_primes) {
            warn!("Failed to save primes to disk: {}", e);
            // Continue anyway, we have the primes in memory
        }

        Ok(stored_primes.primes_data)
    }

    /// Run the aux_info generation protocol with other parties
    async fn run_aux_info_protocol(
        &self,
        session_id: Uuid,
        party_index: u16,
        num_parties: u16,
        primes_data: &[u8],
        participants: Vec<NodeId>,
    ) -> Result<Vec<u8>, String> {
        info!("Starting aux_info generation protocol for session {}", session_id);

        // Register session with message router to get communication channels
        let (outgoing_tx, incoming_rx) = self
            .message_router
            .register_session(
                session_id,
                RouterProtocolType::AuxInfo,
                participants.iter().copied().collect(),
            )
            .await
            .map_err(|e| format!("Failed to register aux_info session: {}", e))?;

        // Convert between ProtocolMessage and protocol-specific message types
        // Spawn adapter task to convert incoming ProtocolMessages to protocol format
        let (protocol_incoming_tx, protocol_incoming_rx) = async_channel::bounded(100);
        tokio::spawn(async move {
            while let Ok(proto_msg) = incoming_rx.recv().await {
                // Deserialize protocol-specific message from payload
                match bincode::deserialize(&proto_msg.payload) {
                    Ok(msg) => {
                        if protocol_incoming_tx.send(msg).await.is_err() {
                            break; // Channel closed
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to deserialize aux_info message: {}", e);
                    }
                }
            }
        });

        // Spawn adapter task to convert outgoing protocol messages to ProtocolMessages
        let (protocol_outgoing_tx, protocol_outgoing_rx) = async_channel::bounded(100);
        let node_id = self.node_id;
        let session_id_clone = session_id;
        let participants_clone = participants.clone();
        tokio::spawn(async move {
            let mut sequence = 0u64;
            while let Ok(msg) = protocol_outgoing_rx.recv().await {
                // Serialize protocol-specific message to payload
                let payload = match bincode::serialize(&msg) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::error!("Failed to serialize aux_info message: {}", e);
                        continue;
                    }
                };

                // Broadcast to all participants
                for &participant in &participants_clone {
                    if participant != node_id {
                        let proto_msg = RouterProtocolMessage {
                            session_id: session_id_clone,
                            from: node_id,
                            to: participant,
                            payload: payload.clone(),
                            sequence,
                            is_broadcast: true, // Broadcast to all participants
                        };
                        if outgoing_tx.send(proto_msg).await.is_err() {
                            tracing::error!("Failed to send message to participant {}", participant);
                        }
                    }
                }
                sequence += 1;
            }
        });

        // Run the protocol using the runner from protocols crate
        let result = protocols::cggmp24::runner::run_aux_info_gen(
            party_index,
            num_parties,
            &session_id.to_string(),
            primes_data,
            protocol_incoming_rx,
            protocol_outgoing_tx,
        )
        .await;

        if result.success {
            result
                .aux_info_data
                .ok_or_else(|| "Aux info generation succeeded but no data returned".to_string())
        } else {
            Err(result
                .error
                .unwrap_or_else(|| "Unknown error".to_string()))
        }
    }

    /// Store aux_info in PostgreSQL
    async fn store_aux_info(
        &self,
        session_id: Uuid,
        party_index: u16,
        aux_info_data: &[u8],
    ) -> Result<(), String> {
        info!(
            "Storing aux_info for session {} party {} ({} bytes)",
            session_id,
            party_index,
            aux_info_data.len()
        );

        self.postgres
            .store_aux_info(session_id, self.node_id, aux_info_data)
            .await
            .map_err(|e| format!("Failed to store aux_info: {}", e))?;

        info!("Aux_info stored successfully in PostgreSQL");
        Ok(())
    }

    /// Update ceremony status
    async fn update_ceremony_status(&self, session_id: Uuid, status: AuxInfoStatus) {
        let mut ceremonies = self.active_ceremonies.write().await;
        if let Some(ceremony) = ceremonies.get_mut(&session_id) {
            ceremony.status = status;
        }
    }

    /// Get aux_info for a specific session
    pub async fn get_aux_info(&self, session_id: Uuid) -> Option<Vec<u8>> {
        // First check active ceremonies
        {
            let ceremonies = self.active_ceremonies.read().await;
            if let Some(ceremony) = ceremonies.get(&session_id) {
                if let Some(data) = &ceremony.aux_info_data {
                    return Some(data.clone());
                }
            }
        }

        // Check PostgreSQL storage
        match self.postgres.get_aux_info(session_id, self.node_id).await {
            Ok(Some(data)) => Some(data),
            Ok(None) => None,
            Err(e) => {
                warn!("Failed to get aux_info from PostgreSQL: {}", e);
                None
            }
        }
    }

    /// Get the latest aux_info for this node (most recent session)
    pub async fn get_latest_aux_info(&self) -> Option<(Uuid, Vec<u8>)> {
        match self.postgres.get_latest_aux_info(self.node_id).await {
            Ok(Some((session_id, data))) => Some((session_id, data)),
            Ok(None) => None,
            Err(e) => {
                warn!("Failed to get latest aux_info from PostgreSQL: {}", e);
                None
            }
        }
    }

    /// Get ceremony status
    pub async fn get_ceremony_status(&self, session_id: Uuid) -> Option<AuxInfoCeremony> {
        let ceremonies = self.active_ceremonies.read().await;
        ceremonies.get(&session_id).cloned()
    }

    /// List all ceremonies
    pub async fn list_ceremonies(&self) -> Vec<AuxInfoCeremony> {
        let ceremonies = self.active_ceremonies.read().await;
        ceremonies.values().cloned().collect()
    }
}
