//! Transaction status and listing commands.

use anyhow::Result;
use serde::Serialize;
use tabled::Tabled;

use crate::{client::ApiClient, output::OutputFormatter};

/// Get transaction status
pub async fn get_status(
    client: &ApiClient,
    formatter: &OutputFormatter,
    txid: String,
) -> Result<()> {
    formatter.info(&format!("Fetching status for transaction {}...", txid));

    let tx = client.get_transaction(&txid).await?;

    if formatter.json_mode {
        formatter.json(&tx)?;
    } else {
        formatter.header("Transaction Details");
        formatter.kv("Transaction ID", &tx.txid);
        formatter.kv("State", &formatter.format_state(&tx.state.to_string()));
        formatter.kv("Recipient", &tx.recipient);
        formatter.kv("Amount", &formatter.format_sats(tx.amount_sats));
        formatter.kv("Amount (BTC)", &formatter.format_btc(tx.amount_sats));
        formatter.kv("Fee", &formatter.format_sats(tx.fee_sats));

        if let Some(ref metadata) = tx.metadata {
            formatter.kv("Metadata", metadata);
        }

        formatter.kv("Created", &formatter.format_timestamp(&tx.created_at));
        formatter.kv("Updated", &formatter.format_timestamp(&tx.updated_at));

        println!();
        match tx.state.to_string().as_str() {
            "confirmed" => formatter.success("Transaction confirmed on-chain"),
            "failed" | "rejected" | "aborted_byzantine" => {
                formatter.error(&format!("Transaction {}", tx.state.to_string()))
            }
            _ => formatter.info(&format!("Transaction is in {} state", tx.state.to_string())),
        }
    }

    Ok(())
}

/// List all transactions
pub async fn list_transactions(client: &ApiClient, formatter: &OutputFormatter) -> Result<()> {
    formatter.info("Fetching transactions...");

    let response = client.list_transactions().await?;

    if formatter.json_mode {
        formatter.json(&response.transactions)?;
    } else {
        if response.total == 0 {
            formatter.info("No transactions found");
            return Ok(());
        }

        formatter.header(&format!("Transactions ({})", response.total));

        // Create table data
        let table_data: Vec<TransactionTableRow> = response
            .transactions
            .into_iter()
            .map(|tx| TransactionTableRow {
                txid: truncate_txid(&tx.txid),
                state: tx.state.to_string(),
                recipient: truncate_address(&tx.recipient),
                amount_sats: format_sats(tx.amount_sats),
                fee_sats: format_sats(tx.fee_sats),
                created: format_ago(&tx.created_at),
            })
            .collect();

        formatter.table(table_data);
        println!();
        formatter.info(&format!("Total: {} transaction(s)", response.total));
    }

    Ok(())
}

/// Table row for transaction list
#[derive(Tabled, Serialize)]
struct TransactionTableRow {
    #[tabled(rename = "TX ID")]
    txid: String,
    #[tabled(rename = "State")]
    state: String,
    #[tabled(rename = "Recipient")]
    recipient: String,
    #[tabled(rename = "Amount")]
    amount_sats: String,
    #[tabled(rename = "Fee")]
    fee_sats: String,
    #[tabled(rename = "Created")]
    created: String,
}

/// Truncate transaction ID for display
fn truncate_txid(txid: &str) -> String {
    if txid.len() > 16 {
        format!("{}...{}", &txid[..8], &txid[txid.len() - 8..])
    } else {
        txid.to_string()
    }
}

/// Truncate address for display
fn truncate_address(addr: &str) -> String {
    if addr.len() > 20 {
        format!("{}...{}", &addr[..10], &addr[addr.len() - 6..])
    } else {
        addr.to_string()
    }
}

/// Format satoshis for table display
fn format_sats(sats: u64) -> String {
    format!("{} sats", sats)
}

/// Format timestamp as "X ago"
fn format_ago(timestamp: &chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let duration = now.signed_duration_since(*timestamp);

    if duration.num_seconds() < 60 {
        format!("{}s ago", duration.num_seconds())
    } else if duration.num_minutes() < 60 {
        format!("{}m ago", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{}h ago", duration.num_hours())
    } else {
        format!("{}d ago", duration.num_days())
    }
}
