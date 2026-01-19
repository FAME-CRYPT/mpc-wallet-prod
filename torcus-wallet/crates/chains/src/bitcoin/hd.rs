//! BIP32 HD public key derivation for MPC wallets.
//!
//! Supports non-hardened derivation only (since we only have public keys).

use bitcoin::Network;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256, Sha512};

use common::MpcWalletError;

use super::address::{
    derive_bitcoin_address, derive_bitcoin_address_legacy, hash160, point_add_scalar,
};

// ============================================================================
// BIP32 Extended Public Key
// ============================================================================

/// An extended public key for BIP32 HD derivation.
///
/// Contains a public key and chain code, allowing derivation of child keys.
/// Note: Only non-hardened derivation is possible with public keys.
#[derive(Debug, Clone)]
pub struct ExtendedPubKey {
    /// The compressed public key (33 bytes).
    pub public_key: [u8; 33],
    /// The chain code (32 bytes).
    pub chain_code: [u8; 32],
    /// Depth in the derivation path.
    pub depth: u8,
    /// Parent fingerprint (first 4 bytes of HASH160 of parent pubkey).
    pub parent_fingerprint: [u8; 4],
    /// Child index.
    pub child_index: u32,
}

impl ExtendedPubKey {
    /// Create an extended public key from a raw public key.
    ///
    /// Since we don't have a BIP39 seed, we derive the chain code deterministically
    /// from the public key itself using: chain_code = SHA256("MPC-BIP32" || pubkey)
    pub fn from_public_key(public_key: &[u8]) -> Result<Self, MpcWalletError> {
        if public_key.len() != 33 {
            return Err(MpcWalletError::InvalidPublicKey(format!(
                "Expected 33 bytes, got {}",
                public_key.len()
            )));
        }

        // Derive chain code deterministically from public key
        let mut hasher = Sha256::new();
        hasher.update(b"MPC-BIP32-CHAINCODE");
        hasher.update(public_key);
        let chain_code: [u8; 32] = hasher.finalize().into();

        let mut pk = [0u8; 33];
        pk.copy_from_slice(public_key);

        tracing::debug!("Created ExtendedPubKey from MPC public key");
        tracing::trace!("  Public key: {}", hex::encode(pk));
        tracing::trace!("  Chain code: {}", hex::encode(chain_code));

        Ok(Self {
            public_key: pk,
            chain_code,
            depth: 0,
            parent_fingerprint: [0u8; 4],
            child_index: 0,
        })
    }

    /// Get the fingerprint of this key (first 4 bytes of HASH160).
    pub fn fingerprint(&self) -> [u8; 4] {
        let hash = hash160(&self.public_key);
        let mut fp = [0u8; 4];
        fp.copy_from_slice(&hash[0..4]);
        fp
    }

    /// Derive a child public key at the given index.
    ///
    /// Only non-hardened derivation is supported (index < 2^31).
    /// For hardened derivation, you would need the private key.
    pub fn derive_child(&self, index: u32) -> Result<Self, MpcWalletError> {
        // Check for hardened derivation attempt
        if index >= 0x80000000 {
            return Err(MpcWalletError::Protocol(
                "Hardened derivation requires private key".to_string(),
            ));
        }

        tracing::debug!("Deriving child key at index {}", index);

        // Data = public_key || index (big-endian)
        let mut data = Vec::with_capacity(37);
        data.extend_from_slice(&self.public_key);
        data.extend_from_slice(&index.to_be_bytes());

        // I = HMAC-SHA512(chain_code, data)
        type HmacSha512 = Hmac<Sha512>;
        let mut mac = HmacSha512::new_from_slice(&self.chain_code)
            .map_err(|e| MpcWalletError::Protocol(format!("HMAC error: {}", e)))?;
        mac.update(&data);
        let result = mac.finalize().into_bytes();

        // Split: IL = result[0..32], IR = result[32..64]
        let il = &result[0..32];
        let ir = &result[32..64];

        // child_public_key = point(IL) + parent_public_key
        let child_pubkey = point_add_scalar(&self.public_key, il)?;

        let mut child_chain_code = [0u8; 32];
        child_chain_code.copy_from_slice(ir);

        tracing::trace!("  IL (tweak): {}", hex::encode(il));
        tracing::trace!("  Child pubkey: {}", hex::encode(child_pubkey));
        tracing::trace!("  Child chain code: {}", hex::encode(child_chain_code));

        Ok(Self {
            public_key: child_pubkey,
            chain_code: child_chain_code,
            depth: self.depth + 1,
            parent_fingerprint: self.fingerprint(),
            child_index: index,
        })
    }

    /// Derive a key at a path like "m/0/1/2" (non-hardened only).
    ///
    /// For BIP84 (native SegWit), use "84'/0'/0'/0/index" but note that
    /// the hardened parts (') require the private key, so we can only
    /// derive from a pre-derived public key at the account level.
    pub fn derive_path(&self, path: &[u32]) -> Result<Self, MpcWalletError> {
        let mut key = self.clone();
        for &index in path {
            key = key.derive_child(index)?;
        }
        Ok(key)
    }

    /// Get the Bitcoin address (P2WPKH) for this key.
    pub fn to_address(&self, network: Network) -> Result<String, MpcWalletError> {
        derive_bitcoin_address(&self.public_key, network)
    }

    /// Get the legacy Bitcoin address (P2PKH) for this key.
    pub fn to_legacy_address(&self, network: Network) -> Result<String, MpcWalletError> {
        derive_bitcoin_address_legacy(&self.public_key, network)
    }
}

// ============================================================================
// HD Wallet for MPC
// ============================================================================

/// A derived address with its metadata.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DerivedAddress {
    /// The Bitcoin address.
    pub address: String,
    /// The public key for this address (hex).
    pub public_key: String,
    /// The derivation path (e.g., "m/84'/0'/0'/0/0").
    pub path: String,
    /// Change index (0 = receiving, 1 = change).
    pub change: u32,
    /// Address index.
    pub index: u32,
}

/// BIP84 HD wallet structure for native SegWit addresses.
///
/// Path structure: m/84'/0'/0'/change/index
/// - 84' = BIP84 (native SegWit)
/// - 0' = Bitcoin mainnet (1' for testnet)
/// - 0' = Account 0
/// - change = 0 for receiving, 1 for change
/// - index = address index
///
/// Since we can only do non-hardened derivation from a public key,
/// we treat the MPC public key as the "account" level public key
/// and derive: account_pubkey/change/index
#[derive(Debug, Clone)]
pub struct MpcHdWallet {
    /// The account-level extended public key (from MPC DKG).
    account_key: ExtendedPubKey,
    /// Network (mainnet or testnet).
    network: Network,
}

impl MpcHdWallet {
    /// Create a new HD wallet from an MPC-generated public key.
    ///
    /// The public key is treated as the account-level key (m/84'/0'/0').
    pub fn new(public_key: &[u8], network: Network) -> Result<Self, MpcWalletError> {
        let account_key = ExtendedPubKey::from_public_key(public_key)?;

        tracing::info!("Created MPC HD Wallet");
        tracing::debug!("  Network: {:?}", network);
        tracing::debug!("  Account pubkey: {}", hex::encode(account_key.public_key));

        Ok(Self {
            account_key,
            network,
        })
    }

    /// Get a receiving address at the given index.
    ///
    /// Path: account/0/index
    pub fn get_receiving_address(&self, index: u32) -> Result<DerivedAddress, MpcWalletError> {
        self.derive_address(0, index)
    }

    /// Get a change address at the given index.
    ///
    /// Path: account/1/index
    pub fn get_change_address(&self, index: u32) -> Result<DerivedAddress, MpcWalletError> {
        self.derive_address(1, index)
    }

    /// Derive an address at the given change and index.
    fn derive_address(&self, change: u32, index: u32) -> Result<DerivedAddress, MpcWalletError> {
        tracing::debug!("Deriving address: change={}, index={}", change, index);

        let path = [change, index];
        let derived_key = self.account_key.derive_path(&path)?;
        let address = derived_key.to_address(self.network)?;

        // BIP-84 coin type: 0' for mainnet, 1' for testnet/signet/regtest
        let coin_type = match self.network {
            Network::Bitcoin => 0,
            _ => 1,
        };

        Ok(DerivedAddress {
            address,
            public_key: hex::encode(derived_key.public_key),
            path: format!("m/84'/{}'/0'/{}/{}", coin_type, change, index),
            change,
            index,
        })
    }

    /// Get multiple receiving addresses.
    pub fn get_receiving_addresses(
        &self,
        start: u32,
        count: u32,
    ) -> Result<Vec<DerivedAddress>, MpcWalletError> {
        let mut addresses = Vec::with_capacity(count as usize);
        for i in start..start + count {
            addresses.push(self.get_receiving_address(i)?);
        }
        Ok(addresses)
    }

    /// Get the account public key (for external verification).
    pub fn account_public_key(&self) -> String {
        hex::encode(self.account_key.public_key)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extended_pubkey_creation() {
        let pubkey_hex = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let pubkey_bytes = hex::decode(pubkey_hex).unwrap();

        let xpub = ExtendedPubKey::from_public_key(&pubkey_bytes).unwrap();
        assert_eq!(xpub.depth, 0);
        assert_eq!(xpub.child_index, 0);
    }

    #[test]
    fn test_child_derivation() {
        let pubkey_hex = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let pubkey_bytes = hex::decode(pubkey_hex).unwrap();

        let xpub = ExtendedPubKey::from_public_key(&pubkey_bytes).unwrap();

        // Derive child at index 0
        let child0 = xpub.derive_child(0).unwrap();
        assert_eq!(child0.depth, 1);
        assert_eq!(child0.child_index, 0);

        // Derive child at index 1
        let child1 = xpub.derive_child(1).unwrap();
        assert_eq!(child1.depth, 1);
        assert_eq!(child1.child_index, 1);

        // Children should have different public keys
        assert_ne!(child0.public_key, child1.public_key);
    }

    #[test]
    fn test_hardened_derivation_fails() {
        let pubkey_hex = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let pubkey_bytes = hex::decode(pubkey_hex).unwrap();

        let xpub = ExtendedPubKey::from_public_key(&pubkey_bytes).unwrap();

        // Hardened index should fail
        let result = xpub.derive_child(0x80000000);
        assert!(result.is_err());
    }

    #[test]
    fn test_hd_wallet_addresses() {
        let pubkey_hex = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let pubkey_bytes = hex::decode(pubkey_hex).unwrap();

        let wallet = MpcHdWallet::new(&pubkey_bytes, Network::Testnet).unwrap();

        // Get first 3 receiving addresses
        let addresses = wallet.get_receiving_addresses(0, 3).unwrap();

        assert_eq!(addresses.len(), 3);
        assert_eq!(addresses[0].index, 0);
        assert_eq!(addresses[1].index, 1);
        assert_eq!(addresses[2].index, 2);

        // All should be different
        assert_ne!(addresses[0].address, addresses[1].address);
        assert_ne!(addresses[1].address, addresses[2].address);

        // All should be testnet SegWit addresses with coin type 1'
        for addr in &addresses {
            assert!(addr.address.starts_with("tb1q"));
            assert!(addr.path.starts_with("m/84'/1'/0'/")); // testnet coin type
            assert!(addr.path.contains("/0/")); // receiving path
        }
    }

    #[test]
    fn test_hd_wallet_mainnet_path() {
        let pubkey_hex = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let pubkey_bytes = hex::decode(pubkey_hex).unwrap();

        let wallet = MpcHdWallet::new(&pubkey_bytes, Network::Bitcoin).unwrap();
        let addr = wallet.get_receiving_address(0).unwrap();

        // Mainnet should use coin type 0'
        assert!(addr.address.starts_with("bc1q"));
        assert!(addr.path.starts_with("m/84'/0'/0'/")); // mainnet coin type
    }

    #[test]
    fn test_change_addresses() {
        let pubkey_hex = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let pubkey_bytes = hex::decode(pubkey_hex).unwrap();

        let wallet = MpcHdWallet::new(&pubkey_bytes, Network::Testnet).unwrap();

        let receiving = wallet.get_receiving_address(0).unwrap();
        let change = wallet.get_change_address(0).unwrap();

        // Same index but different addresses (different change value)
        assert_ne!(receiving.address, change.address);
        assert!(receiving.path.contains("/0/0"));
        assert!(change.path.contains("/1/0"));
    }
}
