//! Wallet management commands.

use anyhow::Result;

use crate::{client::ApiClient, output::OutputFormatter};

/// Get wallet balance
pub async fn get_balance(client: &ApiClient, formatter: &OutputFormatter) -> Result<()> {
    formatter.info("Fetching wallet balance...");

    let balance = client.get_wallet_balance().await?;

    if formatter.json_mode {
        formatter.json(&balance)?;
    } else {
        formatter.header("Wallet Balance");
        formatter.kv("Confirmed", &formatter.format_sats(balance.confirmed));
        formatter.kv(
            "Confirmed (BTC)",
            &formatter.format_btc(balance.confirmed),
        );

        if balance.unconfirmed != 0 {
            formatter.kv(
                "Unconfirmed",
                &format!("{} sats", balance.unconfirmed),
            );
        }

        formatter.kv("Total", &formatter.format_sats(balance.total));
        formatter.kv("Total (BTC)", &formatter.format_btc(balance.total));
    }

    Ok(())
}

/// Get wallet receiving address
pub async fn get_address(client: &ApiClient, formatter: &OutputFormatter) -> Result<()> {
    formatter.info("Fetching wallet address...");

    let address = client.get_wallet_address().await?;

    if formatter.json_mode {
        formatter.json(&address)?;
    } else {
        formatter.header("Wallet Address");
        formatter.kv("Address", &address.address);
        formatter.kv("Type", &address.address_type);

        println!();
        formatter.success(&format!(
            "Send Bitcoin to this address: {}",
            address.address
        ));
    }

    Ok(())
}
