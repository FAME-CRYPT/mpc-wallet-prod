//! Mock services for integration tests.

use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;

/// Mock Bitcoin Esplora API server
pub struct BitcoinMockServer {
    url: String,
    responses: Arc<Mutex<HashMap<String, serde_json::Value>>>,
}

impl BitcoinMockServer {
    /// Create a new mock Bitcoin server
    pub async fn new() -> Self {
        Self {
            url: "http://localhost:3000".to_string(),
            responses: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get mock server URL
    pub fn url(&self) -> String {
        self.url.clone()
    }

    /// Setup mock response for address balance
    pub async fn mock_address_balance(&self, address: &str, balance: u64) {
        let mut responses = self.responses.lock().await;
        responses.insert(
            format!("/address/{}", address),
            json!({
                "address": address,
                "chain_stats": {
                    "funded_txo_sum": balance,
                    "spent_txo_sum": 0,
                },
                "mempool_stats": {
                    "funded_txo_sum": 0,
                    "spent_txo_sum": 0,
                }
            }),
        );
    }

    /// Setup mock response for UTXOs
    pub async fn mock_utxos(&self, address: &str, utxos: Vec<serde_json::Value>) {
        let mut responses = self.responses.lock().await;
        responses.insert(format!("/address/{}/utxo", address), json!(utxos));
    }

    /// Setup mock response for transaction broadcast
    pub async fn mock_broadcast_success(&self, txid: &str) {
        let mut responses = self.responses.lock().await;
        responses.insert("/tx".to_string(), json!({ "txid": txid }));
    }

    /// Setup mock response for transaction confirmation
    pub async fn mock_tx_status(&self, txid: &str, confirmed: bool, block_height: Option<u64>) {
        let mut responses = self.responses.lock().await;
        responses.insert(
            format!("/tx/{}/status", txid),
            json!({
                "confirmed": confirmed,
                "block_height": block_height,
                "block_hash": if confirmed {
                    Some("00000000000000000001")
                } else {
                    None
                },
                "block_time": if confirmed {
                    Some(1_700_000_000u64)
                } else {
                    None
                }
            }),
        );
    }

    /// Setup mock response for fee estimates
    pub async fn mock_fee_estimates(&self) {
        let mut responses = self.responses.lock().await;
        responses.insert(
            "/fee-estimates".to_string(),
            json!({
                "1": 20,
                "3": 15,
                "6": 10,
                "12": 5,
                "24": 3,
                "144": 1
            }),
        );
    }
}

/// Mock cryptographic operations for testing
pub struct MockCrypto;

impl MockCrypto {
    /// Generate a mock ECDSA signature (DER-encoded)
    pub fn mock_ecdsa_signature() -> Vec<u8> {
        // Mock DER-encoded ECDSA signature (70-72 bytes typical)
        vec![
            0x30, 0x44, 0x02, 0x20,
            // r value (32 bytes)
            0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
            0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00,
            0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
            0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00,
            0x02, 0x20,
            // s value (32 bytes)
            0xff, 0xee, 0xdd, 0xcc, 0xbb, 0xaa, 0x99, 0x88,
            0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, 0x00,
            0xff, 0xee, 0xdd, 0xcc, 0xbb, 0xaa, 0x99, 0x88,
            0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, 0x00,
            0x01, // SIGHASH_ALL
        ]
    }

    /// Generate a mock Schnorr signature (64 bytes)
    pub fn mock_schnorr_signature() -> Vec<u8> {
        vec![0xAB; 64]
    }

    /// Generate a mock compressed public key (33 bytes)
    pub fn mock_compressed_pubkey() -> Vec<u8> {
        let mut pubkey = vec![0x02]; // Compressed prefix
        pubkey.extend_from_slice(&[0xCD; 32]);
        pubkey
    }

    /// Generate a mock x-only public key for Taproot (32 bytes)
    pub fn mock_xonly_pubkey() -> Vec<u8> {
        vec![0xEF; 32]
    }
}

/// Mock QUIC connection for network testing
pub struct MockQuicConnection {
    peer_id: String,
    connected: Arc<Mutex<bool>>,
}

impl MockQuicConnection {
    /// Create a new mock QUIC connection
    pub fn new(peer_id: String) -> Self {
        Self {
            peer_id,
            connected: Arc::new(Mutex::new(true)),
        }
    }

    /// Check if connection is active
    pub async fn is_connected(&self) -> bool {
        *self.connected.lock().await
    }

    /// Simulate connection close
    pub async fn close(&self) {
        let mut connected = self.connected.lock().await;
        *connected = false;
    }

    /// Get peer ID
    pub fn peer_id(&self) -> &str {
        &self.peer_id
    }
}

/// Mock presignature pool
pub struct MockPresignaturePool {
    available: Arc<Mutex<usize>>,
}

impl MockPresignaturePool {
    /// Create a new mock presignature pool
    pub fn new(initial_count: usize) -> Self {
        Self {
            available: Arc::new(Mutex::new(initial_count)),
        }
    }

    /// Get available presignature count
    pub async fn available(&self) -> usize {
        *self.available.lock().await
    }

    /// Consume a presignature
    pub async fn consume(&self) -> Result<(), String> {
        let mut available = self.available.lock().await;
        if *available > 0 {
            *available -= 1;
            Ok(())
        } else {
            Err("No presignatures available".to_string())
        }
    }

    /// Add presignatures to pool
    pub async fn add(&self, count: usize) {
        let mut available = self.available.lock().await;
        *available += count;
    }
}
