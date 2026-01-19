use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "threshold-voting-system")]
#[command(version, about = "Distributed threshold voting system with Byzantine Fault Tolerance", long_about = None)]
#[command(author = "Threshold Voting Team")]
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

    /// Query node reputation score
    Reputation {
        #[arg(short, long, help = "Node ID to query")]
        node_id: String,
    },

    /// List all connected peers
    Peers,

    /// Send direct P2P message to peer (testing)
    Send {
        #[arg(short, long, help = "Peer ID (12D3KooW...)")]
        peer_id: String,

        #[arg(short, long, help = "Message content")]
        message: String,
    },

    /// Benchmark serialization performance (JSON vs Binary)
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
