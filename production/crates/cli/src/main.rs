//! Threshold wallet CLI.
//!
//! Command-line interface for interacting with the production MPC wallet system.
//! Provides commands for:
//! - Wallet operations (balance, address)
//! - Transaction management (send, status, list)
//! - Cluster monitoring (status, nodes)
//! - DKG initialization (CGGMP24, FROST)
//! - Presignature generation

use anyhow::Result;
use clap::{Parser, Subcommand};

mod client;
mod commands;
mod config;
mod output;

use client::ApiClient;
use config::Config;
use output::OutputFormatter;

/// Threshold Wallet CLI
#[derive(Parser)]
#[command(name = "threshold-wallet")]
#[command(author, version, about = "Production MPC Threshold Wallet CLI", long_about = None)]
struct Cli {
    /// API server endpoint (overrides config)
    #[arg(long, global = true)]
    api_endpoint: Option<String>,

    /// Output format: table, json
    #[arg(long, global = true, value_name = "FORMAT")]
    output: Option<String>,

    /// Disable colored output
    #[arg(long, global = true)]
    no_color: bool,

    /// Enable JSON output (shorthand for --output json)
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Wallet operations
    #[command(subcommand)]
    Wallet(WalletCommands),

    /// Send Bitcoin transaction
    Send {
        /// Recipient Bitcoin address
        #[arg(long, value_name = "ADDRESS")]
        to: String,

        /// Amount to send in satoshis
        #[arg(long, value_name = "SATS")]
        amount: u64,

        /// Optional OP_RETURN metadata (hex or UTF-8, max 80 bytes)
        #[arg(long, value_name = "HEX")]
        metadata: Option<String>,
    },

    /// Transaction operations
    #[command(subcommand)]
    Tx(TxCommands),

    /// Cluster operations
    #[command(subcommand)]
    Cluster(ClusterCommands),

    /// DKG (Distributed Key Generation) operations
    #[command(subcommand)]
    Dkg(DkgCommands),

    /// Presignature operations
    #[command(subcommand)]
    Presig(PresigCommands),

    /// Configuration management
    #[command(subcommand)]
    Config(ConfigCommands),
}

#[derive(Subcommand)]
enum WalletCommands {
    /// Get wallet balance
    Balance,

    /// Get wallet receiving address
    Address,
}

#[derive(Subcommand)]
enum TxCommands {
    /// Get transaction status
    Status {
        /// Transaction ID
        txid: String,
    },

    /// List all transactions
    List,
}

#[derive(Subcommand)]
enum ClusterCommands {
    /// Get cluster status
    Status,

    /// List cluster nodes
    Nodes,
}

#[derive(Subcommand)]
enum DkgCommands {
    /// Start DKG ceremony
    Start {
        /// Protocol: cggmp24 or frost
        #[arg(long, value_name = "PROTOCOL")]
        protocol: String,

        /// Signing threshold (minimum signatures required)
        #[arg(long, value_name = "N")]
        threshold: u32,

        /// Total number of participants
        #[arg(long, value_name = "M")]
        total: u32,
    },

    /// Get DKG status
    Status {
        /// DKG session ID (optional)
        #[arg(long, value_name = "ID")]
        session_id: Option<String>,
    },
}

#[derive(Subcommand)]
enum PresigCommands {
    /// Generate presignatures
    Generate {
        /// Number of presignatures to generate
        #[arg(long, default_value = "10")]
        count: u32,
    },

    /// List available presignatures
    List,

    /// Get presignature pool status
    Status,
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Show current configuration
    Show,

    /// Set API endpoint
    SetEndpoint {
        /// API server endpoint URL
        endpoint: String,
    },

    /// Set node ID
    SetNodeId {
        /// Node ID
        node_id: u64,
    },

    /// Set output format
    SetFormat {
        /// Output format (table or json)
        format: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let cli = Cli::parse();

    // Load configuration
    let mut config = Config::load()?;

    // Override config with CLI arguments
    if let Some(endpoint) = cli.api_endpoint {
        config.api_endpoint = endpoint;
    }

    if let Some(output) = cli.output {
        config.output_format = output;
    }

    if cli.no_color {
        config.colored = false;
    }

    if cli.json {
        config.output_format = "json".to_string();
    }

    // Create output formatter
    let json_mode = config.output_format == "json";
    let formatter = OutputFormatter::new(config.colored, json_mode);

    // Handle config commands separately (don't need API client)
    if let Commands::Config(config_cmd) = cli.command {
        return handle_config_command(config_cmd, config, &formatter).await;
    }

    // Create API client for other commands
    let client = ApiClient::new(config.api_endpoint.clone(), config.timeout_secs)
        .map_err(|e| {
            anyhow::anyhow!("Failed to create API client: {}", e)
        })?;

    // Execute command
    let result = match cli.command {
        Commands::Wallet(cmd) => handle_wallet_command(cmd, &client, &formatter).await,
        Commands::Send { to, amount, metadata } => {
            commands::send::send_bitcoin(&client, &formatter, to, amount, metadata).await
        }
        Commands::Tx(cmd) => handle_tx_command(cmd, &client, &formatter).await,
        Commands::Cluster(cmd) => handle_cluster_command(cmd, &client, &formatter).await,
        Commands::Dkg(cmd) => handle_dkg_command(cmd, &client, &formatter).await,
        Commands::Presig(cmd) => handle_presig_command(cmd, &client, &formatter).await,
        Commands::Config(_) => unreachable!(), // Handled above
    };

    // Handle errors
    if let Err(e) = result {
        formatter.error(&format!("Error: {}", e));
        std::process::exit(1);
    }

    Ok(())
}

async fn handle_wallet_command(
    cmd: WalletCommands,
    client: &ApiClient,
    formatter: &OutputFormatter,
) -> Result<()> {
    match cmd {
        WalletCommands::Balance => commands::wallet::get_balance(client, formatter).await,
        WalletCommands::Address => commands::wallet::get_address(client, formatter).await,
    }
}

async fn handle_tx_command(
    cmd: TxCommands,
    client: &ApiClient,
    formatter: &OutputFormatter,
) -> Result<()> {
    match cmd {
        TxCommands::Status { txid } => commands::tx::get_status(client, formatter, txid).await,
        TxCommands::List => commands::tx::list_transactions(client, formatter).await,
    }
}

async fn handle_cluster_command(
    cmd: ClusterCommands,
    client: &ApiClient,
    formatter: &OutputFormatter,
) -> Result<()> {
    match cmd {
        ClusterCommands::Status => commands::cluster::get_status(client, formatter).await,
        ClusterCommands::Nodes => commands::cluster::list_nodes(client, formatter).await,
    }
}

async fn handle_dkg_command(
    cmd: DkgCommands,
    client: &ApiClient,
    formatter: &OutputFormatter,
) -> Result<()> {
    match cmd {
        DkgCommands::Start {
            protocol,
            threshold,
            total,
        } => {
            let protocol = protocol.parse()?;
            commands::dkg::start_dkg(client, formatter, protocol, threshold, total).await
        }
        DkgCommands::Status { session_id } => {
            commands::dkg::get_dkg_status(client, formatter, session_id).await
        }
    }
}

async fn handle_presig_command(
    cmd: PresigCommands,
    client: &ApiClient,
    formatter: &OutputFormatter,
) -> Result<()> {
    match cmd {
        PresigCommands::Generate { count } => {
            commands::presig::generate_presignatures(client, formatter, count).await
        }
        PresigCommands::List => commands::presig::list_presignatures(client, formatter).await,
        PresigCommands::Status => commands::presig::get_presig_status(client, formatter).await,
    }
}

async fn handle_config_command(
    cmd: ConfigCommands,
    mut config: Config,
    formatter: &OutputFormatter,
) -> Result<()> {
    match cmd {
        ConfigCommands::Show => {
            if formatter.json_mode {
                formatter.json(&config)?;
            } else {
                formatter.header("Current Configuration");
                formatter.kv("API Endpoint", &config.api_endpoint);
                formatter.kv(
                    "Node ID",
                    &config
                        .node_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "Not set".to_string()),
                );
                formatter.kv("Timeout", &format!("{}s", config.timeout_secs));
                formatter.kv("Output Format", &config.output_format);
                formatter.kv("Colored Output", &config.colored.to_string());

                println!();
                let config_path = Config::config_path()?;
                formatter.info(&format!("Config file: {}", config_path.display()));
            }
            Ok(())
        }
        ConfigCommands::SetEndpoint { endpoint } => {
            config.set_api_endpoint(endpoint.clone())?;
            formatter.success(&format!("API endpoint set to: {}", endpoint));
            Ok(())
        }
        ConfigCommands::SetNodeId { node_id } => {
            config.set_node_id(node_id)?;
            formatter.success(&format!("Node ID set to: {}", node_id));
            Ok(())
        }
        ConfigCommands::SetFormat { format } => {
            config.set_output_format(format.clone())?;
            formatter.success(&format!("Output format set to: {}", format));
            Ok(())
        }
    }
}
