//! Pregenerated Primes Management
//!
//! This module handles generation and persistent storage of pregenerated primes
//! used in the CGGMP24 auxiliary info generation protocol.
//!
//! Prime generation is computationally expensive (can take 30-120 seconds for
//! 2048-bit safe primes), so we generate once and reuse across container restarts.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

/// Wrapper for serializable pregenerated primes.
///
/// We store the primes as bincode-serialized bytes since cggmp24::PregeneratedPrimes
/// implements serde traits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPrimes {
    /// Version for future compatibility
    pub version: u32,
    /// Party index this was generated for
    pub party_index: u16,
    /// Serialized primes data (bincode format)
    #[serde(with = "base64_bytes")]
    pub primes_data: Vec<u8>,
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

impl StoredPrimes {
    pub const CURRENT_VERSION: u32 = 1;
}

/// Result of prime initialization
pub enum PrimeInitResult {
    /// Loaded existing primes from disk
    Loaded(Vec<u8>),
    /// Generated new primes
    Generated(Vec<u8>),
    /// Failed to load or generate
    Failed(String),
}

/// Default path for storing primes
pub fn default_primes_path(party_index: u16) -> PathBuf {
    PathBuf::from(format!("/data/primes-party-{}.json", party_index))
}

/// Check if primes file exists
pub fn primes_exist(path: &Path) -> bool {
    path.exists()
}

/// Load pregenerated primes from disk
pub fn load_primes(path: &Path) -> Result<StoredPrimes, String> {
    info!("Loading pregenerated primes from {:?}", path);

    let data =
        fs::read_to_string(path).map_err(|e| format!("Failed to read primes file: {}", e))?;

    let stored: StoredPrimes =
        serde_json::from_str(&data).map_err(|e| format!("Failed to parse primes file: {}", e))?;

    if stored.version != StoredPrimes::CURRENT_VERSION {
        warn!(
            "Primes file version mismatch: got {}, expected {}",
            stored.version,
            StoredPrimes::CURRENT_VERSION
        );
    }

    info!(
        "Loaded primes for party {} ({} bytes, generated at {})",
        stored.party_index,
        stored.primes_data.len(),
        stored.generated_at
    );

    Ok(stored)
}

/// Save pregenerated primes to disk
pub fn save_primes(path: &Path, primes: &StoredPrimes) -> Result<(), String> {
    info!("Saving pregenerated primes to {:?}", path);

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create primes directory: {}", e))?;
    }

    let data = serde_json::to_string_pretty(primes)
        .map_err(|e| format!("Failed to serialize primes: {}", e))?;

    fs::write(path, &data).map_err(|e| format!("Failed to write primes file: {}", e))?;

    info!("Primes saved successfully ({} bytes)", data.len());
    Ok(())
}

/// Generate new pregenerated primes using cggmp24.
///
/// This is computationally expensive and may take 30-120 seconds.
pub fn generate_primes(party_index: u16) -> Result<StoredPrimes, String> {
    info!("========================================");
    info!("  GENERATING PREGENERATED PRIMES");
    info!("========================================");
    info!("Party index: {}", party_index);
    info!("This may take 30-120 seconds...");

    let start = Instant::now();

    // Generate primes using cggmp24 with 128-bit security level
    let primes: cggmp24::PregeneratedPrimes<cggmp24::security_level::SecurityLevel128> =
        cggmp24::PregeneratedPrimes::generate(&mut OsRng);

    let elapsed = start.elapsed();
    info!(
        "Prime generation completed in {:.2}s",
        elapsed.as_secs_f64()
    );

    // Serialize the primes using bincode (more compact than JSON for binary data)
    let primes_data =
        bincode::serialize(&primes).map_err(|e| format!("Failed to serialize primes: {}", e))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let stored = StoredPrimes {
        version: StoredPrimes::CURRENT_VERSION,
        party_index,
        primes_data,
        generated_at: now,
    };

    info!(
        "Generated primes: {} bytes (serialized)",
        stored.primes_data.len()
    );
    info!("========================================");

    Ok(stored)
}

/// Initialize primes: load from disk if available, otherwise generate new ones.
///
/// If `force` is true, always regenerate primes even if they exist on disk.
pub fn initialize_primes(party_index: u16, path: Option<PathBuf>, force: bool) -> PrimeInitResult {
    let path = path.unwrap_or_else(|| default_primes_path(party_index));

    info!("Initializing pregenerated primes for party {}", party_index);
    debug!("Primes path: {:?}", path);
    debug!("Force regeneration: {}", force);

    // Try to load existing primes (unless force is set)
    if !force && primes_exist(&path) {
        info!("Found existing primes file, attempting to load...");
        match load_primes(&path) {
            Ok(stored) => {
                // Verify party index matches
                if stored.party_index != party_index {
                    warn!(
                        "Primes file party index mismatch: file has {}, we are {}",
                        stored.party_index, party_index
                    );
                    // Continue to regenerate
                } else {
                    // Verify we can deserialize the primes
                    match deserialize_primes(&stored.primes_data) {
                        Ok(_) => {
                            info!("Primes loaded and verified successfully");
                            return PrimeInitResult::Loaded(stored.primes_data);
                        }
                        Err(e) => {
                            warn!("Failed to verify loaded primes: {}", e);
                            // Continue to regenerate
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Failed to load existing primes: {}", e);
                // Continue to regenerate
            }
        }
    } else if force {
        info!("Force regeneration requested, ignoring existing primes");
    } else {
        info!("No existing primes file found");
    }

    // Generate new primes
    info!("Generating new pregenerated primes...");
    match generate_primes(party_index) {
        Ok(stored) => {
            // Save to disk for future use
            if let Err(e) = save_primes(&path, &stored) {
                error!("Failed to save primes to disk: {}", e);
                // Continue anyway - we have them in memory
            }
            PrimeInitResult::Generated(stored.primes_data)
        }
        Err(e) => {
            error!("Failed to generate primes: {}", e);
            PrimeInitResult::Failed(e)
        }
    }
}

/// Deserialize primes data back to cggmp24::PregeneratedPrimes
pub fn deserialize_primes(
    data: &[u8],
) -> Result<cggmp24::PregeneratedPrimes<cggmp24::security_level::SecurityLevel128>, String> {
    bincode::deserialize(data).map_err(|e| format!("Failed to deserialize primes: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stored_primes_serialization() {
        let stored = StoredPrimes {
            version: StoredPrimes::CURRENT_VERSION,
            party_index: 0,
            primes_data: vec![1, 2, 3, 4, 5],
            generated_at: 1234567890,
        };

        let json = serde_json::to_string(&stored).unwrap();
        let deserialized: StoredPrimes = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.version, stored.version);
        assert_eq!(deserialized.party_index, stored.party_index);
        assert_eq!(deserialized.primes_data, stored.primes_data);
        assert_eq!(deserialized.generated_at, stored.generated_at);
    }
}
