//! Cryptographic Utilities
//!
//! Provides cryptographic operations for MPC wallet:
//! - Bitcoin sighash computation
//! - ECDSA signature verification (CGGMP24)
//! - Schnorr signature verification (FROST/BIP-340)
//! - Key serialization helpers

use anyhow::{anyhow, bail, Result};
use sha2::{Digest, Sha256};
use bitcoin::{
    Transaction, TxOut, ScriptBuf, Amount,
    sighash::{SighashCache, EcdsaSighashType, TapSighashType},
    secp256k1::{Secp256k1, Message, ecdsa::Signature as EcdsaSignature, schnorr::Signature as SchnorrSignature},
    PublicKey, XOnlyPublicKey,
    hashes::Hash,
};

/// Compute Bitcoin sighash for ECDSA signing (SIGHASH_ALL)
///
/// # Arguments
/// * `unsigned_tx` - Unsigned transaction bytes
/// * `input_index` - Which input we're signing
/// * `script_pubkey` - Script pubkey of the UTXO being spent
/// * `amount` - Amount in satoshis of the UTXO being spent
///
/// # Returns
/// 32-byte sighash to be signed
pub fn compute_sighash_ecdsa(
    unsigned_tx: &[u8],
    input_index: usize,
    script_pubkey: &[u8],
    amount: u64,
) -> Result<[u8; 32]> {
    // Parse transaction
    let tx: Transaction = bitcoin::consensus::deserialize(unsigned_tx)
        .map_err(|e| anyhow!("Failed to deserialize transaction: {}", e))?;

    // Validate input index
    if input_index >= tx.input.len() {
        bail!("Input index {} out of bounds (tx has {} inputs)", input_index, tx.input.len());
    }

    // Parse script pubkey
    let script = ScriptBuf::from_bytes(script_pubkey.to_vec());

    // Create prevout for BIP-143 sighash
    let prevout = TxOut {
        value: Amount::from_sat(amount),
        script_pubkey: script,
    };

    // Compute BIP-143 sighash for SegWit
    let mut sighash_cache = SighashCache::new(&tx);
    let sighash = sighash_cache
        .p2wpkh_signature_hash(input_index, &prevout.script_pubkey, prevout.value, EcdsaSighashType::All)
        .map_err(|e| anyhow!("Failed to compute sighash: {}", e))?;

    Ok(*sighash.as_byte_array())
}

/// Compute Bitcoin sighash for Schnorr signing (BIP-341 Taproot)
///
/// # Arguments
/// * `unsigned_tx` - Unsigned transaction bytes
/// * `input_index` - Which input we're signing
/// * `prevouts` - All prevout amounts and scriptPubKeys
///
/// # Returns
/// 32-byte BIP-341 tagged hash to be signed
pub fn compute_sighash_taproot(
    unsigned_tx: &[u8],
    input_index: usize,
    prevouts: &[TaprootPrevout],
) -> Result<[u8; 32]> {
    use bitcoin::sighash::Prevouts;

    // Parse transaction
    let tx: Transaction = bitcoin::consensus::deserialize(unsigned_tx)
        .map_err(|e| anyhow!("Failed to deserialize transaction: {}", e))?;

    // Validate input index
    if input_index >= tx.input.len() {
        bail!("Input index {} out of bounds (tx has {} inputs)", input_index, tx.input.len());
    }

    // Convert prevouts
    let prevout_vec: Vec<TxOut> = prevouts
        .iter()
        .map(|p| TxOut {
            value: Amount::from_sat(p.amount),
            script_pubkey: ScriptBuf::from_bytes(p.script_pubkey.clone()),
        })
        .collect();

    // Compute BIP-341 Taproot sighash
    let mut sighash_cache = SighashCache::new(&tx);
    let prevouts_ref = Prevouts::All(&prevout_vec);

    let sighash = sighash_cache
        .taproot_key_spend_signature_hash(input_index, &prevouts_ref, TapSighashType::Default)
        .map_err(|e| anyhow!("Failed to compute Taproot sighash: {}", e))?;

    Ok(*sighash.as_byte_array())
}

/// Taproot prevout information
#[derive(Debug, Clone)]
pub struct TaprootPrevout {
    pub amount: u64,
    pub script_pubkey: Vec<u8>,
}

/// Verify ECDSA signature (DER-encoded or raw 64-byte format)
///
/// # Arguments
/// * `message_hash` - 32-byte message hash
/// * `signature` - DER-encoded or raw ECDSA signature
/// * `pubkey` - 33-byte compressed public key
///
/// # Returns
/// true if signature is valid
pub fn verify_ecdsa_signature(
    message_hash: &[u8; 32],
    signature: &[u8],
    pubkey: &[u8],
) -> Result<bool> {
    // Validate public key
    if pubkey.len() != 33 {
        bail!("Invalid public key length: expected 33, got {}", pubkey.len());
    }

    // Parse public key
    let secp = Secp256k1::verification_only();
    let public_key = PublicKey::from_slice(pubkey)
        .map_err(|e| anyhow!("Invalid public key: {}", e))?;

    // Parse signature (try DER first, then raw)
    let ecdsa_sig = if signature.len() >= 70 && signature.len() <= 73 && signature[0] == 0x30 {
        // DER-encoded (might have SIGHASH_ALL byte at end)
        let sig_bytes = if signature.len() > 64 && signature[signature.len() - 1] == 0x01 {
            &signature[..signature.len() - 1] // Remove SIGHASH_ALL
        } else {
            signature
        };
        EcdsaSignature::from_der(sig_bytes)
            .map_err(|e| anyhow!("Invalid DER signature: {}", e))?
    } else if signature.len() == 64 {
        // Raw compact signature (r || s)
        EcdsaSignature::from_compact(signature)
            .map_err(|e| anyhow!("Invalid compact signature: {}", e))?
    } else {
        bail!("Invalid signature length: expected 64 or DER-encoded, got {}", signature.len());
    };

    // Create message
    let message = Message::from_digest(*message_hash);

    // Verify signature
    match secp.verify_ecdsa(&message, &ecdsa_sig, &public_key.inner) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

/// Verify Schnorr signature (BIP-340)
///
/// # Arguments
/// * `message` - 32-byte message (already hashed)
/// * `signature` - 64-byte Schnorr signature (r || s)
/// * `x_only_pubkey` - 32-byte x-only public key
///
/// # Returns
/// true if signature is valid
pub fn verify_schnorr_signature(
    message: &[u8; 32],
    signature: &[u8],
    x_only_pubkey: &[u8],
) -> Result<bool> {
    // Validate inputs
    if x_only_pubkey.len() != 32 {
        bail!("Invalid x-only public key length: expected 32, got {}", x_only_pubkey.len());
    }

    if signature.len() != 64 {
        bail!("Invalid Schnorr signature length: expected 64, got {}", signature.len());
    }

    // Parse x-only public key
    let secp = Secp256k1::verification_only();
    let xonly_pubkey = XOnlyPublicKey::from_slice(x_only_pubkey)
        .map_err(|e| anyhow!("Invalid x-only public key: {}", e))?;

    // Parse Schnorr signature
    let schnorr_sig = SchnorrSignature::from_slice(signature)
        .map_err(|e| anyhow!("Invalid Schnorr signature: {}", e))?;

    // Create message
    let msg = Message::from_digest(*message);

    // Verify BIP-340 Schnorr signature
    match secp.verify_schnorr(&schnorr_sig, &msg, &xonly_pubkey) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

/// Convert DER-encoded signature to Bitcoin format (with SIGHASH_ALL flag)
pub fn der_to_bitcoin_signature(der_sig: &[u8]) -> Vec<u8> {
    let mut sig = der_sig.to_vec();
    sig.push(0x01); // SIGHASH_ALL
    sig
}

/// Serialize compressed public key (33 bytes)
pub fn serialize_compressed_pubkey(x: &[u8; 32], is_odd: bool) -> [u8; 33] {
    let mut result = [0u8; 33];
    result[0] = if is_odd { 0x03 } else { 0x02 };
    result[1..].copy_from_slice(x);
    result
}

/// Parse compressed public key
pub fn parse_compressed_pubkey(pubkey: &[u8]) -> Result<([u8; 32], bool)> {
    if pubkey.len() != 33 {
        return Err(anyhow!("Invalid compressed pubkey length: {}", pubkey.len()));
    }

    let is_odd = match pubkey[0] {
        0x02 => false,
        0x03 => true,
        _ => return Err(anyhow!("Invalid compressed pubkey prefix: 0x{:02x}", pubkey[0])),
    };

    let mut x = [0u8; 32];
    x.copy_from_slice(&pubkey[1..]);

    Ok((x, is_odd))
}

/// Double SHA256 hash (used in Bitcoin)
pub fn double_sha256(data: &[u8]) -> [u8; 32] {
    let first_hash = Sha256::digest(data);
    let second_hash = Sha256::digest(&first_hash);

    let mut result = [0u8; 32];
    result.copy_from_slice(&second_hash);
    result
}

/// Tagged SHA256 hash (BIP-340 style)
pub fn tagged_hash(tag: &[u8], data: &[u8]) -> [u8; 32] {
    let tag_hash = Sha256::digest(tag);

    let mut hasher = Sha256::new();
    hasher.update(&tag_hash);
    hasher.update(&tag_hash);
    hasher.update(data);

    let mut result = [0u8; 32];
    result.copy_from_slice(&hasher.finalize());
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_sighash_ecdsa() {
        let unsigned_tx = vec![0x01, 0x02, 0x03];
        let script_pubkey = vec![0x76, 0xa9, 0x14]; // OP_DUP OP_HASH160 OP_PUSH20
        let hash = compute_sighash_ecdsa(&unsigned_tx, 0, &script_pubkey, 100_000).unwrap();
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_compute_sighash_taproot() {
        let unsigned_tx = vec![0x01, 0x02, 0x03];
        let prevouts = vec![TaprootPrevout {
            amount: 100_000,
            script_pubkey: vec![0x51, 0x20], // OP_1 OP_PUSH32
        }];
        let hash = compute_sighash_taproot(&unsigned_tx, 0, &prevouts).unwrap();
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_serialize_compressed_pubkey() {
        let x = [0xaau8; 32];
        let pubkey = serialize_compressed_pubkey(&x, false);
        assert_eq!(pubkey.len(), 33);
        assert_eq!(pubkey[0], 0x02);

        let pubkey = serialize_compressed_pubkey(&x, true);
        assert_eq!(pubkey[0], 0x03);
    }

    #[test]
    fn test_parse_compressed_pubkey() {
        let mut pubkey = [0u8; 33];
        pubkey[0] = 0x02;
        let (x, is_odd) = parse_compressed_pubkey(&pubkey).unwrap();
        assert_eq!(x.len(), 32);
        assert!(!is_odd);

        pubkey[0] = 0x03;
        let (_, is_odd) = parse_compressed_pubkey(&pubkey).unwrap();
        assert!(is_odd);
    }

    #[test]
    fn test_double_sha256() {
        let data = b"hello world";
        let hash = double_sha256(data);
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_tagged_hash() {
        let tag = b"TapSighash";
        let data = b"test data";
        let hash = tagged_hash(tag, data);
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_der_to_bitcoin_signature() {
        let der_sig = vec![0x30, 0x44, 0x02, 0x20]; // Mock DER prefix
        let btc_sig = der_to_bitcoin_signature(&der_sig);
        assert_eq!(btc_sig.last(), Some(&0x01)); // SIGHASH_ALL
    }
}
