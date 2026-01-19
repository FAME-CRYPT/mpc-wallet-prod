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

#[tokio::main]
async fn main() {
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
    info!("  Peer ID: {}", config.node.peer_id);
    info!("  Listen Addr: {}", config.node.listen_addr);

    info!("===================================");
    info!("Threshold Voting System Starting");
    info!("===================================");

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Run) | None => {
            run_node(config).await?;
        }
        Some(Commands::Vote { tx_id, value }) => {
            submit_vote(config, tx_id, value).await?;
        }
        Some(Commands::Benchmark { iterations, verbose }) => {
            benchmark::benchmark_serialization(iterations, verbose);
        }
        Some(Commands::BenchmarkAll { iterations, verbose }) => {
            benchmark::run_all_benchmarks(iterations, verbose).await;
        }
        Some(Commands::Status { tx_id }) => {
            query_status(config, tx_id).await?;
        }
        Some(Commands::Info) => {
            show_info(config).await?;
        }
        Some(Commands::Reputation { node_id }) => {
            query_reputation(config, node_id).await?;
        }
        Some(Commands::Peers) => {
            list_peers(config).await?;
        }
        Some(Commands::Send { peer_id, message }) => {
            send_p2p_message(config, peer_id, message).await?;
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
    info!("Starting node: {}", config.node.node_id);

    let app = ThresholdVotingApp::new(config).await?;
    app.run().await?;

    Ok(())
}

async fn submit_vote(config: AppConfig, tx_id: String, value: u64) -> anyhow::Result<()> {
    info!("Submitting vote: tx_id={} value={}", tx_id, value);

    let app = ThresholdVotingApp::new(config.clone()).await?;
    let vote = app.submit_vote(TransactionId::from(tx_id), value)?;

    info!("Vote created successfully");
    info!("  TX ID: {}", vote.tx_id);
    info!("  Node ID: {}", vote.node_id);
    info!("  Value: {}", vote.value);
    info!("  Signature: {}", hex::encode(&vote.signature));

    // Create a temporary P2P node to broadcast the vote
    info!("Broadcasting vote to network...");

    let (vote_tx, _vote_rx) = tokio::sync::mpsc::unbounded_channel();
    let libp2p_keypair = libp2p::identity::Keypair::generate_ed25519();
    let mut p2p_node = threshold_network::P2PNode::new(
        config.node_id(),
        libp2p_keypair,
        vote_tx,
    )?;

    // Listen on random port to avoid conflict with running node
    let random_port = 9100 + (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() % 900) as u16; // 9100-9999
    let listen_addr: libp2p::Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", random_port).parse()
        .map_err(|e| anyhow::anyhow!("Invalid listen address: {}", e))?;
    info!("Temporary P2P node listening on port {}", random_port);
    p2p_node.listen_on(listen_addr)?;

    // Connect to bootstrap peers
    for peer_addr in &config.network.bootstrap_peers {
        if let Ok(addr) = peer_addr.parse::<libp2p::Multiaddr>() {
            info!("Connecting to bootstrap peer: {}", addr);
            p2p_node.dial(addr)?;
        }
    }

    // Wait for connection establishment (needs more time for handshake + GossipSub subscription)
    info!("Waiting for peer connections...");
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // Broadcast the vote
    info!("Broadcasting vote...");
    p2p_node.broadcast_vote(vote.clone())?;
    info!("✅ Vote broadcasted to network");

    // Wait for propagation
    info!("Waiting for propagation...");
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

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
    println!("  Peer ID:     {}", app.get_peer_id());
    println!("  Listen:      {}", config.node.listen_addr);
    println!("  Public Key:  {}", hex::encode(&app.get_public_key()));

    let threshold = app.get_threshold().await?;
    println!("\n⚙️  Consensus Configuration");
    println!("─────────────────────────────────────");
    println!("  Total Nodes: {}", config.consensus.total_nodes);
    println!("  Threshold:   {}", threshold);

    Ok(())
}

async fn query_reputation(config: AppConfig, node_id: String) -> anyhow::Result<()> {
    info!("Querying reputation for node: {}", node_id);

    let app = ThresholdVotingApp::new(config).await?;
    let reputation = app.get_reputation(&node_id).await?;

    println!("\n⭐ Node Reputation");
    println!("─────────────────────────────────────");
    println!("  Node ID:     {}", node_id);
    println!("  Score:       {:.2}", reputation);
    println!("  Status:      {}", if reputation >= 0.5 { "✓ Trustworthy" } else { "⚠️ Low reputation" });

    Ok(())
}

async fn list_peers(config: AppConfig) -> anyhow::Result<()> {
    println!("\n🌐 Connected Peers");
    println!("─────────────────────────────────────");
    println!("⚠️  P2P node access requires running node instance");
    println!("Bootstrap peers:");
    for peer in &config.network.bootstrap_peers {
        println!("  - {}", peer);
    }
    Ok(())
}

async fn send_p2p_message(_config: AppConfig, peer_id: String, message: String) -> anyhow::Result<()> {
    info!("Sending P2P message to {}: {}", peer_id, message);
    println!("\n📨 Sending Direct P2P Message");
    println!("─────────────────────────────────────");
    println!("  Peer ID:     {}", peer_id);
    println!("  Message:     {}", message);
    println!("\n⚠️  This feature requires running P2P node instance");
    println!("Note: Use 'threshold-voting run' to start node first");
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

    println!("\n📊 Network Monitor");
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
        println!("  etcd Endpoints:       {}", config.storage.etcd_endpoints.len());
        println!("  Status:               ✓ Monitoring");
        println!();

        tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
    }
}

fn init_tracing(level: &str, format: &str) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level));

    match format {
        "json" => {
            tracing_subscriber::registry()
                .with(filter)
                .with(tracing_subscriber::fmt::layer().json())
                .init();
        }
        _ => {
            tracing_subscriber::registry()
                .with(filter)
                .with(tracing_subscriber::fmt::layer())
                .init();
        }
    }
}

mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}
