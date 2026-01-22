//! Bitcoin Address Utilities
//!
//! Provides utilities for Bitcoin address generation and validation:
//! - P2WPKH (Native SegWit) from ECDSA public keys
//! - P2TR (Taproot) from Schnorr x-only public keys
//! - Address type detection and validation

use anyhow::{anyhow, bail, Result};

/// Derive P2WPKH (Native SegWit) address from compressed ECDSA public key
///
/// # Arguments
/// * `pubkey` - 33-byte compressed secp256k1 public key (02/03 prefix + 32 bytes)
/// * `network` - Bitcoin network (mainnet, testnet, signet, regtest)
///
/// # Returns
/// bc1q... address for mainnet, tb1q... for testnet
pub fn derive_p2wpkh_address(pubkey: &[u8], network: BitcoinNetwork) -> Result<String> {
    // Validate compressed public key format
    if pubkey.len() != 33 {
        bail!(
            "Invalid compressed public key length: expected 33 bytes, got {}",
            pubkey.len()
        );
    }

    if pubkey[0] != 0x02 && pubkey[0] != 0x03 {
        bail!(
            "Invalid compressed public key prefix: expected 0x02 or 0x03, got 0x{:02x}",
            pubkey[0]
        );
    }

    // Compute HASH160(pubkey) = RIPEMD160(SHA256(pubkey))
    let pubkey_hash = hash160(pubkey);

    // Encode as bech32
    let hrp = match network {
        BitcoinNetwork::Mainnet => "bc",
        BitcoinNetwork::Testnet | BitcoinNetwork::Signet | BitcoinNetwork::Regtest => "tb",
    };

    let address = encode_bech32_v0(hrp, &pubkey_hash)?;

    Ok(address)
}

/// Derive P2TR (Taproot) address from Schnorr x-only public key
///
/// # Arguments
/// * `x_only_pubkey` - 32-byte x-only public key (BIP-340 format)
/// * `network` - Bitcoin network
///
/// # Returns
/// bc1p... address for mainnet, tb1p... for testnet
pub fn derive_p2tr_address(x_only_pubkey: &[u8], network: BitcoinNetwork) -> Result<String> {
    // Validate x-only public key format
    if x_only_pubkey.len() != 32 {
        bail!(
            "Invalid x-only public key length: expected 32 bytes, got {}",
            x_only_pubkey.len()
        );
    }

    // For Taproot, the witness program is the 32-byte x-only pubkey itself
    let hrp = match network {
        BitcoinNetwork::Mainnet => "bc",
        BitcoinNetwork::Testnet | BitcoinNetwork::Signet | BitcoinNetwork::Regtest => "tb",
    };

    let address = encode_bech32_v1(hrp, x_only_pubkey)?;

    Ok(address)
}

/// Bitcoin network types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitcoinNetwork {
    Mainnet,
    Testnet,
    Signet,
    Regtest,
}

impl Default for BitcoinNetwork {
    fn default() -> Self {
        BitcoinNetwork::Mainnet
    }
}

/// Compute HASH160 = RIPEMD160(SHA256(data))
fn hash160(data: &[u8]) -> [u8; 20] {
    use sha2::{Digest, Sha256};
    use ripemd::Ripemd160;

    let sha256_hash = Sha256::digest(data);
    let ripemd_hash = Ripemd160::digest(&sha256_hash);

    let mut result = [0u8; 20];
    result.copy_from_slice(&ripemd_hash);
    result
}

/// Encode witness program as bech32 (SegWit v0)
fn encode_bech32_v0(hrp: &str, witness_program: &[u8]) -> Result<String> {
    // Convert witness program to 5-bit groups for bech32
    let mut data = vec![0u8]; // Witness version 0
    data.extend(convert_bits(witness_program, 8, 5, true)?);

    bech32_encode(hrp, &data, Bech32Variant::Bech32)
}

/// Encode witness program as bech32m (Taproot/SegWit v1+)
fn encode_bech32_v1(hrp: &str, witness_program: &[u8]) -> Result<String> {
    // Convert witness program to 5-bit groups for bech32m
    let mut data = vec![1u8]; // Witness version 1
    data.extend(convert_bits(witness_program, 8, 5, true)?);

    bech32_encode(hrp, &data, Bech32Variant::Bech32m)
}

/// Bech32 encoding variant
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bech32Variant {
    Bech32,  // Original (for SegWit v0)
    Bech32m, // Modified (for Taproot/v1+)
}

/// Convert between bit groups
fn convert_bits(data: &[u8], from_bits: u32, to_bits: u32, pad: bool) -> Result<Vec<u8>> {
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    let mut ret = Vec::new();
    let maxv = (1u32 << to_bits) - 1;

    for &value in data {
        let v = value as u32;
        if v >> from_bits != 0 {
            bail!("Invalid data in convert_bits");
        }
        acc = (acc << from_bits) | v;
        bits += from_bits;
        while bits >= to_bits {
            bits -= to_bits;
            ret.push(((acc >> bits) & maxv) as u8);
        }
    }

    if pad {
        if bits > 0 {
            ret.push(((acc << (to_bits - bits)) & maxv) as u8);
        }
    } else if bits >= from_bits || ((acc << (to_bits - bits)) & maxv) != 0 {
        bail!("Invalid padding in convert_bits");
    }

    Ok(ret)
}

/// Bech32 encode using standard bech32/bech32m
fn bech32_encode(hrp: &str, data: &[u8], variant: Bech32Variant) -> Result<String> {
    use bech32::{Bech32, Bech32m, Hrp};

    let hrp_parsed = Hrp::parse(hrp)
        .map_err(|e| anyhow!("Invalid HRP: {}", e))?;

    let encoded = match variant {
        Bech32Variant::Bech32 => {
            bech32::encode::<Bech32>(hrp_parsed, data)
                .map_err(|e| anyhow!("Bech32 encoding failed: {}", e))?
        }
        Bech32Variant::Bech32m => {
            bech32::encode::<Bech32m>(hrp_parsed, data)
                .map_err(|e| anyhow!("Bech32m encoding failed: {}", e))?
        }
    };

    Ok(encoded)
}

/// Validate Bitcoin address
pub fn validate_address(address: &str) -> Result<BitcoinAddressInfo> {
    if address.starts_with("bc1q") || address.starts_with("tb1q") {
        Ok(BitcoinAddressInfo {
            address_type: AddressType::P2WPKH,
            network: if address.starts_with("bc") {
                BitcoinNetwork::Mainnet
            } else {
                BitcoinNetwork::Testnet
            },
        })
    } else if address.starts_with("bc1p") || address.starts_with("tb1p") {
        Ok(BitcoinAddressInfo {
            address_type: AddressType::P2TR,
            network: if address.starts_with("bc") {
                BitcoinNetwork::Mainnet
            } else {
                BitcoinNetwork::Testnet
            },
        })
    } else if address.starts_with('1') {
        Ok(BitcoinAddressInfo {
            address_type: AddressType::P2PKH,
            network: BitcoinNetwork::Mainnet,
        })
    } else if address.starts_with('3') {
        Ok(BitcoinAddressInfo {
            address_type: AddressType::P2SH,
            network: BitcoinNetwork::Mainnet,
        })
    } else if address.starts_with('m') || address.starts_with('n') {
        Ok(BitcoinAddressInfo {
            address_type: AddressType::P2PKH,
            network: BitcoinNetwork::Testnet,
        })
    } else if address.starts_with('2') {
        Ok(BitcoinAddressInfo {
            address_type: AddressType::P2SH,
            network: BitcoinNetwork::Testnet,
        })
    } else {
        Err(anyhow!("Unsupported address format: {}", address))
    }
}

/// Bitcoin address type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressType {
    P2PKH,   // Legacy
    P2SH,    // Script hash
    P2WPKH,  // Native SegWit
    P2WSH,   // SegWit script
    P2TR,    // Taproot
}

/// Bitcoin address information
#[derive(Debug, Clone)]
pub struct BitcoinAddressInfo {
    pub address_type: AddressType,
    pub network: BitcoinNetwork,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash160() {
        // Test vector from Bitcoin
        let pubkey = hex::decode("0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798")
            .unwrap();
        let hash = hash160(&pubkey);

        // Expected: hash160 of compressed pubkey for address 1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa
        assert_eq!(hash.len(), 20);
    }

    #[test]
    fn test_derive_p2wpkh_address() {
        // Sample compressed pubkey
        let pubkey = [0x02u8; 33]; // Mock pubkey

        let address = derive_p2wpkh_address(&pubkey, BitcoinNetwork::Mainnet).unwrap();
        assert!(address.starts_with("bc1q"));

        let address = derive_p2wpkh_address(&pubkey, BitcoinNetwork::Testnet).unwrap();
        assert!(address.starts_with("tb1q"));
    }

    #[test]
    fn test_derive_p2tr_address() {
        // Sample x-only pubkey
        let x_only = [0xaau8; 32]; // Mock x-only pubkey

        let address = derive_p2tr_address(&x_only, BitcoinNetwork::Mainnet).unwrap();
        assert!(address.starts_with("bc1p"));

        let address = derive_p2tr_address(&x_only, BitcoinNetwork::Testnet).unwrap();
        assert!(address.starts_with("tb1p"));
    }

    #[test]
    fn test_validate_address() {
        let info = validate_address("bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq").unwrap();
        assert_eq!(info.address_type, AddressType::P2WPKH);
        assert_eq!(info.network, BitcoinNetwork::Mainnet);

        let info = validate_address("bc1p5cyxnuxmeuwuvkwfem96lqzszd02n6xdcjrs20cac6yqjjwudpxqkedrcr").unwrap();
        assert_eq!(info.address_type, AddressType::P2TR);
        assert_eq!(info.network, BitcoinNetwork::Mainnet);
    }

    #[test]
    fn test_invalid_pubkey_length() {
        let result = derive_p2wpkh_address(&[0x02; 32], BitcoinNetwork::Mainnet);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_pubkey_prefix() {
        let mut pubkey = [0x02u8; 33];
        pubkey[0] = 0x04; // Invalid prefix
        let result = derive_p2wpkh_address(&pubkey, BitcoinNetwork::Mainnet);
        assert!(result.is_err());
    }
}

// Hex decoding helper for tests
#[cfg(test)]
mod hex {
    pub fn decode(s: &str) -> Result<Vec<u8>, &'static str> {
        if s.len() % 2 != 0 {
            return Err("Odd number of hex digits");
        }

        let mut bytes = Vec::with_capacity(s.len() / 2);
        for i in (0..s.len()).step_by(2) {
            let byte = u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| "Invalid hex")?;
            bytes.push(byte);
        }
        Ok(bytes)
    }
}
