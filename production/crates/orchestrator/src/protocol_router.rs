//! Protocol Router
//!
//! Intelligently routes Bitcoin transactions to the correct signing protocol based on
//! recipient address type:
//! - SegWit (P2WPKH, P2WSH) → CGGMP24 ECDSA
//! - Taproot (P2TR) → FROST Schnorr
//! - Legacy (P2PKH, P2SH) → CGGMP24 ECDSA
//!
//! # Features
//!
//! - **Automatic Detection**: Parses recipient address to determine type
//! - **Protocol Selection**: Routes to CGGMP24 or FROST automatically
//! - **Validation**: Ensures protocol matches address requirements
//! - **Performance Optimization**: Tracks presignature pool availability

use crate::error::{OrchestrationError, Result};
use crate::signing_coordinator::SignatureProtocol;
use serde::{Deserialize, Serialize};

/// Bitcoin address classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BitcoinAddressType {
    /// Native SegWit (bech32): bc1q... - Requires ECDSA
    NativeSegWit,
    /// Taproot (bech32m): bc1p... - Requires Schnorr
    Taproot,
    /// Legacy P2PKH: 1... - Requires ECDSA
    LegacyP2PKH,
    /// Legacy P2SH: 3... - Requires ECDSA
    LegacyP2SH,
    /// Nested SegWit: 3... (P2WPKH-in-P2SH) - Requires ECDSA
    NestedSegWit,
}

impl BitcoinAddressType {
    /// Detect address type from address string with comprehensive validation
    pub fn detect(address: &str) -> Result<Self> {
        // Validate address is not empty
        if address.is_empty() {
            return Err(OrchestrationError::InvalidConfig(
                "Bitcoin address cannot be empty".to_string()
            ));
        }

        // Validate address length (Bitcoin addresses: 26-90 chars including bech32m)
        if address.len() < 26 || address.len() > 90 {
            return Err(OrchestrationError::InvalidConfig(format!(
                "Invalid address length: {} (expected 26-90)", address.len()
            )));
        }

        // MAINNET: Native SegWit P2WPKH (bc1q..., 42 chars)
        if address.starts_with("bc1q") && address.len() >= 42 && address.len() <= 62 {
            return Ok(Self::NativeSegWit);
        }

        // MAINNET: Taproot P2TR (bc1p..., 62 chars)
        if address.starts_with("bc1p") {
            if address.len() != 62 {
                return Err(OrchestrationError::InvalidConfig(format!(
                    "Invalid Taproot address length: {} (expected 62)", address.len()
                )));
            }
            return Ok(Self::Taproot);
        }

        // MAINNET: Legacy P2PKH (1..., 26-35 chars)
        if address.starts_with('1') {
            if address.len() < 26 || address.len() > 35 {
                return Err(OrchestrationError::InvalidConfig(format!(
                    "Invalid P2PKH length: {} (expected 26-35)", address.len()
                )));
            }
            return Ok(Self::LegacyP2PKH);
        }

        // MAINNET: P2SH/Nested SegWit (3..., 26-35 chars)
        if address.starts_with('3') {
            if address.len() < 26 || address.len() > 35 {
                return Err(OrchestrationError::InvalidConfig(format!(
                    "Invalid P2SH length: {} (expected 26-35)", address.len()
                )));
            }
            return Ok(Self::LegacyP2SH);
        }

        // TESTNET: Native SegWit (tb1q..., 42-62 chars)
        if address.starts_with("tb1q") && address.len() >= 42 && address.len() <= 62 {
            return Ok(Self::NativeSegWit);
        }

        // TESTNET: Taproot (tb1p..., 62 chars)
        if address.starts_with("tb1p") {
            if address.len() != 62 {
                return Err(OrchestrationError::InvalidConfig(format!(
                    "Invalid testnet Taproot length: {} (expected 62)", address.len()
                )));
            }
            return Ok(Self::Taproot);
        }

        // TESTNET: P2PKH (m.../n..., 26-35 chars)
        if address.starts_with('m') || address.starts_with('n') {
            if address.len() < 26 || address.len() > 35 {
                return Err(OrchestrationError::InvalidConfig(format!(
                    "Invalid testnet P2PKH length: {} (expected 26-35)", address.len()
                )));
            }
            return Ok(Self::LegacyP2PKH);
        }

        // TESTNET: P2SH (2..., 26-35 chars)
        if address.starts_with('2') {
            if address.len() < 26 || address.len() > 35 {
                return Err(OrchestrationError::InvalidConfig(format!(
                    "Invalid testnet P2SH length: {} (expected 26-35)", address.len()
                )));
            }
            return Ok(Self::LegacyP2SH);
        }

        Err(OrchestrationError::InvalidConfig(format!(
            "Unsupported address format: {} (prefix: {})",
            address, &address[..address.len().min(4)]
        )))
    }

    /// Get required signature protocol for this address type
    pub fn required_protocol(&self) -> SignatureProtocol {
        match self {
            Self::Taproot => SignatureProtocol::FROST,
            Self::NativeSegWit
            | Self::LegacyP2PKH
            | Self::LegacyP2SH
            | Self::NestedSegWit => SignatureProtocol::CGGMP24,
        }
    }

    /// Get human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::NativeSegWit => "Native SegWit (P2WPKH/P2WSH)",
            Self::Taproot => "Taproot (P2TR)",
            Self::LegacyP2PKH => "Legacy Pay-to-PubKey-Hash (P2PKH)",
            Self::LegacyP2SH => "Pay-to-Script-Hash (P2SH)",
            Self::NestedSegWit => "Nested SegWit (P2WPKH-in-P2SH)",
        }
    }

    /// Check if this address type supports batching
    pub fn supports_batching(&self) -> bool {
        matches!(self, Self::NativeSegWit | Self::Taproot)
    }

    /// Get estimated transaction fee rate multiplier
    pub fn fee_multiplier(&self) -> f64 {
        match self {
            Self::NativeSegWit => 1.0,    // Baseline (lowest fees)
            Self::Taproot => 0.95,         // Slightly cheaper than SegWit
            Self::LegacyP2PKH => 1.5,      // 50% higher fees
            Self::LegacyP2SH => 1.4,       // 40% higher fees
            Self::NestedSegWit => 1.2,     // 20% higher fees
        }
    }
}

/// Protocol selection result with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolSelection {
    /// Selected signing protocol
    pub protocol: SignatureProtocol,
    /// Detected address type
    pub address_type: BitcoinAddressType,
    /// Whether presignature pool is required
    pub requires_presignature_pool: bool,
    /// Estimated signing time in milliseconds
    pub estimated_signing_time_ms: u64,
    /// Estimated transaction fee multiplier
    pub fee_multiplier: f64,
    /// Human-readable address description
    pub address_description: String,
}

impl ProtocolSelection {
    /// Select protocol based on recipient address
    pub fn select(recipient_address: &str) -> Result<Self> {
        let address_type = BitcoinAddressType::detect(recipient_address)?;
        let protocol = address_type.required_protocol();

        let (requires_presignature_pool, estimated_signing_time_ms) = match protocol {
            SignatureProtocol::CGGMP24 => (true, 450),   // With pool: <500ms
            SignatureProtocol::FROST => (false, 800),    // No pool yet: ~800ms
        };

        Ok(Self {
            protocol,
            address_type,
            requires_presignature_pool,
            estimated_signing_time_ms,
            fee_multiplier: address_type.fee_multiplier(),
            address_description: address_type.description().to_string(),
        })
    }
}

/// Protocol Router for managing multi-protocol signing
pub struct ProtocolRouter {
    /// CGGMP24 protocol enabled
    cggmp24_enabled: bool,
    /// FROST protocol enabled
    frost_enabled: bool,
}

impl ProtocolRouter {
    /// Create new protocol router
    pub fn new(cggmp24_enabled: bool, frost_enabled: bool) -> Self {
        Self {
            cggmp24_enabled,
            frost_enabled,
        }
    }

    /// Route transaction to appropriate signing protocol
    pub fn route(&self, recipient_address: &str) -> Result<ProtocolSelection> {
        let selection = ProtocolSelection::select(recipient_address)?;

        // Verify protocol is available
        match selection.protocol {
            SignatureProtocol::CGGMP24 if !self.cggmp24_enabled => {
                return Err(OrchestrationError::InvalidConfig(
                    "CGGMP24 protocol not available but required for this address type".to_string(),
                ));
            }
            SignatureProtocol::FROST if !self.frost_enabled => {
                return Err(OrchestrationError::InvalidConfig(
                    "FROST protocol not available but required for Taproot addresses".to_string(),
                ));
            }
            _ => {}
        }

        Ok(selection)
    }

    /// Check if protocol is available
    pub fn is_protocol_available(&self, protocol: SignatureProtocol) -> bool {
        match protocol {
            SignatureProtocol::CGGMP24 => self.cggmp24_enabled,
            SignatureProtocol::FROST => self.frost_enabled,
        }
    }

    /// Get list of supported address types
    pub fn supported_address_types(&self) -> Vec<BitcoinAddressType> {
        let mut types = Vec::new();

        if self.cggmp24_enabled {
            types.push(BitcoinAddressType::NativeSegWit);
            types.push(BitcoinAddressType::LegacyP2PKH);
            types.push(BitcoinAddressType::LegacyP2SH);
            types.push(BitcoinAddressType::NestedSegWit);
        }

        if self.frost_enabled {
            types.push(BitcoinAddressType::Taproot);
        }

        types
    }
}

impl Default for ProtocolRouter {
    fn default() -> Self {
        Self::new(true, false) // Only CGGMP24 enabled by default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_type_detection() {
        // Mainnet SegWit
        assert_eq!(
            BitcoinAddressType::detect("bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq").unwrap(),
            BitcoinAddressType::NativeSegWit
        );

        // Mainnet Taproot
        assert_eq!(
            BitcoinAddressType::detect("bc1p5cyxnuxmeuwuvkwfem96lqzszd02n6xdcjrs20cac6yqjjwudpxqkedrcr").unwrap(),
            BitcoinAddressType::Taproot
        );

        // Legacy P2PKH
        assert_eq!(
            BitcoinAddressType::detect("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa").unwrap(),
            BitcoinAddressType::LegacyP2PKH
        );

        // Legacy P2SH
        assert_eq!(
            BitcoinAddressType::detect("3J98t1WpEZ73CNmYviecrnyiWrnqRhWNLy").unwrap(),
            BitcoinAddressType::LegacyP2SH
        );

        // Testnet SegWit
        assert_eq!(
            BitcoinAddressType::detect("tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx").unwrap(),
            BitcoinAddressType::NativeSegWit
        );

        // Testnet Taproot
        assert_eq!(
            BitcoinAddressType::detect("tb1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqzk5jj0").unwrap(),
            BitcoinAddressType::Taproot
        );
    }

    #[test]
    fn test_protocol_selection() {
        // SegWit should use CGGMP24
        let selection = ProtocolSelection::select("bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq").unwrap();
        assert_eq!(selection.protocol, SignatureProtocol::CGGMP24);
        assert!(selection.requires_presignature_pool);

        // Taproot should use FROST
        let selection = ProtocolSelection::select("bc1p5cyxnuxmeuwuvkwfem96lqzszd02n6xdcjrs20cac6yqjjwudpxqkedrcr").unwrap();
        assert_eq!(selection.protocol, SignatureProtocol::FROST);
        assert!(!selection.requires_presignature_pool);

        // Legacy should use CGGMP24
        let selection = ProtocolSelection::select("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa").unwrap();
        assert_eq!(selection.protocol, SignatureProtocol::CGGMP24);
    }

    #[test]
    fn test_protocol_router() {
        let router = ProtocolRouter::new(true, true);

        // Should route SegWit to CGGMP24
        let selection = router.route("bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq").unwrap();
        assert_eq!(selection.protocol, SignatureProtocol::CGGMP24);

        // Should route Taproot to FROST
        let selection = router.route("bc1p5cyxnuxmeuwuvkwfem96lqzszd02n6xdcjrs20cac6yqjjwudpxqkedrcr").unwrap();
        assert_eq!(selection.protocol, SignatureProtocol::FROST);
    }

    #[test]
    fn test_protocol_availability() {
        let router = ProtocolRouter::new(true, false);

        // CGGMP24 should work
        assert!(router.route("bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq").is_ok());

        // FROST should fail (not enabled)
        assert!(router.route("bc1p5cyxnuxmeuwuvkwfem96lqzszd02n6xdcjrs20cac6yqjjwudpxqkedrcr").is_err());
    }

    #[test]
    fn test_supported_address_types() {
        let router = ProtocolRouter::new(true, true);
        let types = router.supported_address_types();

        assert!(types.contains(&BitcoinAddressType::NativeSegWit));
        assert!(types.contains(&BitcoinAddressType::Taproot));
        assert!(types.contains(&BitcoinAddressType::LegacyP2PKH));
    }

    #[test]
    fn test_fee_multipliers() {
        assert_eq!(BitcoinAddressType::NativeSegWit.fee_multiplier(), 1.0);
        assert_eq!(BitcoinAddressType::Taproot.fee_multiplier(), 0.95);
        assert_eq!(BitcoinAddressType::LegacyP2PKH.fee_multiplier(), 1.5);
    }
}
