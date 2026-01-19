//! Taproot/FROST threshold Schnorr commands.

use anyhow::{Context, Result};
use common::{CreateWalletResponse, NUM_PARTIES};

/// Create a new Taproot wallet using FROST threshold signatures.
pub async fn taproot_create(coordinator_url: &str, name: &str, threshold: u16) -> Result<()> {
    println!("===================================================================");
    println!("              TAPROOT WALLET CREATION (FROST)");
    println!("===================================================================");
    println!();
    println!("  Wallet Name: {}", name);
    println!("  Threshold:   {}-of-{}", threshold, NUM_PARTIES);
    println!("  Signature:   Schnorr (BIP-340)");
    println!("  Address:     Taproot (P2TR, starts with tb1p)");
    println!();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?;

    println!("Step 1: Creating wallet record...");

    let create_url = format!("{}/wallet", coordinator_url);
    let create_request = serde_json::json!({
        "name": name,
        "wallet_type": "taproot"
    });

    let response = client
        .post(&create_url)
        .json(&create_request)
        .send()
        .await
        .context("Failed to create wallet")?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        anyhow::bail!("Failed to create wallet: {}", error_text);
    }

    let wallet: CreateWalletResponse = response.json().await?;
    let wallet_id = wallet.wallet_id.to_string();

    println!("  Wallet created: {}", wallet_id);
    println!();

    println!("Step 2: Running FROST distributed key generation...");
    println!("        (This uses Schnorr threshold signatures for Taproot)");
    println!();

    let keygen_url = format!("{}/frost/keygen/start", coordinator_url);
    let keygen_request = serde_json::json!({
        "wallet_id": wallet_id,
        "threshold": threshold
    });

    let response = client
        .post(&keygen_url)
        .json(&keygen_request)
        .send()
        .await
        .context("Failed to start FROST keygen")?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        // Clean up the wallet record since keygen failed
        eprintln!("  Keygen failed, cleaning up wallet record...");
        let delete_url = format!("{}/wallet/{}", coordinator_url, wallet_id);
        let _ = client.delete(&delete_url).send().await;
        anyhow::bail!("Failed to start FROST keygen: {}", error_text);
    }

    let keygen_result: serde_json::Value = response.json().await?;
    println!(
        "  Keygen session started: {}",
        keygen_result["session_id"].as_str().unwrap_or("-")
    );

    println!();
    println!("Waiting for key generation to complete...");

    let keyshares_url = format!("{}/frost/keyshares", coordinator_url);

    let mut keygen_complete = false;
    let mut frost_pubkey: Option<String> = None;

    for i in 0..60 {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        if let Ok(resp) = client.get(&keyshares_url).send().await {
            if let Ok(shares) = resp.json::<serde_json::Value>().await {
                if let Some(keyshares) = shares["keyshares"].as_array() {
                    for ks in keyshares {
                        if ks["wallet_id"].as_str() == Some(wallet_id.as_str()) {
                            // Keygen is complete when all parties have their shares
                            let nodes_count = ks["nodes_with_shares"]
                                .as_array()
                                .map(|a| a.len())
                                .unwrap_or(0);
                            let num_parties = ks["num_parties"].as_u64().unwrap_or(0) as usize;
                            if num_parties > 0 && nodes_count >= num_parties {
                                keygen_complete = true;
                                frost_pubkey = ks["public_key"].as_str().map(|s| s.to_string());
                            }
                            break;
                        }
                    }
                }
            }
        }

        if keygen_complete {
            break;
        }

        if i % 4 == 0 {
            println!("  Still generating keys... ({} seconds)", (i + 1) * 5);
        }
    }

    if keygen_complete {
        println!("  Key shares generated on all nodes");
        println!();

        if let Some(pubkey) = frost_pubkey {
            println!("Step 3: Updating wallet record with Taproot address...");
            let update_url = format!("{}/wallet/{}/taproot-pubkey", coordinator_url, wallet_id);
            let update_request = serde_json::json!({
                "public_key": pubkey
            });

            let update_response = client.put(&update_url).json(&update_request).send().await;

            match update_response {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(result) = resp.json::<serde_json::Value>().await {
                        let address = result["address"].as_str().unwrap_or("(see wallet details)");
                        println!("  Wallet record updated");
                        println!();
                        println!(
                            "==================================================================="
                        );
                        println!("         TAPROOT WALLET CREATED SUCCESSFULLY!");
                        println!(
                            "==================================================================="
                        );
                        println!();
                        println!("  Wallet ID:    {}", wallet_id);
                        println!("  Name:         {}", name);
                        println!("  Address:      {}", address);
                        println!("  Threshold:    {}-of-{}", threshold, NUM_PARTIES);
                        println!("  Public Key:   {}", pubkey);
                        println!("  Type:         Taproot (P2TR)");
                        println!();
                        println!("Next steps:");
                        println!("  1. Get testnet coins: Send tBTC to {}", address);
                        println!(
                            "  2. Check balance:     mpc-wallet balance -w {}",
                            wallet_id
                        );
                        println!(
                            "  3. Send Bitcoin:      mpc-wallet taproot-send -w {} -t <address> -a <sats>",
                            wallet_id
                        );
                        println!();
                        println!(
                            "==================================================================="
                        );
                    } else {
                        println!("  Wallet updated but could not parse response");
                    }
                }
                Ok(resp) => {
                    let error_text = resp.text().await.unwrap_or_default();
                    println!("  Failed to update wallet record: {}", error_text);
                }
                Err(e) => {
                    println!("  Failed to update wallet record: {}", e);
                }
            }
        } else {
            println!("  Could not retrieve FROST public key from nodes");
        }
    } else {
        println!();
        println!("Keygen may still be running. Check status with:");
        println!("  mpc-wallet taproot-keys");
    }

    Ok(())
}

/// Send Bitcoin using Taproot/FROST threshold signatures.
pub async fn taproot_send(
    coordinator_url: &str,
    wallet_id: &str,
    to: &str,
    amount: u64,
    fee_rate: Option<u64>,
) -> Result<()> {
    println!("===================================================================");
    println!("           TAPROOT THRESHOLD BITCOIN TRANSACTION");
    println!("===================================================================");
    println!();
    println!("  From Wallet: {}", wallet_id);
    println!("  To Address:  {}", to);
    println!(
        "  Amount:      {} sats ({:.8} BTC)",
        amount,
        amount as f64 / 100_000_000.0
    );
    if let Some(rate) = fee_rate {
        println!("  Fee Rate:    {} sat/vB", rate);
    }
    println!();
    println!("This transaction will be signed using FROST threshold Schnorr");
    println!("signatures - the private key is NEVER reconstructed!");
    println!();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()?;

    println!("Building transaction and collecting FROST signatures...");
    println!();

    let send_url = format!("{}/wallet/{}/send-taproot", coordinator_url, wallet_id);
    let mut request_body = serde_json::json!({
        "wallet_id": wallet_id,
        "to_address": to,
        "amount_sats": amount,
    });

    if let Some(rate) = fee_rate {
        request_body["fee_rate"] = serde_json::json!(rate);
    }

    let response = client
        .post(&send_url)
        .json(&request_body)
        .send()
        .await
        .context("Failed to connect to coordinator")?;

    let status = response.status();
    let response_text = response.text().await.unwrap_or_default();

    if !status.is_success() {
        anyhow::bail!("Transaction failed: {}", response_text);
    }

    let result: serde_json::Value =
        serde_json::from_str(&response_text).context("Failed to parse response")?;

    let txid = result["txid"].as_str().unwrap_or("-");

    println!("===================================================================");
    println!("         TAPROOT TRANSACTION BROADCAST SUCCESS!");
    println!("===================================================================");
    println!();
    println!("  TXID:   {}", txid);
    println!(
        "  Amount: {} sats",
        result["amount_sats"].as_u64().unwrap_or(0)
    );
    println!(
        "  Fee:    {} sats",
        result["fee_sats"].as_u64().unwrap_or(0)
    );
    println!("  To:     {}", result["to_address"].as_str().unwrap_or("-"));
    println!();
    println!("  Signed with FROST threshold Schnorr (3-of-4)");
    println!();
    if let Some(msg) = result["message"].as_str() {
        if msg.contains("http") {
            // Extract URL from message
            if let Some(url_start) = msg.find("http") {
                println!("  View on explorer: {}", &msg[url_start..]);
            }
        }
    }
    println!();
    println!("===================================================================");

    Ok(())
}

/// List Taproot/FROST key shares on all nodes.
pub async fn taproot_keys(coordinator_url: &str) -> Result<()> {
    println!("===================================================================");
    println!("                   FROST KEY SHARES (Taproot)");
    println!("===================================================================");
    println!();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    // Get aggregated key shares from coordinator
    let keyshares_url = format!("{}/frost/keyshares", coordinator_url);
    let resp = client
        .get(&keyshares_url)
        .send()
        .await
        .context("Failed to connect to coordinator")?;

    if !resp.status().is_success() {
        anyhow::bail!("Failed to get key shares: {}", resp.status());
    }

    let data: serde_json::Value = resp.json().await?;

    if let Some(keyshares) = data["keyshares"].as_array() {
        if keyshares.is_empty() {
            println!("No FROST key shares found.");
            println!();
            println!("To create a Taproot wallet:");
            println!("  mpc-wallet taproot-create -n <wallet_name>");
        } else {
            for ks in keyshares {
                let wallet_id = ks["wallet_id"].as_str().unwrap_or("-");
                let public_key = ks["public_key"].as_str().unwrap_or("-");
                let threshold = ks["threshold"].as_u64().unwrap_or(0);
                let num_parties = ks["num_parties"].as_u64().unwrap_or(0);
                let nodes_with_shares = ks["nodes_with_shares"]
                    .as_array()
                    .map(|a| a.len())
                    .unwrap_or(0);

                println!("Wallet: {}", wallet_id);
                println!("-----------------------------------------");
                if let Some(address) = ks["address"].as_str() {
                    println!("  Address:     {}", address);
                }
                if public_key.len() > 16 {
                    println!("  Public Key:  {}...", &public_key[..16]);
                } else {
                    println!("  Public Key:  {}", public_key);
                }
                println!("  Threshold:   {}-of-{}", threshold, num_parties);
                println!("  Type:        Taproot (P2TR)");
                println!(
                    "  Nodes:       {:?}",
                    ks["nodes_with_shares"]
                        .as_array()
                        .map(|a| a.iter().filter_map(|v| v.as_u64()).collect::<Vec<_>>())
                        .unwrap_or_default()
                );

                if threshold > 0 && nodes_with_shares >= threshold as usize {
                    println!(
                        "  Status:      Ready for signing ({}/{} shares)",
                        nodes_with_shares, num_parties
                    );
                } else {
                    println!(
                        "  Status:      Not enough shares ({}/{}, need {})",
                        nodes_with_shares, num_parties, threshold
                    );
                }
                println!();
            }
        }
    } else {
        println!("No FROST key shares found.");
    }

    println!("===================================================================");

    Ok(())
}
