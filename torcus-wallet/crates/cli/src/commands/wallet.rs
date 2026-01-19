//! Wallet management commands.

use anyhow::Result;
use common::DerivedAddress;

/// List all wallets.
pub async fn list_wallets(coordinator_url: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/wallets", coordinator_url);

    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        anyhow::bail!("Error: {}", error_text);
    }

    let data: serde_json::Value = response.json().await?;

    println!("MPC Wallets");
    println!("===========");

    if let Some(wallets) = data["wallets"].as_array() {
        if wallets.is_empty() {
            println!("  No wallets found.");
            println!();
            println!("Create your first wallet with:");
            println!("  mpc-wallet cggmp24-create --name \"My Wallet\"  (for SegWit/ECDSA)");
            println!("  mpc-wallet taproot-create --name \"My Wallet\"  (for Taproot/Schnorr)");
        } else {
            for wallet in wallets {
                println!();
                println!(
                    "  ID:         {}",
                    wallet["wallet_id"].as_str().unwrap_or("-")
                );
                println!("  Name:       {}", wallet["name"].as_str().unwrap_or("-"));
                println!(
                    "  Type:       {}",
                    wallet["wallet_type"].as_str().unwrap_or("-")
                );
                println!(
                    "  Address:    {}",
                    wallet["address"].as_str().unwrap_or("-")
                );
                if let Some(created) = wallet["created_at"].as_str() {
                    println!("  Created:    {}", created);
                }
            }
            println!();
            println!("Total: {} wallet(s)", wallets.len());
        }
    }

    Ok(())
}

/// Get a specific wallet.
pub async fn get_wallet(coordinator_url: &str, wallet_id: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/wallet/{}", coordinator_url, wallet_id);

    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        anyhow::bail!("Error: {}", error_text);
    }

    let wallet: serde_json::Value = response.json().await?;

    println!("Wallet Details");
    println!("==============");
    println!(
        "  ID:         {}",
        wallet["wallet_id"].as_str().unwrap_or("-")
    );
    println!("  Name:       {}", wallet["name"].as_str().unwrap_or("-"));
    println!(
        "  Type:       {}",
        wallet["wallet_type"].as_str().unwrap_or("-")
    );
    println!(
        "  Public Key: {}",
        wallet["public_key"].as_str().unwrap_or("-")
    );
    println!(
        "  Address:    {}",
        wallet["address"].as_str().unwrap_or("-")
    );
    if let Some(created) = wallet["created_at"].as_str() {
        println!("  Created:    {}", created);
    }

    Ok(())
}

/// Delete a wallet.
pub async fn delete_wallet(coordinator_url: &str, wallet_id: &str, force: bool) -> Result<()> {
    if !force {
        println!(
            "WARNING: This will delete wallet {} and cannot be undone.",
            wallet_id
        );
        println!("Note: Key shares on MPC nodes will remain (manual cleanup required).");
        println!();
        print!("Are you sure? Type 'yes' to confirm: ");
        use std::io::{self, Write};
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if input.trim().to_lowercase() != "yes" {
            println!("Cancelled.");
            return Ok(());
        }
    }

    let client = reqwest::Client::new();
    let url = format!("{}/wallet/{}", coordinator_url, wallet_id);

    let response = client.delete(&url).send().await?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        anyhow::bail!("Error: {}", error_text);
    }

    println!("Wallet {} deleted successfully.", wallet_id);

    Ok(())
}

/// Derive HD addresses for a wallet.
pub async fn derive_addresses(
    coordinator_url: &str,
    wallet_id: &str,
    start: u32,
    count: u32,
    address_type: &str,
) -> Result<()> {
    println!("Deriving HD addresses...");
    println!("  Wallet:   {}", wallet_id);
    println!("  Start:    {}", start);
    println!("  Count:    {}", count);
    println!("  Type:     {}", address_type);
    println!();

    let client = reqwest::Client::new();
    let url = format!(
        "{}/wallet/{}/derive?start={}&count={}&address_type={}",
        coordinator_url, wallet_id, start, count, address_type
    );

    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        anyhow::bail!("Error: {}", error_text);
    }

    let data: serde_json::Value = response.json().await?;

    println!("Derived Addresses (BIP84 Native SegWit)");
    println!("=======================================");

    if let Some(addresses) = data["addresses"].as_array() {
        for addr in addresses {
            let path = addr["path"].as_str().unwrap_or("-");
            let address = addr["address"].as_str().unwrap_or("-");
            println!("  {} -> {}", path, address);
        }
        println!();
        println!("Total: {} address(es)", addresses.len());
    }

    Ok(())
}

/// Get a specific address.
pub async fn get_address(coordinator_url: &str, wallet_id: &str, index: u32) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/wallet/{}/address/{}", coordinator_url, wallet_id, index);

    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        anyhow::bail!("Error: {}", error_text);
    }

    let addr: DerivedAddress = response.json().await?;

    println!("Address at index {}", index);
    println!("==================");
    println!("  Path:       {}", addr.path);
    println!("  Address:    {}", addr.address);
    println!("  Public Key: {}", addr.public_key);

    Ok(())
}

/// Get system information from the coordinator.
pub async fn get_info(coordinator_url: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/info", coordinator_url);

    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        anyhow::bail!("Error: {}", error_text);
    }

    let info: serde_json::Value = response.json().await?;

    println!("MPC Wallet System Info");
    println!("======================");
    println!(
        "  Threshold:     {}-of-{}",
        info["threshold"], info["parties"]
    );
    println!("  Wallets:       {}", info["wallets_count"]);
    println!("  Nodes:");
    if let Some(nodes) = info["nodes"].as_array() {
        for (i, node) in nodes.iter().enumerate() {
            println!("    {}. {}", i + 1, node.as_str().unwrap_or("unknown"));
        }
    }

    Ok(())
}
