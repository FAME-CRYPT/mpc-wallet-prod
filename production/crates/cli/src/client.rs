//! REST API client for communicating with the threshold wallet API server.

use anyhow::{Context, Result};
use reqwest::{Client as HttpClient, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use threshold_types::TransactionState;

/// API client for threshold wallet operations
#[derive(Clone)]
pub struct ApiClient {
    base_url: String,
    client: HttpClient,
}

impl ApiClient {
    /// Create a new API client
    pub fn new(base_url: String, timeout_secs: u64) -> Result<Self> {
        let client = HttpClient::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { base_url, client })
    }

    /// Check API server health
    pub async fn health_check(&self) -> Result<HealthResponse> {
        let url = format!("{}/health", self.base_url);
        let response = self.client.get(&url).send().await?;

        if response.status().is_success() {
            let health = response.json::<HealthResponse>().await?;
            Ok(health)
        } else {
            anyhow::bail!("Health check failed: {}", response.status());
        }
    }

    /// Get wallet balance
    pub async fn get_wallet_balance(&self) -> Result<WalletBalanceResponse> {
        let url = format!("{}/api/v1/wallet/balance", self.base_url);
        let response = self.client.get(&url).send().await?;

        self.handle_response(response).await
    }

    /// Get wallet address
    pub async fn get_wallet_address(&self) -> Result<WalletAddressResponse> {
        let url = format!("{}/api/v1/wallet/address", self.base_url);
        let response = self.client.get(&url).send().await?;

        self.handle_response(response).await
    }

    /// Create a new transaction
    pub async fn create_transaction(
        &self,
        recipient: String,
        amount_sats: u64,
        metadata: Option<String>,
    ) -> Result<CreateTransactionResponse> {
        let url = format!("{}/api/v1/transactions", self.base_url);

        let request = CreateTransactionRequest {
            recipient,
            amount_sats,
            metadata,
        };

        let response = self.client.post(&url).json(&request).send().await?;

        self.handle_response(response).await
    }

    /// Get transaction status
    pub async fn get_transaction(&self, txid: &str) -> Result<TransactionStatusResponse> {
        let url = format!("{}/api/v1/transactions/{}", self.base_url, txid);
        let response = self.client.get(&url).send().await?;

        self.handle_response(response).await
    }

    /// List all transactions
    pub async fn list_transactions(&self) -> Result<ListTransactionsResponse> {
        let url = format!("{}/api/v1/transactions", self.base_url);
        let response = self.client.get(&url).send().await?;

        self.handle_response(response).await
    }

    /// Get cluster status
    pub async fn get_cluster_status(&self) -> Result<ClusterStatusResponse> {
        let url = format!("{}/api/v1/cluster/status", self.base_url);
        let response = self.client.get(&url).send().await?;

        self.handle_response(response).await
    }

    /// List cluster nodes
    pub async fn list_cluster_nodes(&self) -> Result<ListNodesResponse> {
        let url = format!("{}/api/v1/cluster/nodes", self.base_url);
        let response = self.client.get(&url).send().await?;

        self.handle_response(response).await
    }

    /// Start DKG ceremony
    pub async fn start_dkg(
        &self,
        protocol: String,
        threshold: u32,
        total: u32,
        participants: Vec<u64>,
    ) -> Result<DkgResponse> {
        let url = format!("{}/api/v1/dkg/generate", self.base_url);

        let request = DkgRequest {
            protocol,
            threshold,
            total,
            participants,
        };

        let response = self.client.post(&url).json(&request).send().await?;

        self.handle_response(response).await
    }

    /// Get DKG status
    pub async fn get_dkg_status(&self) -> Result<DkgStatusResponse> {
        let url = format!("{}/api/v1/dkg/status", self.base_url);
        let response = self.client.get(&url).send().await?;

        self.handle_response(response).await
    }

    /// Start aux_info generation
    pub async fn start_aux_info(
        &self,
        num_parties: u16,
        participants: Vec<u64>,
    ) -> Result<AuxInfoResponse> {
        let url = format!("{}/api/v1/aux-info/generate", self.base_url);

        let request = AuxInfoRequest {
            num_parties,
            participants,
        };

        let response = self.client.post(&url).json(&request).send().await?;

        self.handle_response(response).await
    }

    /// Get aux_info status
    pub async fn get_aux_info_status(&self) -> Result<AuxInfoStatusResponse> {
        let url = format!("{}/api/v1/aux-info/status", self.base_url);
        let response = self.client.get(&url).send().await?;

        self.handle_response(response).await
    }

    /// Handle API response and parse errors
    async fn handle_response<T: for<'de> Deserialize<'de>>(
        &self,
        response: reqwest::Response,
    ) -> Result<T> {
        let status = response.status();

        if status.is_success() {
            let data = response.json::<T>().await
                .context("Failed to parse response")?;
            Ok(data)
        } else {
            // Try to parse error response
            let error_text = response.text().await
                .unwrap_or_else(|_| "Unknown error".to_string());

            match status {
                StatusCode::NOT_FOUND => {
                    anyhow::bail!("Resource not found: {}", error_text)
                }
                StatusCode::BAD_REQUEST => {
                    anyhow::bail!("Bad request: {}", error_text)
                }
                StatusCode::INTERNAL_SERVER_ERROR => {
                    anyhow::bail!("Server error: {}", error_text)
                }
                _ => {
                    anyhow::bail!("Request failed ({}): {}", status, error_text)
                }
            }
        }
    }
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WalletBalanceResponse {
    pub confirmed: u64,
    pub unconfirmed: u64,
    pub total: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WalletAddressResponse {
    pub address: String,
    pub address_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTransactionRequest {
    pub recipient: String,
    pub amount_sats: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTransactionResponse {
    pub txid: String,
    pub state: TransactionState,
    pub recipient: String,
    pub amount_sats: u64,
    pub fee_sats: u64,
    pub metadata: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TransactionStatusResponse {
    pub txid: String,
    pub state: TransactionState,
    pub recipient: String,
    pub amount_sats: u64,
    pub fee_sats: u64,
    pub metadata: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListTransactionsResponse {
    pub transactions: Vec<TransactionStatusResponse>,
    pub total: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClusterStatusResponse {
    pub total_nodes: u32,
    pub healthy_nodes: u32,
    pub threshold: u32,
    pub status: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NodeInfo {
    pub node_id: u64,
    pub status: String,
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
    pub total_votes: i64,
    pub total_violations: i64,
    pub seconds_since_heartbeat: f64,
    pub is_banned: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListNodesResponse {
    pub nodes: Vec<NodeInfo>,
    pub total: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DkgRequest {
    pub protocol: String,
    pub threshold: u32,
    pub total: u32,
    pub participants: Vec<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DkgResponse {
    pub success: bool,
    pub session_id: String,
    pub party_index: u16,
    pub threshold: u32,
    pub total: u32,
    pub key_share_size_bytes: usize,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DkgStatusResponse {
    pub has_key_share: bool,
    pub latest_session_id: Option<String>,
    pub key_share_size_bytes: usize,
    pub total_ceremonies: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuxInfoRequest {
    pub num_parties: u16,
    pub participants: Vec<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuxInfoResponse {
    pub success: bool,
    pub session_id: String,
    pub party_index: u16,
    pub num_parties: u16,
    pub aux_info_size_bytes: usize,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuxInfoStatusResponse {
    pub has_aux_info: bool,
    pub latest_session_id: Option<String>,
    pub aux_info_size_bytes: usize,
    pub total_ceremonies: usize,
}
