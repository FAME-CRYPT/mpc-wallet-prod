mod app;
mod benchmark;
mod cli;
mod config;

use app::ThresholdVotingApp;
use cli::{Cli, Commands};
use clap::Parser;
use config::AppConfig;
use threshold_types::TransactionId;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use rustls;

#[tokio::main]
async fn main() {
    // Install default crypto provider for rustls before any TLS operations
    let _ = rustls::crypto::ring::default_provider().install_default();

    if let Err(e) = run().await {
        error!("Application error: {}", e);
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    // Initialize tracing first with default level
    init_tracing("info", "json");

    info!("Loading configuration...");
    let config = AppConfig::load().map_err(|e| {
        error!("Failed to load config: {}", e);
        e
    })?;

    info!("Configuration loaded successfully");
    info!("  Node ID: {}", config.node.node_id);
    info!("  Listen Addr: {}", config.node.listen_addr);
    info!("  mTLS CA: {}", config.mtls.ca_cert_path);

    info!("===================================");
    info!("mTLS Threshold Voting System");
    info!("===================================");

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Run) | None => {
            run_node(config).await?;
        }
        Some(Commands::Vote { tx_id, value }) => {
            submit_vote(config, tx_id, value).await?;
        }
        Some(Commands::Status { tx_id }) => {
            query_status(config, tx_id).await?;
        }
        Some(Commands::Info) => {
            show_info(config).await?;
        }
        Some(Commands::Peers) => {
            list_peers(config).await?;
        }
        Some(Commands::Benchmark { iterations, verbose }) => {
            benchmark::benchmark_serialization(iterations, verbose);
        }
        Some(Commands::BenchmarkAll { iterations, verbose }) => {
            benchmark::run_all_benchmarks(iterations, verbose).await;
        }
        Some(Commands::TestByzantine { test_type, tx_id }) => {
            test_byzantine(config, test_type, tx_id).await?;
        }
        Some(Commands::Monitor { interval }) => {
            monitor_network(config, interval).await?;
        }
    }

    Ok(())
}

async fn run_node(config: AppConfig) -> anyhow::Result<()> {
    info!("Starting mTLS node: {}", config.node.node_id);

    let app = ThresholdVotingApp::new(config).await?;
    app.run().await?;

    Ok(())
}

async fn submit_vote(config: AppConfig, tx_id: String, value: u64) -> anyhow::Result<()> {
    info!("Submitting vote: tx_id={} value={}", tx_id, value);

    let app = ThresholdVotingApp::new(config).await?;
    let vote = app.submit_vote(TransactionId::from(tx_id), value)?;

    info!("Vote created successfully");
    info!("  TX ID: {}", vote.tx_id);
    info!("  Node ID: {}", vote.node_id.0);
    info!("  Value: {}", vote.value);
    info!("  Signature: {}", hex::encode(&vote.signature));

    Ok(())
}

async fn query_status(config: AppConfig, tx_id: String) -> anyhow::Result<()> {
    info!("Querying status for transaction: {}", tx_id);

    let app = ThresholdVotingApp::new(config).await?;
    let tx_id = TransactionId::from(tx_id);

    let vote_counts = app.get_vote_counts(&tx_id).await?;
    let threshold = app.get_threshold().await?;
    let state = app.get_transaction_state(&tx_id).await.ok();
    let has_submission = app.has_blockchain_submission(&tx_id).await.ok().unwrap_or(false);

    println!("\n📊 Transaction Status");
    println!("─────────────────────────────────────");
    println!("  TX ID:       {}", tx_id);
    println!("  Status:      {:?}", state.unwrap_or(threshold_types::TransactionState::Collecting));
    println!("  Threshold:   {}", threshold);

    if !vote_counts.is_empty() {
        println!("\n  Vote Counts:");
        for (value, count) in vote_counts.iter() {
            let marker = if *count >= threshold { "✓" } else { " " };
            println!("    {} value={}: {} votes", marker, value, count);
        }
    } else {
        println!("  Vote Count:  No votes yet");
    }

    if has_submission {
        println!("\n  Submitted:   true");
    }

    Ok(())
}

async fn show_info(config: AppConfig) -> anyhow::Result<()> {
    let app = ThresholdVotingApp::new(config.clone()).await?;

    println!("\n📡 Node Information");
    println!("─────────────────────────────────────");
    println!("  Node ID:     {}", app.get_node_id());
    println!("  Listen:      {}", config.node.listen_addr);
    println!("  Public Key:  {}", hex::encode(&app.get_public_key()));
    println!("\n🔒 mTLS Configuration");
    println!("─────────────────────────────────────");
    println!("  CA Cert:     {}", config.mtls.ca_cert_path);
    println!("  Node Cert:   {}", config.mtls.node_cert_path);
    println!("  TLS Version: {}", config.mtls.tls_version);

    let threshold = app.get_threshold().await?;
    println!("\n⚙️  Consensus Configuration");
    println!("─────────────────────────────────────");
    println!("  Total Nodes: {}", config.consensus.total_nodes);
    println!("  Threshold:   {}", threshold);

    Ok(())
}

async fn list_peers(config: AppConfig) -> anyhow::Result<()> {
    println!("\n🌐 Connected Peers (mTLS)");
    println!("─────────────────────────────────────");
    println!("⚠️  Peer access requires running node instance");
    println!("Bootstrap peers:");
    for peer in &config.network.bootstrap_peers {
        println!("  - {}", peer);
    }
    Ok(())
}

async fn test_byzantine(config: AppConfig, test_type: String, tx_id: Option<String>) -> anyhow::Result<()> {
    info!("Running Byzantine test: {}", test_type);

    let app = ThresholdVotingApp::new(config).await?;
    let test_tx_id = TransactionId::from(tx_id.unwrap_or_else(|| "test_byzantine_001".to_string()));

    println!("\n🛡️  Byzantine Fault Detection Test");
    println!("─────────────────────────────────────");
    println!("  Test Type:   {}", test_type);
    println!("  TX ID:       {}", test_tx_id);

    match test_type.as_str() {
        "double-vote" => {
            println!("\n[SIMULATION] Double-voting attack:");
            println!("  1. Node votes value=42");
            let vote1 = app.submit_vote(test_tx_id.clone(), 42)?;
            println!("     ✓ First vote created: {}", hex::encode(&vote1.signature)[..16].to_string());

            println!("  2. Same node votes value=99");
            let vote2 = app.submit_vote(test_tx_id.clone(), 99)?;
            println!("     ✓ Second vote created: {}", hex::encode(&vote2.signature)[..16].to_string());

            println!("\n[EXPECTED] Byzantine detector should:");
            println!("  - Detect conflicting votes from same node");
            println!("  - Ban the peer");
            println!("  - Abort transaction");
        }
        "minority-attack" => {
            println!("\n[SIMULATION] Minority vote attack:");
            println!("  Scenario: 4/5 nodes vote '1', malicious node votes '2'");
            println!("  Expected: Malicious node detected and banned");
            println!("\n⚠️  Full simulation requires multiple running nodes");
        }
        "invalid-signature" => {
            println!("\n[SIMULATION] Invalid signature attack:");
            println!("  Expected: Signature verification fails immediately");
            println!("  Action: Reject vote, ban peer");
        }
        _ => {
            println!("\n❌ Unknown test type: {}", test_type);
            println!("Available types: double-vote, minority-attack, invalid-signature");
        }
    }

    Ok(())
}

async fn monitor_network(config: AppConfig, interval: u64) -> anyhow::Result<()> {
    info!("Starting network monitor (interval: {}s)", interval);

    let app = ThresholdVotingApp::new(config.clone()).await?;

    println!("\n📊 mTLS Network Monitor");
    println!("─────────────────────────────────────");
    println!("  Update Interval: {}s", interval);
    println!("  Node:            {}", app.get_node_id());
    println!("─────────────────────────────────────\n");

    loop {
        let threshold = app.get_threshold().await?;

        println!("⏰ {} UTC", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"));
        println!("  Threshold:            {}", threshold);
        println!("  Total Nodes:          {}", config.consensus.total_nodes);
        println!("  Bootstrap Peers:      {}", config.network.bootstrap_peers.len());
        println!("  etcd Endpoints:       {}", config.etcd.endpoints.len());
        println!("  Status:               ✓ Monitoring");
        println!();

        tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
    }
}

fn init_tracing(level: &str, _format: &str) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level));

    // Simple text output (JSON format requires additional features)
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}
