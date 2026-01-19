use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "threshold-voting-mtls")]
#[command(version, about = "mTLS-based threshold voting system with Byzantine Fault Tolerance", long_about = None)]
#[command(author = "MPC Wallet Team")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run the voting node (default mode)
    Run,

    /// Submit a vote for a transaction
    Vote {
        #[arg(short, long, help = "Transaction ID")]
        tx_id: String,

        #[arg(short, long, help = "Value to vote for")]
        value: u64,
    },

    /// Query transaction status and vote counts
    Status {
        #[arg(short, long, help = "Transaction ID")]
        tx_id: String,
    },

    /// Show detailed node information
    Info,

    /// List all connected peers
    Peers,

    /// Benchmark serialization performance
    Benchmark {
        #[arg(short, long, default_value = "1000", help = "Number of iterations")]
        iterations: usize,

        #[arg(short, long, default_value = "false", help = "Show detailed stats")]
        verbose: bool,
    },

    /// Run all comprehensive benchmarks
    BenchmarkAll {
        #[arg(short, long, default_value = "1000", help = "Number of iterations")]
        iterations: usize,

        #[arg(short, long, default_value = "false", help = "Show detailed stats")]
        verbose: bool,
    },

    /// Test Byzantine fault detection mechanisms
    TestByzantine {
        #[arg(short, long, help = "Test type: double-vote, invalid-signature, minority-attack")]
        test_type: String,

        #[arg(long, help = "Transaction ID for test")]
        tx_id: Option<String>,
    },

    /// Monitor network health and metrics
    Monitor {
        #[arg(short, long, default_value = "5", help = "Update interval in seconds")]
        interval: u64,
    },
}
