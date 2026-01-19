//! Bitcoin-related commands (balance, faucet, send).

use anyhow::{Context, Result};
use common::{BalanceResponse, SendBitcoinRequest, SendBitcoinResponse};

/// Get wallet balance.
pub async fn get_balance(coordinator_url: &str, wallet_id: &str) -> Result<()> {
    println!("Checking balance...");

    let client = reqwest::Client::new();
    let url = format!("{}/wallet/{}/balance", coordinator_url, wallet_id);

    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        anyhow::bail!("Error: {}", error_text);
    }

    let balance: BalanceResponse = response.json().await?;

    println!();
    println!("Wallet Balance");
    println!("==============");
    println!("  Address:      {}", balance.address);
    println!(
        "  Confirmed:    {} sats ({} BTC)",
        balance.confirmed_sats, balance.confirmed_btc
    );
    if balance.unconfirmed_sats != 0 {
        println!("  Unconfirmed:  {} sats", balance.unconfirmed_sats);
    }
    println!(
        "  Total:        {} sats ({} BTC)",
        balance.total_sats, balance.total_btc
    );

    if balance.total_sats == 0 {
        println!();
        println!("No balance! Get testnet coins:");
        println!("  mpc-wallet faucet --wallet-id {}", wallet_id);
    }

    Ok(())
}

/// Get faucet information.
pub async fn get_faucet(coordinator_url: &str, wallet_id: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/wallet/{}/faucet", coordinator_url, wallet_id);

    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        anyhow::bail!("Error: {}", error_text);
    }

    let data: serde_json::Value = response.json().await?;

    println!("===================================================================");
    println!("                    TESTNET BITCOIN FAUCET");
    println!("===================================================================");
    println!();
    println!("Your wallet address:");
    println!();
    println!("  {}", data["address"].as_str().unwrap_or("-"));
    println!();
    println!("Copy this address and paste it into one of these faucets:");
    println!();
    if let Some(faucets) = data["faucets"].as_array() {
        for (i, faucet) in faucets.iter().enumerate() {
            println!("  {}. {}", i + 1, faucet.as_str().unwrap_or("-"));
        }
    }
    println!();
    println!("View on block explorer:");
    println!("  {}", data["explorer_url"].as_str().unwrap_or("-"));
    println!();
    println!("===================================================================");
    println!("NOTE: Testnet coins have no real value - they're for testing!");
    println!("===================================================================");

    Ok(())
}

/// Mine blocks to a wallet address (regtest only).
pub async fn mine_blocks(coordinator_url: &str, wallet_id: &str, blocks: u32) -> Result<()> {
    println!("Mining {} blocks to wallet {}...", blocks, wallet_id);
    println!();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    let url = format!("{}/wallet/{}/mine", coordinator_url, wallet_id);

    let response = client
        .post(&url)
        .json(&serde_json::json!({ "blocks": blocks }))
        .send()
        .await
        .context("Failed to connect to coordinator")?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        anyhow::bail!("Mining failed: {}", error_text);
    }

    let result: serde_json::Value = response.json().await?;

    println!("===================================================================");
    println!("                    BLOCKS MINED");
    println!("===================================================================");
    println!();
    println!(
        "  Blocks mined:  {}",
        result["blocks_mined"].as_u64().unwrap_or(0)
    );
    println!(
        "  To address:    {}",
        result["address"].as_str().unwrap_or("-")
    );
    println!();
    println!("{}", result["message"].as_str().unwrap_or(""));
    println!();
    println!("===================================================================");

    Ok(())
}

/// Send Bitcoin (legacy endpoint).
pub async fn send_bitcoin(
    coordinator_url: &str,
    wallet_id: &str,
    to: &str,
    amount: u64,
    fee_rate: Option<u64>,
) -> Result<()> {
    println!("Preparing transaction...");
    println!("  From wallet: {}", wallet_id);
    println!("  To:          {}", to);
    println!(
        "  Amount:      {} sats ({:.8} BTC)",
        amount,
        amount as f64 / 100_000_000.0
    );
    if let Some(rate) = fee_rate {
        println!("  Fee rate:    {} sat/vB", rate);
    }
    println!();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;

    let url = format!("{}/wallet/{}/send", coordinator_url, wallet_id);
    let request = SendBitcoinRequest {
        wallet_id: wallet_id.to_string(),
        to_address: to.to_string(),
        amount_sats: amount,
        fee_rate,
    };

    println!("Collecting signatures from MPC nodes...");

    let response = client
        .post(&url)
        .json(&request)
        .send()
        .await
        .context("Failed to connect to coordinator")?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        anyhow::bail!("Transaction failed: {}", error_text);
    }

    let result: SendBitcoinResponse = response.json().await?;

    println!();
    println!("===================================================================");
    println!("                  TRANSACTION BROADCAST!");
    println!("===================================================================");
    println!();
    println!("  TXID:    {}", result.txid);
    println!("  Amount:  {} sats", result.amount_sats);
    println!("  Fee:     {} sats", result.fee_sats);
    println!("  To:      {}", result.to_address);
    println!();
    println!("{}", result.message);
    println!();
    println!("===================================================================");

    Ok(())
}
