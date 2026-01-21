use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use threshold_types::{TransactionState, TxId};

/// API client for interacting with MPC wallet nodes
#[derive(Clone)]
pub struct ApiClient {
    base_url: String,
    client: Client,
}

impl ApiClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            base_url: base_url.into(),
            client,
        }
    }

    pub async fn health_check(&self) -> Result<HealthResponse> {
        let url = format!("{}/health", self.base_url);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send health check request")?;

        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("Health check failed with status: {}", status);
        }

        response
            .json()
            .await
            .context("Failed to parse health check response")
    }

    pub async fn send_transaction(&self, request: &SendTransactionRequest) -> Result<SendTransactionResponse> {
        let url = format!("{}/api/v1/transactions", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(request)
            .send()
            .await
            .context("Failed to send transaction request")?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Transaction request failed with status {}: {}", status, error_text);
        }

        response
            .json()
            .await
            .context("Failed to parse transaction response")
    }

    pub async fn get_transaction(&self, txid: &str) -> Result<TransactionResponse> {
        let url = format!("{}/api/v1/transactions/{}", self.base_url, txid);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to get transaction")?;

        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("Get transaction failed with status: {}", status);
        }

        response
            .json()
            .await
            .context("Failed to parse transaction response")
    }

    pub async fn list_transactions(&self) -> Result<ListTransactionsResponse> {
        let url = format!("{}/api/v1/transactions", self.base_url);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to list transactions")?;

        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("List transactions failed with status: {}", status);
        }

        response
            .json()
            .await
            .context("Failed to parse transactions list")
    }

    pub async fn get_wallet_balance(&self) -> Result<WalletBalanceResponse> {
        let url = format!("{}/api/v1/wallet/balance", self.base_url);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to get wallet balance")?;

        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("Get wallet balance failed with status: {}", status);
        }

        response
            .json()
            .await
            .context("Failed to parse wallet balance response")
    }

    pub async fn get_metrics(&self) -> Result<String> {
        let url = format!("{}/metrics", self.base_url);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to get metrics")?;

        response
            .text()
            .await
            .context("Failed to read metrics text")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendTransactionRequest {
    pub recipient: String,
    pub amount_sats: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendTransactionResponse {
    pub txid: String,
    pub state: TransactionState,
    pub recipient: String,
    pub amount_sats: u64,
    pub fee_sats: u64,
    pub metadata: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionResponse {
    pub txid: String,
    pub state: TransactionState,
    pub recipient: String,
    pub amount_sats: u64,
    pub fee_sats: u64,
    pub metadata: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTransactionsResponse {
    pub transactions: Vec<TransactionResponse>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletBalanceResponse {
    pub confirmed: u64,
    pub unconfirmed: u64,
    pub total: u64,
}
