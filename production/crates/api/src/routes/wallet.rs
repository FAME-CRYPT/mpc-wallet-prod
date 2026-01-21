//! Wallet management endpoints

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::{state::AppState, ApiResult};

/// Wallet balance response
#[derive(Debug, Serialize, Deserialize)]
pub struct WalletBalanceResponse {
    /// Total confirmed balance in satoshis
    pub confirmed: u64,
    /// Total unconfirmed balance in satoshis
    pub unconfirmed: u64,
    /// Total balance (confirmed + unconfirmed)
    pub total: u64,
}

/// Wallet address response
#[derive(Debug, Serialize, Deserialize)]
pub struct WalletAddressResponse {
    /// Bitcoin address for receiving funds
    pub address: String,
    /// Address type (e.g., "p2wpkh", "p2tr")
    pub address_type: String,
}

/// GET /api/v1/wallet/balance - Get wallet balance
///
/// Returns the current balance of the MPC wallet including
/// confirmed and unconfirmed amounts
pub async fn get_balance(
    State(state): State<AppState>,
) -> ApiResult<Json<WalletBalanceResponse>> {
    // Fetch balance from Bitcoin client
    let balance = crate::handlers::wallet::get_wallet_balance(state.bitcoin.as_ref()).await?;

    Ok(Json(WalletBalanceResponse {
        confirmed: balance.confirmed,
        unconfirmed: balance.unconfirmed,
        total: balance.total,
    }))
}

/// GET /api/v1/wallet/address - Get receiving address
///
/// Returns the current receiving address for the MPC wallet
pub async fn get_address(
    State(state): State<AppState>,
) -> ApiResult<Json<WalletAddressResponse>> {
    // Fetch address from handler
    let address_info = crate::handlers::wallet::get_wallet_address(state.bitcoin.as_ref()).await?;

    Ok(Json(WalletAddressResponse {
        address: address_info.address,
        address_type: address_info.address_type,
    }))
}
