//! Transaction management endpoints

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use threshold_types::{TransactionState, TxId};

use crate::{error::ApiError, state::AppState, ApiResult};

/// Request to create a new transaction
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTransactionRequest {
    /// Recipient Bitcoin address
    pub recipient: String,
    /// Amount in satoshis
    pub amount_sats: u64,
    /// Optional OP_RETURN metadata (max 80 bytes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
}

/// Response after creating a transaction
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

/// Transaction status response
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

/// List of transactions response
#[derive(Debug, Serialize, Deserialize)]
pub struct ListTransactionsResponse {
    pub transactions: Vec<TransactionStatusResponse>,
    pub total: usize,
}

/// POST /api/v1/transactions - Create a new transaction
///
/// Creates a new Bitcoin transaction with optional OP_RETURN metadata.
/// The transaction will go through the MPC threshold signing process.
pub async fn create_transaction(
    State(state): State<AppState>,
    Json(payload): Json<CreateTransactionRequest>,
) -> ApiResult<Json<CreateTransactionResponse>> {
    // Validate recipient address format
    if payload.recipient.is_empty() {
        return Err(ApiError::BadRequest(
            "Recipient address is required".to_string(),
        ));
    }

    // Validate amount
    if payload.amount_sats == 0 {
        return Err(ApiError::BadRequest(
            "Amount must be greater than zero".to_string(),
        ));
    }

    // Validate metadata size (OP_RETURN max is 80 bytes)
    if let Some(ref metadata) = payload.metadata {
        if metadata.len() > 80 {
            return Err(ApiError::BadRequest(
                "Metadata exceeds maximum size of 80 bytes".to_string(),
            ));
        }
    }

    // Use handler to create transaction
    let tx = crate::handlers::transactions::create_transaction(
        state.postgres.as_ref(),
        state.bitcoin.as_ref(),
        &payload.recipient,
        payload.amount_sats,
        payload.metadata.as_deref(),
    )
    .await?;

    Ok(Json(CreateTransactionResponse {
        txid: tx.txid.0.clone(),
        state: tx.state,
        recipient: tx.recipient,
        amount_sats: tx.amount_sats,
        fee_sats: tx.fee_sats,
        metadata: tx.metadata,
        created_at: tx.created_at,
    }))
}

/// GET /api/v1/transactions/:txid - Get transaction status
///
/// Retrieves the current status of a specific transaction
pub async fn get_transaction(
    State(state): State<AppState>,
    Path(txid): Path<String>,
) -> ApiResult<Json<TransactionStatusResponse>> {
    let txid = TxId::from(txid);

    // Fetch transaction from database
    let tx = state
        .postgres
        .get_transaction(&txid)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Transaction not found: {}", txid)))?;

    Ok(Json(TransactionStatusResponse {
        txid: tx.txid.0,
        state: tx.state,
        recipient: tx.recipient,
        amount_sats: tx.amount_sats,
        fee_sats: tx.fee_sats,
        metadata: tx.metadata,
        created_at: tx.created_at,
        updated_at: tx.updated_at,
    }))
}

/// GET /api/v1/transactions - List all transactions
///
/// Returns a list of all transactions with their current status
pub async fn list_transactions(
    State(state): State<AppState>,
) -> ApiResult<Json<ListTransactionsResponse>> {
    // Fetch all transactions from database
    let transactions = crate::handlers::transactions::list_transactions(state.postgres.as_ref())
        .await?;

    let total = transactions.len();
    let transaction_responses: Vec<TransactionStatusResponse> = transactions
        .into_iter()
        .map(|tx| TransactionStatusResponse {
            txid: tx.txid.0,
            state: tx.state,
            recipient: tx.recipient,
            amount_sats: tx.amount_sats,
            fee_sats: tx.fee_sats,
            metadata: tx.metadata,
            created_at: tx.created_at,
            updated_at: tx.updated_at,
        })
        .collect();

    Ok(Json(ListTransactionsResponse {
        transactions: transaction_responses,
        total,
    }))
}
