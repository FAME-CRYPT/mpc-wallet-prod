//! Send Bitcoin transaction command.

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;
use threshold_types::TransactionState;

use crate::{client::ApiClient, output::OutputFormatter};

/// Send Bitcoin to an address
pub async fn send_bitcoin(
    client: &ApiClient,
    formatter: &OutputFormatter,
    to_address: String,
    amount_sats: u64,
    metadata: Option<String>,
) -> Result<()> {
    // Validate inputs
    if to_address.is_empty() {
        anyhow::bail!("Recipient address cannot be empty");
    }

    if amount_sats == 0 {
        anyhow::bail!("Amount must be greater than zero");
    }

    // Validate metadata size if provided
    if let Some(ref meta) = metadata {
        if meta.len() > 80 {
            anyhow::bail!("Metadata exceeds maximum size of 80 bytes");
        }
    }

    // Show transaction details
    formatter.header("Transaction Details");
    formatter.kv("Recipient", &to_address);
    formatter.kv("Amount", &formatter.format_sats(amount_sats));
    formatter.kv("Amount (BTC)", &formatter.format_btc(amount_sats));
    if let Some(ref meta) = metadata {
        formatter.kv("Metadata", meta);
    }

    println!();
    formatter.info("Creating transaction...");

    // Create transaction
    let tx = client
        .create_transaction(to_address.clone(), amount_sats, metadata)
        .await
        .context("Failed to create transaction")?;

    formatter.success(&format!("Transaction created: {}", tx.txid));
    formatter.kv("Transaction ID", &tx.txid);
    formatter.kv("State", &formatter.format_state(&tx.state.to_string()));
    formatter.kv("Fee", &formatter.format_sats(tx.fee_sats));

    // If the transaction needs signing, show progress
    if matches!(
        tx.state,
        TransactionState::Pending
            | TransactionState::Voting
            | TransactionState::Collecting
            | TransactionState::Signing
    ) {
        println!();
        formatter.info("Transaction submitted to threshold signing process");
        formatter.info(&format!("Track progress with: threshold-wallet tx status {}", tx.txid));

        // Optional: Monitor transaction progress
        if !formatter.json_mode {
            println!();
            let monitor = dialoguer::Confirm::new()
                .with_prompt("Monitor transaction progress?")
                .default(true)
                .interact()
                .unwrap_or(false);

            if monitor {
                monitor_transaction(client, formatter, &tx.txid).await?;
            }
        }
    }

    Ok(())
}

/// Monitor transaction progress until completion
async fn monitor_transaction(
    client: &ApiClient,
    formatter: &OutputFormatter,
    txid: &str,
) -> Result<()> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );

    let mut last_state = TransactionState::Pending;

    loop {
        let tx = match client.get_transaction(txid).await {
            Ok(tx) => tx,
            Err(e) => {
                spinner.finish_with_message(format!("Failed to fetch status: {}", e));
                return Err(e);
            }
        };

        // Update spinner if state changed
        if tx.state != last_state {
            spinner.set_message(format!("State: {}", formatter.format_state(&tx.state.to_string())));
            last_state = tx.state;
        }

        // Check if transaction is complete
        match tx.state {
            TransactionState::Confirmed => {
                spinner.finish_with_message("Transaction confirmed!");
                formatter.success(&format!("Transaction {} confirmed on-chain", txid));
                break;
            }
            TransactionState::Failed | TransactionState::Rejected | TransactionState::AbortedByzantine => {
                spinner.finish_with_message(format!("Transaction {}", tx.state.to_string()));
                formatter.error(&format!(
                    "Transaction {} - state: {}",
                    txid,
                    tx.state.to_string()
                ));
                break;
            }
            _ => {
                // Continue monitoring
                spinner.tick();
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }

    Ok(())
}
