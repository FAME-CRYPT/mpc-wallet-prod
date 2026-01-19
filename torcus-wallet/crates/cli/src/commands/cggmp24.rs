//! CGGMP24 threshold ECDSA commands.

use anyhow::{Context, Result};
use common::{
    CreateWalletRequest, CreateWalletResponse, SendBitcoinRequest, SendBitcoinResponse, WalletType,
    NUM_PARTIES,
};

/// Initialize CGGMP24 system by generating aux_info on all nodes.
pub async fn cggmp24_init(coordinator_url: &str) -> Result<()> {
    println!("===================================================================");
    println!("              CGGMP24 SYSTEM INITIALIZATION");
    println!("===================================================================");
    println!();
    println!("This will generate auxiliary cryptographic parameters on all nodes.");
    println!("This process takes 1-5 minutes depending on hardware...");
    println!();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()?;

    let url = format!("{}/aux-info/start", coordinator_url);

    println!("Starting aux_info generation...");

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .await
        .context("Failed to connect to coordinator")?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        anyhow::bail!("Failed to start aux_info generation: {}", error_text);
    }

    let result: serde_json::Value = response.json().await?;

    println!();
    println!("Aux info generation started!");
    println!();
    println!(
        "  Session ID: {}",
        result["session_id"].as_str().unwrap_or("-")
    );
    println!();
    println!("The process runs in the background. Check status with:");
    println!("  mpc-wallet cggmp24-status");
    println!();

    // Poll for completion using coordinator's aggregated status
    println!("Waiting for completion (this may take several minutes)...");

    let status_url = format!("{}/nodes/status", coordinator_url);

    let mut ready = false;
    for i in 0..120 {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        if let Ok(resp) = client.get(&status_url).send().await {
            if let Ok(status) = resp.json::<serde_json::Value>().await {
                // Check if all nodes are ready
                if status["all_cggmp24_ready"].as_bool().unwrap_or(false) {
                    ready = true;
                    break;
                }
            }
        }

        if i % 6 == 0 {
            println!("  Still waiting... ({} seconds)", (i + 1) * 5);
        }
    }

    if ready {
        println!();
        println!("===================================================================");
        println!("          CGGMP24 SYSTEM READY FOR KEY GENERATION");
        println!("===================================================================");
    } else {
        println!();
        println!("Timeout waiting for aux_info. Check status manually:");
        println!("  mpc-wallet cggmp24-status");
    }

    Ok(())
}

/// Check CGGMP24 status on all nodes.
pub async fn cggmp24_status(coordinator_url: &str) -> Result<()> {
    println!("Checking CGGMP24 status on all nodes...");
    println!();

    let client = reqwest::Client::new();

    // Get aggregated node status from coordinator
    let status_url = format!("{}/nodes/status", coordinator_url);
    let resp = client
        .get(&status_url)
        .send()
        .await
        .context("Failed to connect to coordinator")?;

    if !resp.status().is_success() {
        anyhow::bail!("Failed to get node status: {}", resp.status());
    }

    let status: serde_json::Value = resp.json().await?;

    println!("===================================================================");
    println!("                    CGGMP24 NODE STATUS");
    println!("===================================================================");
    println!();

    if let Some(nodes) = status["nodes"].as_array() {
        for node in nodes {
            let node_index = node["node_index"].as_u64().unwrap_or(0);
            let reachable = node["reachable"].as_bool().unwrap_or(false);

            print!("Node {}: ", node_index);

            if !reachable {
                println!("UNREACHABLE");
                if let Some(err) = node["error"].as_str() {
                    println!("        Error: {}", err);
                }
            } else {
                let has_primes = node["primes_available"].as_bool().unwrap_or(false);
                let has_aux_info = node["aux_info_available"].as_bool().unwrap_or(false);
                let is_ready = node["cggmp24_ready"].as_bool().unwrap_or(false);
                let keyshares = node["cggmp24_key_shares"].as_u64().unwrap_or(0);

                if is_ready {
                    println!("READY");
                } else {
                    println!("NOT READY");
                }
                println!(
                    "        Primes:     {}",
                    if has_primes { "yes" } else { "no" }
                );
                println!(
                    "        Aux Info:   {}",
                    if has_aux_info { "yes" } else { "no" }
                );
                println!("        Key Shares: {}", keyshares);
            }
            println!();
        }
    }

    // Summary
    let all_reachable = status["all_reachable"].as_bool().unwrap_or(false);
    let all_ready = status["all_cggmp24_ready"].as_bool().unwrap_or(false);

    if all_reachable && all_ready {
        println!("System Status: ALL NODES READY");
    } else if !all_reachable {
        println!("System Status: SOME NODES UNREACHABLE");
    } else {
        println!("System Status: NODES NOT READY (run mpc-wallet cggmp24-init)");
    }
    println!();

    // Check relay sessions
    let sessions_url = format!("{}/relay/sessions", coordinator_url);
    if let Ok(resp) = client.get(&sessions_url).send().await {
        if let Ok(sessions) = resp.json::<serde_json::Value>().await {
            if let Some(active) = sessions["active_sessions"].as_array() {
                if !active.is_empty() {
                    println!("Active MPC Sessions: {}", active.len());
                    for session in active {
                        println!(
                            "  - {} ({})",
                            session["session_id"].as_str().unwrap_or("-"),
                            session["protocol"].as_str().unwrap_or("-")
                        );
                    }
                    println!();
                }
            }
        }
    }

    println!("===================================================================");

    Ok(())
}

/// Create a new CGGMP24 wallet.
pub async fn cggmp24_create(coordinator_url: &str, name: &str, threshold: u16) -> Result<()> {
    println!("===================================================================");
    println!("              CGGMP24 WALLET CREATION");
    println!("===================================================================");
    println!();
    println!("  Wallet Name: {}", name);
    println!("  Threshold:   {}-of-{}", threshold, NUM_PARTIES);
    println!();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?;

    // Step 1: Create wallet in coordinator
    println!("Step 1: Creating wallet record...");

    let create_url = format!("{}/wallet", coordinator_url);
    let create_request = CreateWalletRequest {
        name: name.to_string(),
        wallet_type: WalletType::Bitcoin,
    };

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
    println!("  Address: {}", wallet.address);
    println!();

    // Step 2: Start CGGMP24 key generation
    println!("Step 2: Running CGGMP24 distributed key generation...");
    println!("        (This uses real threshold cryptography - no key reconstruction!)");
    println!();

    let keygen_url = format!("{}/cggmp24/keygen/start", coordinator_url);
    let keygen_request = serde_json::json!({
        "wallet_id": wallet_id,
        "threshold": threshold
    });

    let response = client
        .post(&keygen_url)
        .json(&keygen_request)
        .send()
        .await
        .context("Failed to start CGGMP24 keygen")?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        // Clean up the wallet record since keygen failed
        eprintln!("  Keygen failed, cleaning up wallet record...");
        let delete_url = format!("{}/wallet/{}", coordinator_url, wallet_id);
        let _ = client.delete(&delete_url).send().await;
        anyhow::bail!("Failed to start CGGMP24 keygen: {}", error_text);
    }

    let keygen_result: serde_json::Value = response.json().await?;
    println!(
        "  Keygen session started: {}",
        keygen_result["session_id"].as_str().unwrap_or("-")
    );

    // Wait for keygen to complete
    println!();
    println!("Waiting for key generation to complete...");

    let keyshares_url = format!("{}/cggmp24/keyshares", coordinator_url);

    let mut keygen_complete = false;
    let mut cggmp24_pubkey: Option<String> = None;

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
                                cggmp24_pubkey = ks["public_key"].as_str().map(|s| s.to_string());
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
        println!("Step 3: Updating wallet with CGGMP24 public key...");

        let final_address: String;
        let final_pubkey: String;

        if let Some(pubkey) = cggmp24_pubkey {
            let update_url = format!("{}/wallet/{}/pubkey", coordinator_url, wallet_id);
            let update_request = serde_json::json!({
                "public_key": pubkey
            });

            match client.put(&update_url).json(&update_request).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        if let Ok(result) = resp.json::<serde_json::Value>().await {
                            final_address = result["address"]
                                .as_str()
                                .unwrap_or(&wallet.address)
                                .to_string();
                            final_pubkey = pubkey;
                            println!("  Wallet updated with CGGMP24 public key");
                            println!("  New address: {}", final_address);
                        } else {
                            final_address = wallet.address.clone();
                            final_pubkey = wallet.public_key.clone();
                            println!("  Could not parse update response, using original address");
                        }
                    } else {
                        final_address = wallet.address.clone();
                        final_pubkey = wallet.public_key.clone();
                        println!("  Failed to update wallet public key: {}", resp.status());
                    }
                }
                Err(e) => {
                    final_address = wallet.address.clone();
                    final_pubkey = wallet.public_key.clone();
                    println!("  Failed to update wallet: {}", e);
                }
            }
        } else {
            final_address = wallet.address.clone();
            final_pubkey = wallet.public_key.clone();
            println!("  Could not retrieve CGGMP24 public key from nodes");
        }

        println!();
        println!("===================================================================");
        println!("            CGGMP24 WALLET CREATED SUCCESSFULLY!");
        println!("===================================================================");
        println!();
        println!("  Wallet ID:    {}", wallet_id);
        println!("  Name:         {}", name);
        println!("  Address:      {}", final_address);
        println!("  Threshold:    {}-of-{}", threshold, NUM_PARTIES);
        println!("  Public Key:   {}", final_pubkey);
        println!();
        println!("Next steps:");
        println!("  1. Get testnet coins: mpc-wallet faucet -w {}", wallet_id);
        println!(
            "  2. Check balance:     mpc-wallet balance -w {}",
            wallet_id
        );
        println!(
            "  3. Send Bitcoin:      mpc-wallet cggmp24-send -w {} -t <address> -a <sats>",
            wallet_id
        );
        println!();
        println!("===================================================================");
    } else {
        println!();
        println!("Keygen may still be running. Check status with:");
        println!("  mpc-wallet cggmp24-keys");
    }

    Ok(())
}

/// Send Bitcoin using CGGMP24 threshold signatures.
pub async fn cggmp24_send(
    coordinator_url: &str,
    wallet_id: &str,
    to: &str,
    amount: u64,
    fee_rate: Option<u64>,
) -> Result<()> {
    println!("===================================================================");
    println!("           CGGMP24 THRESHOLD BITCOIN TRANSACTION");
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
    println!("This transaction will be signed using real CGGMP24 threshold");
    println!("signatures - the private key is NEVER reconstructed!");
    println!();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()?;

    let url = format!("{}/wallet/{}/send", coordinator_url, wallet_id);
    let request = SendBitcoinRequest {
        wallet_id: wallet_id.to_string(),
        to_address: to.to_string(),
        amount_sats: amount,
        fee_rate,
    };

    println!("Building transaction and collecting threshold signatures...");
    println!();

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

    println!("===================================================================");
    println!("          CGGMP24 TRANSACTION BROADCAST SUCCESS!");
    println!("===================================================================");
    println!();
    println!("  TXID:        {}", result.txid);
    println!("  Amount:      {} sats", result.amount_sats);
    println!("  Fee:         {} sats", result.fee_sats);
    println!("  To:          {}", result.to_address);
    println!();
    println!("  Signed with CGGMP24 threshold ECDSA (3-of-4)");
    println!();
    println!("{}", result.message);
    println!();
    println!("===================================================================");

    Ok(())
}

/// List CGGMP24 key shares on all nodes.
pub async fn cggmp24_keys(coordinator_url: &str) -> Result<()> {
    println!("===================================================================");
    println!("                   CGGMP24 KEY SHARES");
    println!("===================================================================");
    println!();

    let client = reqwest::Client::new();

    // Get aggregated key shares from coordinator
    let keyshares_url = format!("{}/cggmp24/keyshares", coordinator_url);
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
            println!("No CGGMP24 key shares found on any node.");
            println!();
            println!("Create a wallet with: mpc-wallet cggmp24-create -n <name>");
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
                println!("  Threshold:   {}-of-{}", threshold, num_parties);
                println!("  Public Key:  {}", public_key);
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
        println!("No CGGMP24 key shares found.");
    }

    println!("===================================================================");

    Ok(())
}
