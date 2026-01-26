//! Distributed Key Generation Service
//!
//! This module implements DKG (Distributed Key Generation) coordination for both:
//! - CGGMP24 (ECDSA) for SegWit addresses
//! - FROST (Schnorr) for Taproot addresses
//!
//! # Protocol Separation
//!
//! ⚠️ CRITICAL: Do NOT mix libraries:
//! - CGGMP24 uses `cggmp24` library for ECDSA threshold signing
//! - FROST uses `givre` library for Schnorr threshold signing
//!
//! Each protocol has its own DKG ceremony and produces different key types.

use crate::error::{OrchestrationError, Result};
use crate::message_router::{MessageRouter, ProtocolMessage as RouterProtocolMessage, ProtocolType as RouterProtocolType};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use threshold_network::QuicEngine;
use threshold_storage::{EtcdStorage, PostgresStorage};
use threshold_types::{DkgMessage, NetworkMessage, NodeId, PeerId, TxId};
use tokio::sync::{Mutex, RwLock};
use tracing::{error, info, warn};
use uuid::Uuid;

// Protocol keygen modules (use existing working implementations)
use protocols::cggmp24::keygen as cggmp24_keygen;
use protocols::cggmp24::runner::ProtocolMessage as Cggmp24Message;
use protocols::frost::keygen as frost_keygen;
use protocols::frost::keygen::ProtocolMessage as FrostMessage;

// Bitcoin address derivation
use common::bitcoin_address::{derive_p2tr_address, derive_p2wpkh_address, BitcoinNetwork};

// Async channel for message passing
use async_channel;

/// Protocol type for DKG ceremony
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProtocolType {
    /// CGGMP24 ECDSA for SegWit addresses (P2WPKH, P2WSH)
    CGGMP24,
    /// FROST Schnorr for Taproot addresses (P2TR)
    FROST,
}

impl std::fmt::Display for ProtocolType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtocolType::CGGMP24 => write!(f, "cggmp24"),
            ProtocolType::FROST => write!(f, "frost"),
        }
    }
}

/// Result of a DKG ceremony
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DkgResult {
    /// Session ID of the DKG ceremony
    pub session_id: Uuid,
    /// Protocol used (CGGMP24 or FROST)
    pub protocol: ProtocolType,
    /// Shared public key (compressed for CGGMP24, x-only for FROST)
    pub public_key: Vec<u8>,
    /// Bitcoin address derived from public key
    pub address: String,
    /// Threshold (e.g., 4 for 4-of-5)
    pub threshold: u32,
    /// Total participants
    pub total_nodes: u32,
    /// Ceremony completion time
    pub completed_at: chrono::DateTime<Utc>,
}

/// DKG ceremony status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DkgStatus {
    /// Ceremony is in progress
    Running,
    /// Ceremony completed successfully
    Completed,
    /// Ceremony failed
    Failed,
}

impl std::fmt::Display for DkgStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DkgStatus::Running => write!(f, "running"),
            DkgStatus::Completed => write!(f, "completed"),
            DkgStatus::Failed => write!(f, "failed"),
        }
    }
}

/// DKG ceremony state stored in etcd
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DkgCeremony {
    pub session_id: Uuid,
    pub protocol: ProtocolType,
    pub threshold: u32,
    pub total_nodes: u32,
    pub participants: Vec<NodeId>,
    pub status: DkgStatus,
    pub current_round: u32,
    pub started_at: chrono::DateTime<Utc>,
    pub completed_at: Option<chrono::DateTime<Utc>>,
    pub public_key: Option<Vec<u8>>,
    pub error: Option<String>,
}

impl DkgCeremony {
    /// Convert to storage DkgCeremony
    fn to_storage(&self) -> threshold_storage::DkgCeremony {
        threshold_storage::DkgCeremony {
            session_id: self.session_id,
            protocol: format!("{:?}", self.protocol).to_lowercase(),
            threshold: self.threshold,
            total_nodes: self.total_nodes,
            status: format!("{:?}", self.status).to_lowercase(),
            public_key: self.public_key.clone(),
            started_at: self.started_at,
            completed_at: self.completed_at,
            error: self.error.clone(),
        }
    }

    /// Convert from storage DkgCeremony
    fn from_storage(storage: threshold_storage::DkgCeremony) -> Self {
        let protocol = match storage.protocol.to_lowercase().as_str() {
            "cggmp24" => ProtocolType::CGGMP24,
            "frost" => ProtocolType::FROST,
            _ => ProtocolType::CGGMP24, // Default
        };

        let status = match storage.status.to_lowercase().as_str() {
            "running" => DkgStatus::Running,
            "completed" => DkgStatus::Completed,
            "failed" => DkgStatus::Failed,
            _ => DkgStatus::Running, // Default
        };

        Self {
            session_id: storage.session_id,
            protocol,
            threshold: storage.threshold,
            total_nodes: storage.total_nodes,
            participants: Vec::new(), // Not stored in PostgreSQL
            status,
            current_round: 0, // Not stored
            started_at: storage.started_at,
            completed_at: storage.completed_at,
            public_key: storage.public_key,
            error: storage.error,
        }
    }
}

/// DKG Service for coordinating distributed key generation
pub struct DkgService {
    /// PostgreSQL storage for key shares (encrypted)
    postgres: Arc<PostgresStorage>,
    /// etcd storage for ceremony coordination (with interior mutability)
    etcd: Arc<Mutex<EtcdStorage>>,
    /// QUIC network engine for P2P communication
    quic: Arc<QuicEngine>,
    /// Message router for protocol communication
    message_router: Arc<MessageRouter>,
    /// Current node ID
    node_id: NodeId,
    /// Active DKG ceremonies (session_id -> ceremony state)
    active_ceremonies: Arc<RwLock<HashMap<Uuid, DkgCeremony>>>,
    /// DKG round message buffer (session_id -> round -> node_id -> payload)
    message_buffer: Arc<Mutex<HashMap<Uuid, HashMap<u32, HashMap<NodeId, Vec<u8>>>>>>,
}

impl DkgService {
    /// Create new DKG service
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

    /// Initiate DKG ceremony with automatic protocol selection based on Bitcoin address
    ///
    /// This is a convenience method that detects the address type and selects
    /// the appropriate protocol (CGGMP24 for SegWit, FROST for Taproot).
    ///
    /// # Arguments
    /// * `threshold` - Signing threshold (e.g., 4 for 4-of-5)
    /// * `total_nodes` - Total number of participating nodes
    /// * `bitcoin_address` - Target Bitcoin address for protocol selection
    ///
    /// # Returns
    /// DKG result with session ID, public key, and derived address
    pub async fn initiate_dkg_auto(
        &self,
        threshold: u32,
        total_nodes: u32,
        bitcoin_address: &str,
    ) -> Result<DkgResult> {
        use crate::protocol_router::BitcoinAddressType;

        // Detect address type and required protocol
        let address_type = BitcoinAddressType::detect(bitcoin_address)?;
        let protocol = match address_type {
            BitcoinAddressType::Taproot => {
                info!("Detected Taproot address ({}), using FROST protocol", bitcoin_address);
                ProtocolType::FROST
            }
            _ => {
                info!("Detected SegWit/Legacy address ({}), using CGGMP24 protocol", bitcoin_address);
                ProtocolType::CGGMP24
            }
        };

        // Delegate to main initiate_dkg method
        self.initiate_dkg(protocol, threshold, total_nodes).await
    }

    /// Initiate a new DKG ceremony (coordinator node only)
    ///
    /// This method:
    /// 1. Acquires distributed lock in etcd
    /// 2. Creates ceremony record
    /// 3. Broadcasts DKG initiation to all participants
    /// 4. Coordinates multi-round DKG protocol
    /// 5. Stores resulting key shares
    pub async fn initiate_dkg(
        &self,
        protocol: ProtocolType,
        threshold: u32,
        total_nodes: u32,
    ) -> Result<DkgResult> {
        info!(
            "Initiating DKG ceremony: protocol={} threshold={} total_nodes={}",
            protocol, threshold, total_nodes
        );

        // Validate parameters
        if threshold > total_nodes {
            return Err(OrchestrationError::InvalidConfig(format!(
                "Threshold {} cannot exceed total_nodes {}",
                threshold, total_nodes
            )));
        }

        if threshold < 2 {
            return Err(OrchestrationError::InvalidConfig(
                "Threshold must be at least 2".to_string(),
            ));
        }

        // Acquire distributed lock for DKG
        let lock_key = "/locks/dkg";
        let lock_acquired = {
            let mut etcd = self.etcd.lock().await;
            etcd.acquire_lock(lock_key, 300) // 5 minute timeout
                .await
                .map_err(|e| OrchestrationError::StorageError(format!("Failed to acquire DKG lock: {}", e)))?
        };

        if !lock_acquired {
            return Err(OrchestrationError::CeremonyInProgress(
                "Another DKG ceremony is already running".to_string(),
            ));
        }

        // Create ceremony record
        let session_id = Uuid::new_v4();
        let participants: Vec<NodeId> = (1..=total_nodes).map(|i| NodeId(i as u64)).collect();

        let ceremony = DkgCeremony {
            session_id,
            protocol,
            threshold,
            total_nodes,
            participants: participants.clone(),
            status: DkgStatus::Running,
            current_round: 0,
            started_at: Utc::now(),
            completed_at: None,
            public_key: None,
            error: None,
        };

        // Store ceremony in PostgreSQL
        self.postgres
            .create_dkg_ceremony(&ceremony.to_storage())
            .await
            .map_err(|e| {
                OrchestrationError::StorageError(format!("Failed to create ceremony: {}", e))
            })?;

        // Store in active ceremonies
        {
            let mut ceremonies = self.active_ceremonies.write().await;
            ceremonies.insert(session_id, ceremony.clone());
        }

        // Run protocol-specific DKG
        let result = match protocol {
            ProtocolType::CGGMP24 => self.run_cggmp24_dkg(session_id, participants).await,
            ProtocolType::FROST => self.run_frost_dkg(session_id, participants).await,
        };

        // Release lock
        {
            let mut etcd = self.etcd.lock().await;
            etcd.release_lock(lock_key)
                .await
                .map_err(|e| OrchestrationError::StorageError(format!("Failed to release lock: {}", e)))?;
        }

        match result {
            Ok(public_key) => {
                // Update ceremony status to completed
                let mut ceremonies = self.active_ceremonies.write().await;
                if let Some(ceremony) = ceremonies.get_mut(&session_id) {
                    ceremony.status = DkgStatus::Completed;
                    ceremony.completed_at = Some(Utc::now());
                    ceremony.public_key = Some(public_key.clone());

                    // Update PostgreSQL
                    self.postgres
                        .complete_dkg_ceremony(session_id, &public_key)
                        .await
                        .map_err(|e| {
                            OrchestrationError::StorageError(format!(
                                "Failed to update ceremony: {}",
                                e
                            ))
                        })?;
                }

                // Derive Bitcoin address
                let address = self.derive_address(protocol, &public_key)?;

                // Store public key in etcd for cluster-wide access
                let pubkey_key = format!("/cluster/public_keys/{}", protocol);
                {
                    let mut etcd = self.etcd.lock().await;
                    etcd.put(&pubkey_key, &public_key).await
                        .map_err(|e| OrchestrationError::Storage(e.into()))?;
                }

                // Store DKG configuration in etcd for presignature service
                let config_key = format!("/cluster/dkg/{}/config", protocol);
                let config = serde_json::json!({
                    "threshold": threshold,
                    "total_nodes": total_nodes,
                    "public_key": hex::encode(&public_key),
                });
                let config_bytes = serde_json::to_vec(&config)
                    .map_err(|e| OrchestrationError::Internal(format!("JSON serialization failed: {}", e)))?;
                {
                    let mut etcd = self.etcd.lock().await;
                    etcd.put(&config_key, &config_bytes).await
                        .map_err(|e| OrchestrationError::Storage(e.into()))?;
                }

                info!(
                    "DKG ceremony completed: session={} protocol={} address={} threshold={} total_nodes={}",
                    session_id, protocol, address, threshold, total_nodes
                );

                Ok(DkgResult {
                    session_id,
                    protocol,
                    public_key,
                    address,
                    threshold,
                    total_nodes,
                    completed_at: Utc::now(),
                })
            }
            Err(e) => {
                error!("DKG ceremony failed: session={} error={}", session_id, e);

                // Update ceremony status to failed
                let mut ceremonies = self.active_ceremonies.write().await;
                if let Some(ceremony) = ceremonies.get_mut(&session_id) {
                    ceremony.status = DkgStatus::Failed;
                    ceremony.error = Some(e.to_string());

                    // Update PostgreSQL
                    self.postgres
                        .fail_dkg_ceremony(session_id, &e.to_string())
                        .await
                        .ok(); // Ignore errors here
                }

                Err(e)
            }
        }
    }

    /// Join an existing DKG ceremony (participant nodes)
    ///
    /// This method is called by non-coordinator nodes to join a DKG ceremony
    /// that was already initiated by the coordinator node.
    /// It reads the ceremony details from PostgreSQL and participates in the protocol.
    pub async fn join_dkg_ceremony(&self, session_id: Uuid) -> Result<DkgResult> {
        info!("Joining DKG ceremony: session_id={}", session_id);

        // Read ceremony details from PostgreSQL (created by coordinator)
        let ceremony_data = self.postgres
            .get_dkg_ceremony(session_id)
            .await
            .map_err(|e| OrchestrationError::StorageError(format!("Ceremony not found: {}", e)))?;

        // Parse protocol type
        let protocol = match ceremony_data.protocol.as_str() {
            "cggmp24" => ProtocolType::CGGMP24,
            "frost" => ProtocolType::FROST,
            _ => {
                return Err(OrchestrationError::InvalidConfig(format!(
                    "Unknown protocol: {}",
                    ceremony_data.protocol
                )));
            }
        };

        // Build participants list
        let participants: Vec<NodeId> = (1..=ceremony_data.total_nodes)
            .map(|i| NodeId(i as u64))
            .collect();

        // Store in active ceremonies (for tracking)
        {
            let mut ceremonies = self.active_ceremonies.write().await;
            ceremonies.insert(session_id, DkgCeremony {
                session_id,
                protocol,
                threshold: ceremony_data.threshold,
                total_nodes: ceremony_data.total_nodes,
                participants: participants.clone(),
                status: DkgStatus::Running,
                current_round: 0,
                started_at: ceremony_data.started_at,
                completed_at: None,
                public_key: None,
                error: None,
            });
        }

        // Run protocol-specific DKG (same as coordinator, but without lock)
        let result = match protocol {
            ProtocolType::CGGMP24 => self.run_cggmp24_dkg(session_id, participants).await,
            ProtocolType::FROST => self.run_frost_dkg(session_id, participants).await,
        };

        match result {
            Ok(public_key) => {
                // Update ceremony status to completed
                let mut ceremonies = self.active_ceremonies.write().await;
                if let Some(ceremony) = ceremonies.get_mut(&session_id) {
                    ceremony.status = DkgStatus::Completed;
                    ceremony.completed_at = Some(Utc::now());
                    ceremony.public_key = Some(public_key.clone());
                }

                // Derive Bitcoin address
                let address = self.derive_address(protocol, &public_key)?;

                Ok(DkgResult {
                    session_id,
                    protocol,
                    public_key,
                    address,
                    threshold: ceremony_data.threshold,
                    total_nodes: ceremony_data.total_nodes,
                    completed_at: Utc::now(),
                })
            }
            Err(e) => {
                error!("DKG ceremony failed: session={} error={}", session_id, e);

                // Update ceremony status to failed
                let mut ceremonies = self.active_ceremonies.write().await;
                if let Some(ceremony) = ceremonies.get_mut(&session_id) {
                    ceremony.status = DkgStatus::Failed;
                    ceremony.error = Some(e.to_string());
                }

                Err(e)
            }
        }
    }

    /// Run CGGMP24 DKG ceremony (5-6 rounds)
    ///
    /// Uses the `cggmp24` library for ECDSA threshold key generation.
    /// Produces a compressed secp256k1 public key (33 bytes) for SegWit addresses.
    async fn run_cggmp24_dkg(
        &self,
        session_id: Uuid,
        participants: Vec<NodeId>,
    ) -> Result<Vec<u8>> {
        info!(
            "Running CGGMP24 DKG: session={} participants={:?}",
            session_id, participants
        );

        // Find this node's party index in the participants list
        let party_index = participants
            .iter()
            .position(|p| *p == self.node_id)
            .ok_or_else(|| {
                OrchestrationError::InvalidConfig(format!(
                    "Current node {} not found in participants list",
                    self.node_id
                ))
            })? as u16;

        let threshold = {
            let ceremonies = self.active_ceremonies.read().await;
            ceremonies
                .get(&session_id)
                .map(|c| c.threshold as u16)
                .ok_or_else(|| OrchestrationError::CeremonyNotFound(session_id))?
        };

        let total_parties = participants.len() as u16;

        info!(
            "CGGMP24 DKG parameters: party_index={} threshold={} total={}",
            party_index, threshold, total_parties
        );

        // Register session with message router to get communication channels
        let (outgoing_tx, incoming_rx) = self
            .message_router
            .register_session(
                session_id,
                RouterProtocolType::DKG,
                participants.iter().copied().collect(),
            )
            .await
            .map_err(|e| {
                OrchestrationError::Internal(format!("Failed to register DKG session: {}", e))
            })?;

        // Convert between RouterProtocolMessage and CGGMP24 protocol message types
        // Spawn adapter task to convert incoming RouterProtocolMessages to CGGMP24 format
        let (protocol_incoming_tx, protocol_incoming_rx) = async_channel::bounded(100);
        let session_id_for_incoming = session_id.to_string();
        let node_id_for_incoming = self.node_id;
        tokio::spawn(async move {
            while let Ok(router_msg) = incoming_rx.recv().await {
                tracing::info!(
                    "📨 [Node {}] Incoming message from party {}, seq={}, payload_size={}",
                    node_id_for_incoming,
                    router_msg.from.0 - 1,
                    router_msg.sequence,
                    router_msg.payload.len()
                );

                // Convert RouterProtocolMessage to Cggmp24Message
                // NOTE: We set recipient=None because RouterProtocolMessage doesn't preserve
                // whether the original message was broadcast or P2P. The message_router
                // sends individual copies to each recipient, so we treat all as broadcasts.
                let cggmp24_msg = Cggmp24Message {
                    session_id: session_id_for_incoming.clone(),
                    sender: router_msg.from.0 as u16 - 1, // NodeId starts from 1, party_index from 0
                    recipient: None, // Treat as broadcast (was incorrectly set to Some before)
                    round: 0,
                    payload: router_msg.payload,
                    seq: router_msg.sequence,
                };
                if protocol_incoming_tx.send(cggmp24_msg).await.is_err() {
                    tracing::warn!("Protocol incoming channel closed");
                    break;
                }
            }
            tracing::info!("Incoming message adapter task finished");
        });

        // Spawn adapter task to convert outgoing CGGMP24 messages to RouterProtocolMessages
        let (protocol_outgoing_tx, protocol_outgoing_rx) = async_channel::bounded::<Cggmp24Message>(100);
        let node_id = self.node_id;
        let session_id_clone = session_id;
        let participants_clone = participants.clone();
        tokio::spawn(async move {
            while let Ok(cggmp24_msg) = protocol_outgoing_rx.recv().await {
                // Convert Cggmp24Message to RouterProtocolMessage
                // Handle both broadcast (recipient=None) and unicast (recipient=Some)
                match cggmp24_msg.recipient {
                    None => {
                        // Broadcast to all participants except sender
                        tracing::info!(
                            "📤 [Node {}] Broadcasting message seq={}, payload_size={} to {} participants",
                            node_id,
                            cggmp24_msg.seq,
                            cggmp24_msg.payload.len(),
                            participants_clone.len() - 1
                        );
                        for &participant in &participants_clone {
                            if participant != node_id {
                                let router_msg = RouterProtocolMessage {
                                    session_id: session_id_clone,
                                    from: node_id,
                                    to: participant,
                                    payload: cggmp24_msg.payload.clone(),
                                    sequence: cggmp24_msg.seq,
                                };
                                if outgoing_tx.send(router_msg).await.is_err() {
                                    tracing::error!("Failed to send broadcast message to participant {}", participant);
                                }
                            }
                        }
                    }
                    Some(recipient_index) => {
                        // Unicast to specific participant
                        let recipient = NodeId((recipient_index + 1) as u64); // party_index 0-based, NodeId 1-based
                        tracing::info!(
                            "📤 [Node {}] Sending P2P message seq={}, payload_size={} to party {}",
                            node_id,
                            cggmp24_msg.seq,
                            cggmp24_msg.payload.len(),
                            recipient_index
                        );
                        let router_msg = RouterProtocolMessage {
                            session_id: session_id_clone,
                            from: node_id,
                            to: recipient,
                            payload: cggmp24_msg.payload,
                            sequence: cggmp24_msg.seq,
                        };
                        if outgoing_tx.send(router_msg).await.is_err() {
                            tracing::error!("Failed to send unicast message to participant {}", recipient);
                        }
                    }
                }
            }
            tracing::info!("Outgoing message adapter task finished");
        });

        // Run the CGGMP24 DKG protocol using existing working implementation
        let result = cggmp24_keygen::run_keygen(
            party_index,
            total_parties,
            threshold,
            &session_id.to_string(),
            protocol_incoming_rx,
            protocol_outgoing_tx,
        )
        .await;

        if !result.success {
            return Err(OrchestrationError::Protocol(
                result.error.unwrap_or_else(|| "Unknown CGGMP24 DKG error".to_string()),
            ));
        }

        let key_share_data = result
            .key_share_data
            .ok_or_else(|| OrchestrationError::Protocol("No key share generated".to_string()))?;

        let public_key = result
            .public_key
            .ok_or_else(|| OrchestrationError::Protocol("No public key generated".to_string()))?;

        // Store key share in PostgreSQL
        let node_id = NodeId(party_index as u64);
        self.postgres
            .store_key_share(
                session_id,
                node_id,
                &key_share_data,
            )
            .await
            .map_err(|e| {
                OrchestrationError::StorageError(format!("Failed to store key share: {}", e))
            })?;

        info!(
            "CGGMP24 DKG completed successfully in {:.2}s: session={} pubkey_len={}",
            result.duration_secs, session_id, public_key.len()
        );

        // Return compressed public key (33 bytes)
        Ok(public_key)
    }

    /// Run FROST DKG ceremony (2-3 rounds)
    ///
    /// Uses the `givre` library for Schnorr threshold key generation.
    /// Produces an x-only public key (32 bytes) for Taproot addresses.
    async fn run_frost_dkg(
        &self,
        session_id: Uuid,
        participants: Vec<NodeId>,
    ) -> Result<Vec<u8>> {
        info!(
            "Running FROST DKG: session={} participants={:?}",
            session_id, participants
        );

        // Find this node's party index in the participants list
        let party_index = participants
            .iter()
            .position(|p| *p == self.node_id)
            .ok_or_else(|| {
                OrchestrationError::InvalidConfig(format!(
                    "Current node {} not found in participants list",
                    self.node_id
                ))
            })? as u16;

        let threshold = {
            let ceremonies = self.active_ceremonies.read().await;
            ceremonies
                .get(&session_id)
                .map(|c| c.threshold as u16)
                .ok_or_else(|| OrchestrationError::CeremonyNotFound(session_id))?
        };

        let total_parties = participants.len() as u16;

        info!(
            "FROST DKG parameters: party_index={} threshold={} total={}",
            party_index, threshold, total_parties
        );

        // Register session with message router to get communication channels
        let (outgoing_tx, incoming_rx) = self
            .message_router
            .register_session(
                session_id,
                RouterProtocolType::DKG,
                participants.iter().copied().collect(),
            )
            .await
            .map_err(|e| {
                OrchestrationError::Internal(format!("Failed to register DKG session: {}", e))
            })?;

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
                        tracing::error!("Failed to deserialize FROST DKG message: {}", e);
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
                        tracing::error!("Failed to serialize FROST DKG message: {}", e);
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
                        };
                        if outgoing_tx.send(proto_msg).await.is_err() {
                            tracing::error!("Failed to send message to participant {}", participant);
                        }
                    }
                }
                sequence += 1;
            }
        });

        // Run the FROST DKG protocol using existing working implementation
        let result = frost_keygen::run_frost_keygen(
            party_index,
            total_parties,
            threshold,
            &session_id.to_string(),
            protocol_incoming_rx,
            protocol_outgoing_tx,
        )
        .await;

        if !result.success {
            return Err(OrchestrationError::Protocol(
                result.error.unwrap_or_else(|| "Unknown FROST DKG error".to_string()),
            ));
        }

        let key_share_data = result
            .key_share_data
            .ok_or_else(|| OrchestrationError::Protocol("No key share generated".to_string()))?;

        let public_key = result
            .public_key
            .ok_or_else(|| OrchestrationError::Protocol("No public key generated".to_string()))?;

        // Store key share in PostgreSQL
        let node_id = NodeId(party_index as u64);
        self.postgres
            .store_key_share(
                session_id,
                node_id,
                &key_share_data,
            )
            .await
            .map_err(|e| {
                OrchestrationError::StorageError(format!("Failed to store key share: {}", e))
            })?;

        info!(
            "FROST DKG completed successfully in {:.2}s: session={} x_only_pubkey_len={}",
            result.duration_secs, session_id, public_key.len()
        );

        // Return x-only public key (32 bytes)
        Ok(public_key)
    }

    /// Broadcast DKG message to all participants
    async fn broadcast_dkg_message(
        &self,
        session_id: Uuid,
        round: u32,
        payload: Vec<u8>,
    ) -> Result<()> {
        let msg = NetworkMessage::DkgRound(DkgMessage {
            session_id,
            round,
            from: self.node_id,
            payload,
        });

        let stream_id = 0; // DKG stream
        // Broadcast to all peers
        self.quic
            .broadcast(&msg, stream_id, None)
            .await
            .map_err(|e| OrchestrationError::NetworkError(format!("Failed to broadcast: {}", e)))?;

        Ok(())
    }

    /// Collect DKG messages from all participants for a specific round
    async fn collect_dkg_round(
        &self,
        session_id: Uuid,
        round: u32,
        expected_count: usize,
        timeout_secs: u64,
    ) -> Result<HashMap<NodeId, Vec<u8>>> {
        use tokio::time::{timeout, Duration};

        let result = timeout(Duration::from_secs(timeout_secs), async {
            loop {
                let buffer = self.message_buffer.lock().await;
                if let Some(session_rounds) = buffer.get(&session_id) {
                    if let Some(round_messages) = session_rounds.get(&round) {
                        if round_messages.len() >= expected_count {
                            return Ok(round_messages.clone());
                        }
                    }
                }
                drop(buffer);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await;

        match result {
            Ok(messages) => messages,
            Err(_) => Err(OrchestrationError::Timeout(format!(
                "Timeout waiting for DKG round {} messages",
                round
            ))),
        }
    }

    /// Handle incoming DKG message from another node
    pub async fn handle_dkg_message(&self, msg: DkgMessage) -> Result<()> {
        info!(
            "Received DKG message: session={} round={} from={}",
            msg.session_id, msg.round, msg.from
        );

        // Store message in buffer
        let mut buffer = self.message_buffer.lock().await;
        buffer
            .entry(msg.session_id)
            .or_insert_with(HashMap::new)
            .entry(msg.round)
            .or_insert_with(HashMap::new)
            .insert(msg.from, msg.payload);

        Ok(())
    }

    /// Derive Bitcoin address from public key based on protocol
    fn derive_address(&self, protocol: ProtocolType, public_key: &[u8]) -> Result<String> {
        // Use mainnet for production (can be made configurable later)
        let network = BitcoinNetwork::Mainnet;

        match protocol {
            ProtocolType::CGGMP24 => {
                // CGGMP24 uses compressed public key (33 bytes) for P2WPKH (Native SegWit)
                if public_key.len() != 33 {
                    return Err(OrchestrationError::InvalidPublicKey(format!(
                        "Expected 33-byte compressed key for CGGMP24, got {}",
                        public_key.len()
                    )));
                }

                // Derive P2WPKH address (bc1q...)
                let address = derive_p2wpkh_address(public_key, network)
                    .map_err(|e| OrchestrationError::Internal(format!(
                        "Failed to derive P2WPKH address: {}", e
                    )))?;

                info!("Derived P2WPKH address for CGGMP24: {}", address);
                Ok(address)
            }
            ProtocolType::FROST => {
                // FROST uses x-only public key (32 bytes) for P2TR (Taproot)
                if public_key.len() != 32 {
                    return Err(OrchestrationError::InvalidPublicKey(format!(
                        "Expected 32-byte x-only key for FROST, got {}",
                        public_key.len()
                    )));
                }

                // Derive P2TR address (bc1p...)
                let address = derive_p2tr_address(public_key, network)
                    .map_err(|e| OrchestrationError::Internal(format!(
                        "Failed to derive P2TR address: {}", e
                    )))?;

                info!("Derived P2TR address for FROST: {}", address);
                Ok(address)
            }
        }
    }

    /// Get status of a DKG ceremony
    pub async fn get_ceremony_status(&self, session_id: Uuid) -> Result<DkgCeremony> {
        let ceremonies = self.active_ceremonies.read().await;
        ceremonies
            .get(&session_id)
            .cloned()
            .ok_or_else(|| OrchestrationError::CeremonyNotFound(session_id))
    }

    /// List all DKG ceremonies
    pub async fn list_ceremonies(&self) -> Result<Vec<DkgCeremony>> {
        let storage_ceremonies = self.postgres
            .list_dkg_ceremonies()
            .await
            .map_err(|e| OrchestrationError::StorageError(format!("Failed to list ceremonies: {}", e)))?;

        Ok(storage_ceremonies
            .into_iter()
            .map(DkgCeremony::from_storage)
            .collect())
    }
}

/// Helper module for hex encoding
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_type_display() {
        assert_eq!(ProtocolType::CGGMP24.to_string(), "cggmp24");
        assert_eq!(ProtocolType::FROST.to_string(), "frost");
    }

    #[test]
    fn test_dkg_status_display() {
        assert_eq!(DkgStatus::Running.to_string(), "running");
        assert_eq!(DkgStatus::Completed.to_string(), "completed");
        assert_eq!(DkgStatus::Failed.to_string(), "failed");
    }

    #[tokio::test]
    #[ignore] // Requires running etcd and PostgreSQL
    async fn test_dkg_initiation() {
        // TODO: Add integration test with mock etcd and PostgreSQL
    }
}
