//! Aux Info (Auxiliary Information) commands.
//!
//! Aux info is required for CGGMP24 presignature generation and contains
//! Paillier encryption keys and ring-Pedersen parameters.

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

use crate::{client::ApiClient, output::OutputFormatter};

/// Start aux_info generation ceremony
pub async fn start_aux_info(
    client: &ApiClient,
    formatter: &OutputFormatter,
    num_parties: u16,
) -> Result<()> {
    // Validate parameters
    if num_parties < 2 {
        anyhow::bail!("Number of parties must be at least 2");
    }

    formatter.header("Starting Aux Info Generation");
    formatter.kv("Number of Parties", &num_parties.to_string());

    println!();

    // Generate participant IDs (1..=num_parties)
    let participants: Vec<u64> = (1..=num_parties as u64).collect();

    // Show progress indicator
    let pb = if !formatter.json_mode {
        let progress = ProgressBar::new_spinner();
        progress.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );
        progress.set_message("Generating auxiliary information...");
        progress.enable_steady_tick(Duration::from_millis(100));
        Some(progress)
    } else {
        None
    };

    // Call API
    let result = client.start_aux_info(num_parties, participants).await?;

    if let Some(pb) = pb {
        pb.finish_and_clear();
    }

    // Display results
    if formatter.json_mode {
        formatter.json(&result)?;
    } else {
        if result.success {
            formatter.success("Aux info generation completed successfully!");
            println!();
            formatter.kv("Session ID", &result.session_id);
            formatter.kv("Party Index", &result.party_index.to_string());
            formatter.kv("Aux Info Size", &format!("{} bytes", result.aux_info_size_bytes));
            println!();
            formatter.info("Aux info is stored and ready for presignature generation");
        } else {
            formatter.error(&format!(
                "Aux info generation failed: {}",
                result
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string())
            ));
        }
    }

    Ok(())
}

/// Get aux_info status
pub async fn get_aux_info_status(
    client: &ApiClient,
    formatter: &OutputFormatter,
) -> Result<()> {
    formatter.header("Aux Info Status");
    println!();

    // Call API
    let status = client.get_aux_info_status().await?;

    // Display results
    if formatter.json_mode {
        formatter.json(&status)?;
    } else {
        if status.has_aux_info {
            formatter.success("Aux info available");
            println!();

            if let Some(latest_session) = &status.latest_session_id {
                formatter.kv("Latest Session ID", latest_session);
            }
            formatter.kv(
                "Aux Info Size",
                &format!("{} bytes", status.aux_info_size_bytes),
            );
            formatter.kv("Total Ceremonies", &status.total_ceremonies.to_string());
            println!();

            formatter.info("Aux info is ready for presignature generation");
            formatter.info("Use 'threshold-wallet presig generate' to create presignatures");
        } else {
            formatter.warning("No aux info available");
            println!();
            formatter.kv("Total Ceremonies", &status.total_ceremonies.to_string());
            println!();
            formatter.info("Run 'threshold-wallet aux-info start' to generate aux info");
            formatter.info("Aux info is required before generating presignatures");
        }
    }

    Ok(())
}
