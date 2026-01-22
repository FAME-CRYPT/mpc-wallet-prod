//! DKG (Distributed Key Generation) commands.
//!
//! Note: DKG endpoints are not yet implemented in the REST API.
//! This module provides placeholder implementations that will be connected
//! once the API endpoints are available.

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

use crate::{client::ApiClient, output::OutputFormatter};

/// Protocol type for DKG
#[derive(Debug, Clone, Copy)]
pub enum DkgProtocol {
    Cggmp24,
    Frost,
}

impl std::fmt::Display for DkgProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DkgProtocol::Cggmp24 => write!(f, "CGGMP24"),
            DkgProtocol::Frost => write!(f, "FROST"),
        }
    }
}

impl std::str::FromStr for DkgProtocol {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "cggmp24" | "cggmp" => Ok(DkgProtocol::Cggmp24),
            "frost" => Ok(DkgProtocol::Frost),
            _ => anyhow::bail!("Invalid protocol. Must be 'cggmp24' or 'frost'"),
        }
    }
}

/// Start DKG ceremony
pub async fn start_dkg(
    client: &ApiClient,
    formatter: &OutputFormatter,
    protocol: DkgProtocol,
    threshold: u32,
    total: u32,
) -> Result<()> {
    // Validate parameters
    if threshold == 0 {
        anyhow::bail!("Threshold must be greater than zero");
    }

    if total == 0 {
        anyhow::bail!("Total participants must be greater than zero");
    }

    if threshold > total {
        anyhow::bail!("Threshold cannot exceed total participants");
    }

    // Typically threshold should be > total/2 for security
    if threshold <= total / 2 {
        formatter.warning(&format!(
            "Threshold {} is not greater than half of total participants {}. This may not provide adequate security.",
            threshold, total
        ));
    }

    formatter.header("Starting DKG Ceremony");
    formatter.kv("Protocol", &protocol.to_string());
    formatter.kv("Threshold", &threshold.to_string());
    formatter.kv("Total Participants", &total.to_string());

    println!();

    // Generate participant IDs (1..=total)
    let participants: Vec<u64> = (1..=total as u64).collect();

    // Show progress indicator
    let pb = if !formatter.json_mode {
        let progress = ProgressBar::new_spinner();
        progress.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );
        progress.set_message("Initiating DKG ceremony...");
        progress.enable_steady_tick(Duration::from_millis(100));
        Some(progress)
    } else {
        None
    };

    // Call API
    let protocol_str = match protocol {
        DkgProtocol::Cggmp24 => "cggmp24".to_string(),
        DkgProtocol::Frost => "frost".to_string(),
    };

    let result = client
        .start_dkg(protocol_str, threshold, total, participants)
        .await?;

    if let Some(pb) = pb {
        pb.finish_and_clear();
    }

    // Display results
    if formatter.json_mode {
        formatter.json(&result)?;
    } else {
        if result.success {
            formatter.success("DKG ceremony completed successfully!");
            println!();
            formatter.kv("Session ID", &result.session_id);
            formatter.kv("Party Index", &result.party_index.to_string());
            formatter.kv("Key Share Size", &format!("{} bytes", result.key_share_size_bytes));
            println!();

            match protocol {
                DkgProtocol::Cggmp24 => {
                    formatter.info("Generated CGGMP24 key for:");
                    println!("  - SegWit (P2WPKH) addresses");
                    println!("  - ECDSA threshold signatures");
                }
                DkgProtocol::Frost => {
                    formatter.info("Generated FROST key for:");
                    println!("  - Taproot (P2TR) addresses");
                    println!("  - Schnorr threshold signatures");
                }
            }
        } else {
            formatter.error(&format!(
                "DKG ceremony failed: {}",
                result.error.unwrap_or_else(|| "Unknown error".to_string())
            ));
        }
    }

    Ok(())
}

/// Get DKG status
pub async fn get_dkg_status(
    client: &ApiClient,
    formatter: &OutputFormatter,
    session_id: Option<String>,
) -> Result<()> {
    formatter.header("DKG Status");

    if let Some(id) = &session_id {
        formatter.kv("Querying Session", id);
        println!();
    }

    // Call API
    let status = client.get_dkg_status().await?;

    // Display results
    if formatter.json_mode {
        formatter.json(&status)?;
    } else {
        if status.has_key_share {
            formatter.success("Key share available");
            println!();

            if let Some(latest_session) = &status.latest_session_id {
                formatter.kv("Latest Session ID", latest_session);
            }
            formatter.kv("Key Share Size", &format!("{} bytes", status.key_share_size_bytes));
            formatter.kv("Total Ceremonies", &status.total_ceremonies.to_string());
            println!();

            formatter.info("Use 'threshold-wallet dkg start' to initiate a new ceremony");
        } else {
            formatter.warning("No key share available");
            println!();
            formatter.kv("Total Ceremonies", &status.total_ceremonies.to_string());
            println!();
            formatter.info("Run 'threshold-wallet dkg start' to generate keys");
        }
    }

    Ok(())
}
