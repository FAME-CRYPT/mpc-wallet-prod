//! CGGMP24 Auxiliary Info Generation
//!
//! This module handles the generation of auxiliary information (Paillier keys,
//! ring-Pedersen parameters, etc.) required for CGGMP24 threshold signing.
//!
//! The aux_info generation is a multi-party protocol that must be run before
//! key generation or signing can take place.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Stored auxiliary info for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAuxInfo {
    /// Version for future compatibility
    pub version: u32,
    /// Party index this was generated for
    pub party_index: u16,
    /// Number of parties in the protocol
    pub num_parties: u16,
    /// Serialized aux_info data (JSON format, base64 encoded)
    #[serde(with = "base64_bytes")]
    pub aux_info_data: Vec<u8>,
    /// Session ID used for generation
    pub session_id: String,
    /// Timestamp when generated (Unix seconds)
    pub generated_at: u64,
}

/// Custom serialization for binary data as base64
mod base64_bytes {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let encoded = STANDARD.encode(bytes);
        encoded.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        STANDARD.decode(&encoded).map_err(serde::de::Error::custom)
    }
}

impl StoredAuxInfo {
    pub const CURRENT_VERSION: u32 = 1;
}

/// Result of aux_info initialization
pub enum AuxInfoInitResult {
    /// Loaded existing aux_info from disk
    Loaded(Vec<u8>),
    /// Need to generate (primes available)
    NeedToGenerate,
    /// Cannot generate (no primes)
    NoPrimes,
    /// Failed to load
    Failed(String),
}

/// Default path for storing aux_info
pub fn default_aux_info_path(party_index: u16) -> PathBuf {
    PathBuf::from(format!("/data/aux-info-party-{}.json", party_index))
}

/// Check if aux_info file exists
pub fn aux_info_exists(path: &Path) -> bool {
    path.exists()
}

/// Load aux_info from disk
pub fn load_aux_info(path: &Path) -> Result<StoredAuxInfo, String> {
    info!("Loading aux_info from {:?}", path);

    let data =
        fs::read_to_string(path).map_err(|e| format!("Failed to read aux_info file: {}", e))?;

    let stored: StoredAuxInfo =
        serde_json::from_str(&data).map_err(|e| format!("Failed to parse aux_info file: {}", e))?;

    if stored.version != StoredAuxInfo::CURRENT_VERSION {
        warn!(
            "Aux_info file version mismatch: got {}, expected {}",
            stored.version,
            StoredAuxInfo::CURRENT_VERSION
        );
    }

    info!(
        "Loaded aux_info for party {} ({} bytes, session: {})",
        stored.party_index,
        stored.aux_info_data.len(),
        stored.session_id
    );

    Ok(stored)
}

/// Save aux_info to disk
pub fn save_aux_info(path: &Path, aux_info: &StoredAuxInfo) -> Result<(), String> {
    info!("Saving aux_info to {:?}", path);

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create aux_info directory: {}", e))?;
    }

    let data = serde_json::to_string_pretty(aux_info)
        .map_err(|e| format!("Failed to serialize aux_info: {}", e))?;

    fs::write(path, &data).map_err(|e| format!("Failed to write aux_info file: {}", e))?;

    info!("Aux_info saved successfully ({} bytes)", data.len());
    Ok(())
}

/// Check if aux_info can be loaded or needs to be generated
pub fn check_aux_info(party_index: u16, path: Option<PathBuf>) -> AuxInfoInitResult {
    let path = path.unwrap_or_else(|| default_aux_info_path(party_index));

    if aux_info_exists(&path) {
        match load_aux_info(&path) {
            Ok(stored) => {
                if stored.party_index != party_index {
                    warn!(
                        "Aux_info party index mismatch: file has {}, we are {}",
                        stored.party_index, party_index
                    );
                    AuxInfoInitResult::NeedToGenerate
                } else {
                    AuxInfoInitResult::Loaded(stored.aux_info_data)
                }
            }
            Err(e) => {
                warn!("Failed to load aux_info: {}", e);
                AuxInfoInitResult::NeedToGenerate
            }
        }
    } else {
        AuxInfoInitResult::NeedToGenerate
    }
}

/// Parameters for aux_info generation via HTTP coordinator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxInfoGenRequest {
    /// Unique session ID for this generation
    pub session_id: String,
    /// Number of parties participating
    pub num_parties: u16,
    /// Peer addresses for libp2p connection (optional, uses mDNS if empty)
    pub peer_addrs: Vec<String>,
}

/// Response from aux_info generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxInfoGenResponse {
    /// Whether generation succeeded
    pub success: bool,
    /// Party index
    pub party_index: u16,
    /// Size of generated aux_info in bytes
    pub aux_info_size_bytes: usize,
    /// Time taken for generation
    pub generation_time_secs: f64,
    /// Error message if failed
    pub error: Option<String>,
}

/// Status of aux_info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxInfoStatus {
    pub party_index: u16,
    pub has_aux_info: bool,
    pub aux_info_size_bytes: usize,
    pub session_id: Option<String>,
    pub generated_at: Option<u64>,
}

/// Get current aux_info status
pub fn get_aux_info_status(party_index: u16) -> AuxInfoStatus {
    let path = default_aux_info_path(party_index);

    if let Ok(stored) = load_aux_info(&path) {
        AuxInfoStatus {
            party_index,
            has_aux_info: true,
            aux_info_size_bytes: stored.aux_info_data.len(),
            session_id: Some(stored.session_id),
            generated_at: Some(stored.generated_at),
        }
    } else {
        AuxInfoStatus {
            party_index,
            has_aux_info: false,
            aux_info_size_bytes: 0,
            session_id: None,
            generated_at: None,
        }
    }
}

/// Deserialize aux_info data back to cggmp24::AuxInfo
/// Uses serde_json because cggmp24 types require deserialize_any support
pub fn deserialize_aux_info(
    data: &[u8],
) -> Result<cggmp24::key_share::AuxInfo<cggmp24::security_level::SecurityLevel128>, String> {
    serde_json::from_slice(data).map_err(|e| format!("Failed to deserialize aux_info: {}", e))
}

/// Serialize aux_info to bytes
/// Uses serde_json because cggmp24 types require deserialize_any support
pub fn serialize_aux_info(
    aux_info: &cggmp24::key_share::AuxInfo<cggmp24::security_level::SecurityLevel128>,
) -> Result<Vec<u8>, String> {
    serde_json::to_vec(aux_info).map_err(|e| format!("Failed to serialize aux_info: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stored_aux_info_serialization() {
        let stored = StoredAuxInfo {
            version: StoredAuxInfo::CURRENT_VERSION,
            party_index: 0,
            num_parties: 4,
            aux_info_data: vec![1, 2, 3, 4, 5],
            session_id: "test-session".to_string(),
            generated_at: 1234567890,
        };

        let json = serde_json::to_string(&stored).unwrap();
        let deserialized: StoredAuxInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.version, stored.version);
        assert_eq!(deserialized.party_index, stored.party_index);
        assert_eq!(deserialized.aux_info_data, stored.aux_info_data);
        assert_eq!(deserialized.session_id, stored.session_id);
    }
}
