mod http_transport;
mod keygen;
mod message_board_client;
mod presignature;
mod presignature_pool;
mod signing;
mod signing_fast;

use anyhow::Result;
use cggmp24::supported_curves::Secp256k1;
use cggmp24::{KeyShare, PregeneratedPrimes};
use log::{debug, info};
use message_board_client::MessageBoardClient;
use rand::rngs::OsRng;
use std::env;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;

// Threshold configuration: 3-of-4
const THRESHOLD: u16 = 3;
const TOTAL_NODES: u16 = 4;

// Fixed execution ID for keygen (all nodes must use the same)
const KEYGEN_EXECUTION_ID: &[u8; 32] = b"threshold_keygen_initial_2024___";

/// Configuration for the node
struct Config {
    node_id: String,
    node_index: u16,
    message_board_url: String,
    key_share_path: PathBuf,
    aux_info_path: PathBuf,
    public_key_path: PathBuf,
}

impl Config {
    /// Load configuration from environment variables
    fn from_env() -> Result<Self> {
        let node_id = env::var("NODE_ID").unwrap_or_else(|_| "node-1".to_string());

        // Parse node index from node_id (e.g., "node-1" -> 0)
        let node_index = parse_node_index(&node_id)?;

        let message_board_url = env::var("MESSAGE_BOARD_URL")
            .unwrap_or_else(|_| "http://message-board:8080".to_string());

        // Path to store key share, aux info, and public key (use /tmp for now, /data in Docker)
        let data_dir = env::var("DATA_DIR").unwrap_or_else(|_| "/tmp".to_string());
        let key_share_path = PathBuf::from(format!("{}/key_share_{}.json", data_dir, node_id));
        let aux_info_path = PathBuf::from(format!("{}/aux_info_{}.json", data_dir, node_id));
        let public_key_path = PathBuf::from(format!("{}/public_key.json", data_dir));

        Ok(Config {
            node_id,
            node_index,
            message_board_url,
            key_share_path,
            aux_info_path,
            public_key_path,
        })
    }
}

/// Parse node index from node_id string
/// "node-1" -> 0, "node-2" -> 1, etc.
fn parse_node_index(node_id: &str) -> Result<u16> {
    let parts: Vec<&str> = node_id.split('-').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid node ID format: {}", node_id);
    }

    let num: u16 = parts[1].parse()?;
    if num == 0 {
        anyhow::bail!("Node ID must be 1-based");
    }
    Ok(num - 1) // Convert to 0-based index
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::init();

    // Load configuration
    let config = Config::from_env()?;
    info!("Starting threshold signing node: {}", config.node_id);
    info!("Node index: {}", config.node_index);
    info!("MessageBoard URL: {}", config.message_board_url);
    info!("Key share path: {:?}", config.key_share_path);

    // Create MessageBoard client
    let mb_client =
        MessageBoardClient::new(config.message_board_url.clone(), config.node_id.clone());

    // Check if we have a complete key share (incomplete + aux_info)
    // If not, run keygen and aux_info_gen
    let key_share: KeyShare<Secp256k1> = if keygen::key_share_exists(&config.key_share_path).await
        && tokio::fs::try_exists(&config.aux_info_path)
            .await
            .unwrap_or(false)
    {
        info!("Loading existing key share and aux info...");
        let incomplete = keygen::load_key_share(&config.key_share_path).await?;
        let aux_json = tokio::fs::read_to_string(&config.aux_info_path).await?;
        let aux_info = serde_json::from_str(&aux_json)?;

        // Combine into complete key share
        KeyShare::from_parts((incomplete.core, aux_info))?
    } else {
        info!("No complete key share found, running setup protocols...");
        info!("Waiting for other nodes to be ready...");

        // In a real system, nodes would coordinate startup
        // For now, just wait a bit for all nodes to start
        sleep(Duration::from_millis(100)).await;

        // Step 1: Run keygen to get incomplete key share
        if !keygen::key_share_exists(&config.key_share_path).await {
            info!("Step 1/2: Running distributed key generation...");
            let keygen_config = keygen::KeygenConfig {
                my_index: config.node_index,
                n: TOTAL_NODES,
                t: THRESHOLD,
                execution_id: *KEYGEN_EXECUTION_ID,
            };

            let incomplete_share = keygen::run_keygen(
                keygen_config,
                mb_client.clone(),
                "keygen_initial".to_string(),
            )
            .await?;

            keygen::save_key_share(&incomplete_share, &config.key_share_path).await?;

            // Save the shared public key (all nodes produce the same public key)
            keygen::save_public_key(&incomplete_share, &config.public_key_path).await?;
        }

        // Load the incomplete share
        let incomplete = keygen::load_key_share(&config.key_share_path).await?;

        // Step 2: Run aux_info_gen if we don't have aux info yet
        let aux_info = if !tokio::fs::try_exists(&config.aux_info_path)
            .await
            .unwrap_or(false)
        {
            info!("Step 2/2: Running auxiliary info generation...");
            info!("This may take a while (generating safe primes)...");

            // Generate primes (this is slow)
            let mut rng = OsRng;
            let pregenerated = PregeneratedPrimes::generate(&mut rng);

            // Create execution ID for aux gen
            let aux_eid_bytes = b"threshold_auxgen_initial_2024___";
            let aux_eid = cggmp24::ExecutionId::new(aux_eid_bytes);

            // Create transport for aux gen
            let aux_transport = http_transport::http_transport(
                mb_client.clone(),
                "auxgen_initial".to_string(),
                config.node_index,
            );
            let aux_party = round_based::MpcParty::connected(aux_transport);

            // Run aux info generation protocol
            let aux_info =
                cggmp24::aux_info_gen(aux_eid, config.node_index, TOTAL_NODES, pregenerated)
                    .start(&mut rng, aux_party)
                    .await?;

            // Save aux info
            let aux_json = serde_json::to_string_pretty(&aux_info)?;
            tokio::fs::write(&config.aux_info_path, aux_json).await?;
            info!("Auxiliary info saved to {:?}", config.aux_info_path);

            aux_info
        } else {
            // Load existing aux info
            let aux_json = tokio::fs::read_to_string(&config.aux_info_path).await?;
            serde_json::from_str(&aux_json)?
        };

        // Combine into complete key share
        KeyShare::from_parts((incomplete.core, aux_info))?
    };

    info!("Complete key share ready, initializing presignature pool...");

    // Create presignature pool (target: 20 presignatures, max: 30)
    let presig_pool = presignature_pool::PresignaturePool::new(20, 30);
    let pool_handle = presig_pool.clone_handle();

    // Start background task to generate presignatures
    // Now uses separate presignature endpoints to avoid conflicts with regular signing
    let bg_key_share = key_share.clone();
    let bg_client = mb_client.clone();
    let bg_pool = pool_handle.clone();

    tokio::spawn(async move {
        presignature_generation_loop(config.node_index, bg_key_share, bg_client, bg_pool).await
    });

    info!("Presignature pool initialized, background generation started");

    // Main signing loop (now uses presignatures for fast signing)
    run_signing_loop(config.node_index, key_share, mb_client, pool_handle).await?;

    Ok(())
}

/// Background loop: generate presignatures to keep the pool filled
async fn presignature_generation_loop(
    my_index: u16,
    key_share: KeyShare<Secp256k1>,
    client: MessageBoardClient,
    pool: presignature_pool::PresignaturePoolHandle<
        Secp256k1,
        cggmp24::security_level::SecurityLevel128,
    >,
) -> Result<()> {
    let participants = signing::select_participants(THRESHOLD, TOTAL_NODES);
    let mut presig_counter = 0u64;

    info!("Presignature generation loop started");

    loop {
        // Only node-0 initiates presignature generation (leader-based coordination)
        // All nodes will participate when they see the request
        if my_index == 0 && pool.needs_refill().await {
            let stats = pool.stats().await;
            info!(
                "Pool needs refill: {}/{} ({}% full)",
                stats.current_size, stats.target_size, stats.utilization
            );

            presig_counter += 1;
            let request_id = format!("presig_gen_{}", presig_counter);

            info!(
                "Generating presignature #{} with request_id: {}",
                presig_counter, request_id
            );

            match presignature::generate_presignature(
                request_id,
                my_index,
                &participants,
                &key_share,
                client.clone(),
            )
            .await
            {
                Ok(stored_presig) => {
                    info!("Presignature #{} generated successfully", presig_counter);
                    if let Err(e) = pool.add(stored_presig).await {
                        info!("Failed to add presignature to pool: {}", e);
                    }
                }
                Err(e) => {
                    info!("Failed to generate presignature: {}", e);
                    // Wait a bit before retrying
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        } else {
            // Pool is full, wait before checking again
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    }
}

/// Main loop: poll for signing requests and participate in signing
async fn run_signing_loop(
    my_index: u16,
    key_share: KeyShare<Secp256k1>,
    client: MessageBoardClient,
    pool: presignature_pool::PresignaturePoolHandle<
        Secp256k1,
        cggmp24::security_level::SecurityLevel128,
    >,
) -> Result<()> {
    let mut processed_requests: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    info!("Entering signing loop, polling every 100ms for fast response...");

    let mut processed_presig_requests: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    loop {
        // Poll MessageBoard for pending signing requests
        match poll_and_process_requests(
            my_index,
            &key_share,
            &client,
            &pool,
            &mut processed_requests,
        )
        .await
        {
            Ok(count) => {
                if count > 0 {
                    info!("Processed {} signing requests", count);
                }
            }
            Err(e) => {
                info!("Error in signing loop: {}", e);
                // Continue despite errors - don't crash the loop
            }
        }

        // Non-leader nodes: poll for presignature requests and participate
        if my_index != 0 {
            match poll_and_process_presignature_requests(
                my_index,
                &key_share,
                &client,
                &pool,
                &mut processed_presig_requests,
            )
            .await
            {
                Ok(count) => {
                    if count > 0 {
                        info!("Participated in {} presignature requests", count);
                    }
                }
                Err(e) => {
                    // Ignore errors in presignature polling
                    debug!("Error polling presignature requests: {}", e);
                }
            }
        }

        sleep(Duration::from_millis(100)).await;
    }
}

/// Poll for pending requests and process them
async fn poll_and_process_requests(
    my_index: u16,
    key_share: &KeyShare<Secp256k1>,
    client: &MessageBoardClient,
    pool: &presignature_pool::PresignaturePoolHandle<
        Secp256k1,
        cggmp24::security_level::SecurityLevel128,
    >,
    processed_requests: &mut std::collections::HashSet<String>,
) -> Result<usize> {
    // Fetch pending requests from MessageBoard
    let url = format!("{}/requests?status=pending", client.base_url);
    let response = client.client.get(&url).send().await?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to fetch pending requests: {}", response.status());
    }

    let body: serde_json::Value = response.json().await?;

    // Get requests array, or use empty vec if not found
    let empty_vec = vec![];
    let requests = body["requests"].as_array().unwrap_or(&empty_vec);

    let mut count = 0;

    for req_value in requests {
        let request_id = req_value["id"].as_str().unwrap_or("");
        let message = req_value["message"].as_str().unwrap_or("");

        if request_id.is_empty() || message.is_empty() {
            continue;
        }

        // Skip if already processed
        if processed_requests.contains(request_id) {
            continue;
        }

        info!("Found pending request: {}", request_id);

        // Determine participants for this request
        // Simple strategy: first THRESHOLD nodes (0, 1, 2)
        let participants = signing::select_participants(THRESHOLD, TOTAL_NODES);

        // Check if we're a participant
        if !participants.contains(&my_index) {
            info!("Not a participant for request {}, skipping", request_id);
            processed_requests.insert(request_id.to_string());
            continue;
        }

        info!("Participating in signing for request {}", request_id);
        info!("Participants: {:?}", participants);

        // Run fast signing protocol (uses presignatures if available)
        match signing_fast::sign_message_fast(
            request_id.to_string(),
            message,
            my_index,
            &participants,
            key_share,
            client.clone(),
            pool.clone(),
        )
        .await
        {
            Ok(signature) => {
                info!("Signing successful for request {}", request_id);

                // Submit the signature to MessageBoard
                // Only the first node to complete submits (to avoid duplicates)
                match client.submit_signature(request_id, &signature).await {
                    Ok(_) => {
                        info!("Signature submitted for request {}", request_id);
                    }
                    Err(e) => {
                        info!(
                            "Failed to submit signature (may be already submitted): {}",
                            e
                        );
                    }
                }

                processed_requests.insert(request_id.to_string());
                count += 1;
            }
            Err(e) => {
                info!("Signing failed for request {}: {}", request_id, e);
                // Don't mark as processed so we can retry
            }
        }
    }

    Ok(count)
}

/// Poll for pending presignature requests and participate in generation
/// This function is called by non-leader nodes to participate in presignature generation
async fn poll_and_process_presignature_requests(
    my_index: u16,
    key_share: &KeyShare<Secp256k1>,
    client: &MessageBoardClient,
    pool: &presignature_pool::PresignaturePoolHandle<
        Secp256k1,
        cggmp24::security_level::SecurityLevel128,
    >,
    processed_requests: &mut std::collections::HashSet<String>,
) -> Result<usize> {
    // Fetch pending presignature requests from MessageBoard
    let url = format!("{}/presignature-requests?status=pending", client.base_url);
    let response = client.client.get(&url).send().await?;

    if !response.status().is_success() {
        anyhow::bail!(
            "Failed to fetch pending presignature requests: {}",
            response.status()
        );
    }

    let body: serde_json::Value = response.json().await?;

    // Get requests array, or use empty vec if not found
    let empty_vec = vec![];
    let requests = body["requests"].as_array().unwrap_or(&empty_vec);

    let mut count = 0;

    for req_value in requests {
        let request_id = req_value["id"].as_str().unwrap_or("");

        if request_id.is_empty() {
            continue;
        }

        // Skip if already processed
        if processed_requests.contains(request_id) {
            continue;
        }

        info!("Found pending presignature request: {}", request_id);

        // Determine participants (same as signing: first THRESHOLD nodes)
        let participants = signing::select_participants(THRESHOLD, TOTAL_NODES);

        // Check if we're a participant
        if !participants.contains(&my_index) {
            info!(
                "Not a participant for presignature request {}, skipping",
                request_id
            );
            processed_requests.insert(request_id.to_string());
            continue;
        }

        info!(
            "Participating in presignature generation for request {}",
            request_id
        );
        info!("Participants: {:?}", participants);

        // Participate in presignature generation
        match presignature::generate_presignature(
            request_id.to_string(),
            my_index,
            &participants,
            key_share,
            client.clone(),
        )
        .await
        {
            Ok(stored_presig) => {
                info!(
                    "Presignature generated successfully for request {}",
                    request_id
                );

                // Add to pool
                if let Err(e) = pool.add(stored_presig).await {
                    info!("Failed to add presignature to pool: {}", e);
                }

                // Mark as processed
                processed_requests.insert(request_id.to_string());
                count += 1;
            }
            Err(e) => {
                info!(
                    "Presignature generation failed for request {}: {}",
                    request_id, e
                );
                // Don't mark as processed so we can retry
            }
        }
    }

    Ok(count)
}
