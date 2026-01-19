//! MPC Wallet CLI
//!
//! Command-line interface for interacting with the MPC wallet system.

mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use common::WalletType;
use tracing::{error, Level};

/// MPC Wallet CLI - Manage your threshold signature wallets.
#[derive(Parser, Debug)]
#[command(name = "mpc-wallet")]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Coordinator server URL.
    #[arg(long, default_value = "http://localhost:3000", global = true)]
    coordinator: String,

    /// Enable verbose output.
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum WalletTypeArg {
    Bitcoin,
    Ethereum,
}

impl From<WalletTypeArg> for WalletType {
    fn from(arg: WalletTypeArg) -> Self {
        match arg {
            WalletTypeArg::Bitcoin => WalletType::Bitcoin,
            WalletTypeArg::Ethereum => WalletType::Ethereum,
        }
    }
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Create a new MPC wallet (deprecated - use cggmp24-create or taproot-create).
    CreateWallet {
        /// Human-readable name for the wallet.
        #[arg(short, long)]
        name: String,

        /// Type of wallet to create.
        #[arg(short = 't', long, value_enum, default_value = "bitcoin")]
        wallet_type: WalletTypeArg,
    },

    /// List all wallets.
    ListWallets,

    /// Get details of a specific wallet.
    GetWallet {
        /// Wallet ID (UUID).
        #[arg(short, long)]
        wallet_id: String,
    },

    /// Delete a wallet.
    DeleteWallet {
        /// Wallet ID (UUID).
        #[arg(short, long)]
        wallet_id: String,

        /// Skip confirmation prompt.
        #[arg(short, long)]
        force: bool,
    },

    /// Derive HD addresses for a Bitcoin wallet (BIP32/BIP84).
    DeriveAddresses {
        /// Wallet ID (UUID).
        #[arg(short, long)]
        wallet_id: String,

        /// Starting index (default: 0).
        #[arg(short, long, default_value = "0")]
        start: u32,

        /// Number of addresses to derive (default: 5).
        #[arg(short, long, default_value = "5")]
        count: u32,

        /// Address type: "receiving" or "change".
        #[arg(short = 'a', long, default_value = "receiving")]
        address_type: String,
    },

    /// Get a specific address at an index.
    GetAddress {
        /// Wallet ID (UUID).
        #[arg(short, long)]
        wallet_id: String,

        /// Address index.
        #[arg(short, long)]
        index: u32,
    },

    /// Get wallet balance (Bitcoin only).
    Balance {
        /// Wallet ID (UUID).
        #[arg(short, long)]
        wallet_id: String,
    },

    /// Get testnet faucet information.
    Faucet {
        /// Wallet ID (UUID).
        #[arg(short, long)]
        wallet_id: String,
    },

    /// Mine blocks to a wallet address (regtest only).
    Mine {
        /// Wallet ID (UUID).
        #[arg(short, long)]
        wallet_id: String,

        /// Number of blocks to mine (default: 101 for coinbase maturity).
        #[arg(short, long, default_value = "101")]
        blocks: u32,
    },

    /// Send Bitcoin to an address.
    Send {
        /// Wallet ID (UUID).
        #[arg(short, long)]
        wallet_id: String,

        /// Destination address.
        #[arg(short, long)]
        to: String,

        /// Amount in satoshis.
        #[arg(short, long)]
        amount: u64,

        /// Fee rate in sat/vB (optional).
        #[arg(short, long)]
        fee_rate: Option<u64>,
    },

    /// Get system information from the coordinator.
    Info,

    // ========== CGGMP24 Commands ==========
    /// Initialize CGGMP24 system (generate aux_info on all nodes).
    Cggmp24Init,

    /// Check CGGMP24 node status (primes, aux_info, readiness).
    Cggmp24Status,

    /// Create a new CGGMP24 wallet with threshold signing.
    Cggmp24Create {
        /// Human-readable name for the wallet.
        #[arg(short, long)]
        name: String,

        /// Signing threshold (default: 3).
        #[arg(short, long, default_value = "3")]
        threshold: u16,
    },

    /// Send Bitcoin using CGGMP24 threshold signatures.
    Cggmp24Send {
        /// Wallet ID (UUID or wallet name used during cggmp24-create).
        #[arg(short, long)]
        wallet_id: String,

        /// Destination address.
        #[arg(short, long)]
        to: String,

        /// Amount in satoshis.
        #[arg(short, long)]
        amount: u64,

        /// Fee rate in sat/vB (optional).
        #[arg(short, long)]
        fee_rate: Option<u64>,
    },

    /// List CGGMP24 key shares on nodes.
    Cggmp24Keys,

    // ========== Taproot/FROST Commands ==========
    /// Create a new Taproot wallet using FROST threshold signatures.
    #[command(name = "taproot-create")]
    TaprootCreate {
        /// Wallet name.
        #[arg(short, long)]
        name: String,
        /// Threshold (t in t-of-n), default is 3 for 3-of-4.
        #[arg(short, long, default_value = "3")]
        threshold: u16,
    },

    /// Send Bitcoin using Taproot/FROST threshold signatures.
    #[command(name = "taproot-send")]
    TaprootSend {
        /// Wallet ID (UUID or wallet name).
        #[arg(short, long)]
        wallet_id: String,
        /// Recipient Bitcoin address.
        #[arg(short, long)]
        to: String,
        /// Amount in satoshis.
        #[arg(short, long)]
        amount: u64,
        /// Fee rate in sat/vB (optional).
        #[arg(short, long)]
        fee_rate: Option<u64>,
    },

    /// List Taproot/FROST key shares on nodes.
    #[command(name = "taproot-keys")]
    TaprootKeys,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    let level = if cli.verbose {
        Level::TRACE
    } else {
        Level::INFO
    };

    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(false)
        .without_time()
        .init();

    match cli.command {
        Commands::CreateWallet { .. } => {
            let message = "The 'create-wallet' command is deprecated. Use \
`mpc-wallet cggmp24-create --name \"name\"` or `mpc-wallet taproot-create --name \"name\"`.";
            error!("{}", message);
            anyhow::bail!(message);
        }
        Commands::ListWallets => {
            commands::list_wallets(&cli.coordinator).await?;
        }
        Commands::GetWallet { wallet_id } => {
            commands::get_wallet(&cli.coordinator, &wallet_id).await?;
        }
        Commands::DeleteWallet { wallet_id, force } => {
            commands::delete_wallet(&cli.coordinator, &wallet_id, force).await?;
        }
        Commands::DeriveAddresses {
            wallet_id,
            start,
            count,
            address_type,
        } => {
            commands::derive_addresses(&cli.coordinator, &wallet_id, start, count, &address_type)
                .await?;
        }
        Commands::GetAddress { wallet_id, index } => {
            commands::get_address(&cli.coordinator, &wallet_id, index).await?;
        }
        Commands::Balance { wallet_id } => {
            commands::get_balance(&cli.coordinator, &wallet_id).await?;
        }
        Commands::Faucet { wallet_id } => {
            commands::get_faucet(&cli.coordinator, &wallet_id).await?;
        }
        Commands::Mine { wallet_id, blocks } => {
            commands::mine_blocks(&cli.coordinator, &wallet_id, blocks).await?;
        }
        Commands::Send {
            wallet_id,
            to,
            amount,
            fee_rate,
        } => {
            commands::send_bitcoin(&cli.coordinator, &wallet_id, &to, amount, fee_rate).await?;
        }
        Commands::Info => {
            commands::get_info(&cli.coordinator).await?;
        }
        // CGGMP24 Commands
        Commands::Cggmp24Init => {
            commands::cggmp24_init(&cli.coordinator).await?;
        }
        Commands::Cggmp24Status => {
            commands::cggmp24_status(&cli.coordinator).await?;
        }
        Commands::Cggmp24Create { name, threshold } => {
            commands::cggmp24_create(&cli.coordinator, &name, threshold).await?;
        }
        Commands::Cggmp24Send {
            wallet_id,
            to,
            amount,
            fee_rate,
        } => {
            commands::cggmp24_send(&cli.coordinator, &wallet_id, &to, amount, fee_rate).await?;
        }
        Commands::Cggmp24Keys => {
            commands::cggmp24_keys(&cli.coordinator).await?;
        }
        // Taproot/FROST Commands
        Commands::TaprootCreate { name, threshold } => {
            commands::taproot_create(&cli.coordinator, &name, threshold).await?;
        }
        Commands::TaprootSend {
            wallet_id,
            to,
            amount,
            fee_rate,
        } => {
            commands::taproot_send(&cli.coordinator, &wallet_id, &to, amount, fee_rate).await?;
        }
        Commands::TaprootKeys => {
            commands::taproot_keys(&cli.coordinator).await?;
        }
    }

    Ok(())
}
