//! Bitcoin address derivation utilities.
//!
//! Supports:
//! - P2WPKH (Native SegWit) addresses
//! - P2PKH (Legacy) addresses
//! - P2TR (Taproot) addresses

use bitcoin::{Address, CompressedPublicKey, Network, PublicKey};
use sha2::{Digest, Sha256};

use common::MpcWalletError;

// ============================================================================
// Basic Address Derivation (non-HD)
// ============================================================================

/// Derive a Bitcoin address from a compressed public key.
///
/// Uses P2WPKH (native SegWit) address format.
pub fn derive_bitcoin_address(
    public_key_bytes: &[u8],
    network: Network,
) -> Result<String, MpcWalletError> {
    tracing::debug!("derive_bitcoin_address called");
    tracing::debug!("  Input bytes length: {}", public_key_bytes.len());
    tracing::debug!("  Network: {:?}", network);
    tracing::trace!("  Public key bytes: {}", hex::encode(public_key_bytes));

    if public_key_bytes.len() != 33 {
        tracing::error!(
            "Invalid public key length: expected 33, got {}",
            public_key_bytes.len()
        );
        return Err(MpcWalletError::InvalidPublicKey(format!(
            "Expected 33 bytes, got {}",
            public_key_bytes.len()
        )));
    }

    let compressed = CompressedPublicKey::from_slice(public_key_bytes).map_err(|e| {
        tracing::error!("Failed to parse compressed public key: {}", e);
        MpcWalletError::InvalidPublicKey(e.to_string())
    })?;

    let address = Address::p2wpkh(&compressed, network);
    tracing::debug!("Address created: {}", address);

    Ok(address.to_string())
}

/// Derive a legacy Bitcoin address (P2PKH) from a compressed public key.
pub fn derive_bitcoin_address_legacy(
    public_key_bytes: &[u8],
    network: Network,
) -> Result<String, MpcWalletError> {
    tracing::debug!("derive_bitcoin_address_legacy called");

    if public_key_bytes.len() != 33 {
        return Err(MpcWalletError::InvalidPublicKey(format!(
            "Expected 33 bytes, got {}",
            public_key_bytes.len()
        )));
    }

    let compressed = CompressedPublicKey::from_slice(public_key_bytes)
        .map_err(|e| MpcWalletError::InvalidPublicKey(e.to_string()))?;

    let pubkey = PublicKey::from(compressed);
    let address = Address::p2pkh(pubkey, network);

    Ok(address.to_string())
}

/// Derive a Taproot Bitcoin address (P2TR) from a compressed public key.
///
/// This creates a key-path only address (no script tree).
/// The address will start with tb1p (testnet) or bc1p (mainnet).
pub fn derive_bitcoin_address_taproot(
    public_key_bytes: &[u8],
    network: Network,
) -> Result<String, MpcWalletError> {
    use bitcoin::key::TweakedPublicKey;

    tracing::debug!("derive_bitcoin_address_taproot called (no tap tweak for FROST)");

    if public_key_bytes.len() != 33 {
        return Err(MpcWalletError::InvalidPublicKey(format!(
            "Expected 33 bytes (compressed pubkey), got {}",
            public_key_bytes.len()
        )));
    }

    // Parse the compressed public key
    let compressed = CompressedPublicKey::from_slice(public_key_bytes)
        .map_err(|e| MpcWalletError::InvalidPublicKey(e.to_string()))?;

    // Get the x-only public key (32 bytes, x-coordinate only)
    // This drops the parity byte
    let (x_only, _parity) = compressed.0.x_only_public_key();

    // For FROST compatibility: DO NOT apply tap tweak
    // FROST signs with the untweaked key, so we use the raw x-only key as the output key
    let tweaked_pubkey = TweakedPublicKey::dangerous_assume_tweaked(x_only);

    // Create the Taproot address
    let address = Address::p2tr_tweaked(tweaked_pubkey, network);
    tracing::debug!("Taproot address created (untweaked for FROST): {}", address);

    Ok(address.to_string())
}

/// Derive a Taproot Bitcoin address from an x-only public key (32 bytes).
///
/// This is useful when you already have the x-only key from FROST keygen.
///
/// NOTE: For FROST threshold signing compatibility, we do NOT apply the BIP-341
/// tap tweak. FROST signs with the untweaked key, so the address must use the
/// untweaked public key. This is safe for key-path-only spending.
pub fn derive_bitcoin_address_taproot_from_xonly(
    x_only_bytes: &[u8],
    network: Network,
) -> Result<String, MpcWalletError> {
    use bitcoin::key::TweakedPublicKey;
    use bitcoin::secp256k1::XOnlyPublicKey;

    tracing::debug!("derive_bitcoin_address_taproot_from_xonly called (no tap tweak for FROST)");

    if x_only_bytes.len() != 32 {
        return Err(MpcWalletError::InvalidPublicKey(format!(
            "Expected 32 bytes (x-only pubkey), got {}",
            x_only_bytes.len()
        )));
    }

    // Parse the x-only public key
    let x_only = XOnlyPublicKey::from_slice(x_only_bytes).map_err(|e| {
        MpcWalletError::InvalidPublicKey(format!("Invalid x-only public key: {}", e))
    })?;

    // For FROST compatibility: DO NOT apply tap tweak
    // FROST signs with the untweaked key, so we use the raw x-only key as the output key
    // This uses dangerous_assume_tweaked to wrap the untweaked key
    let tweaked_pubkey = TweakedPublicKey::dangerous_assume_tweaked(x_only);

    // Create the Taproot address
    let address = Address::p2tr_tweaked(tweaked_pubkey, network);
    tracing::debug!("Taproot address created (untweaked for FROST): {}", address);

    Ok(address.to_string())
}

/// Derive an Ethereum address from a public key.
///
/// Ethereum uses keccak256(uncompressed_pubkey[1..65])[12..32].
pub fn derive_ethereum_address(public_key_bytes: &[u8]) -> Result<String, MpcWalletError> {
    tracing::debug!("derive_ethereum_address called");

    // For proper Ethereum, we need to decompress and use keccak256
    // For now, using SHA256 as placeholder (would need keccak256 crate for real impl)

    if public_key_bytes.len() == 33 {
        // Decompress the public key first
        let uncompressed = decompress_pubkey(public_key_bytes)?;

        // Ethereum address = last 20 bytes of keccak256(uncompressed[1..65])
        // Using SHA256 as placeholder for keccak256
        let mut hasher = Sha256::new();
        hasher.update(&uncompressed[1..65]);
        let hash = hasher.finalize();
        let address = format!("0x{}", hex::encode(&hash[12..32]));
        Ok(address)
    } else if public_key_bytes.len() == 65 {
        let mut hasher = Sha256::new();
        hasher.update(&public_key_bytes[1..]);
        let hash = hasher.finalize();
        let address = format!("0x{}", hex::encode(&hash[12..32]));
        Ok(address)
    } else {
        Err(MpcWalletError::InvalidPublicKey(format!(
            "Expected 33 or 65 bytes, got {}",
            public_key_bytes.len()
        )))
    }
}

// ============================================================================
// Elliptic Curve Helpers (secp256k1)
// ============================================================================

/// Add a scalar to a public key point: result = pubkey + scalar * G
pub fn point_add_scalar(pubkey: &[u8; 33], scalar: &[u8]) -> Result<[u8; 33], MpcWalletError> {
    use generic_ec::curves::Secp256k1;
    use generic_ec::{Point, Scalar};

    // Parse the public key point
    let point = Point::<Secp256k1>::from_bytes(pubkey)
        .map_err(|_| MpcWalletError::InvalidPublicKey("Invalid point".into()))?;

    // Parse the scalar
    let scalar = Scalar::<Secp256k1>::from_be_bytes(scalar)
        .map_err(|_| MpcWalletError::Protocol("Invalid scalar".into()))?;

    // Compute: point + scalar * G
    let generator = Point::<Secp256k1>::generator();
    let tweak_point = generator * scalar;
    let result = point + tweak_point;

    // Serialize back to compressed form
    let bytes = result.to_bytes(true);
    let mut output = [0u8; 33];
    output.copy_from_slice(&bytes);

    Ok(output)
}

/// Decompress a secp256k1 public key from 33 bytes to 65 bytes.
pub fn decompress_pubkey(compressed: &[u8]) -> Result<[u8; 65], MpcWalletError> {
    use generic_ec::curves::Secp256k1;
    use generic_ec::Point;

    let point = Point::<Secp256k1>::from_bytes(compressed)
        .map_err(|_| MpcWalletError::InvalidPublicKey("Invalid compressed key".into()))?;

    let uncompressed = point.to_bytes(false);
    let mut output = [0u8; 65];
    output.copy_from_slice(&uncompressed);

    Ok(output)
}

/// Compute HASH160 (SHA256 + RIPEMD160) of data.
pub fn hash160(data: &[u8]) -> [u8; 20] {
    use ripemd::Ripemd160;

    let sha256_hash = Sha256::digest(data);
    let ripemd_hash = Ripemd160::digest(sha256_hash);

    let mut output = [0u8; 20];
    output.copy_from_slice(&ripemd_hash);
    output
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitcoin_address_derivation() {
        // Generator point public key
        let pubkey_hex = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let pubkey_bytes = hex::decode(pubkey_hex).unwrap();

        let address = derive_bitcoin_address(&pubkey_bytes, Network::Bitcoin).unwrap();
        assert!(address.starts_with("bc1"));
    }
}
