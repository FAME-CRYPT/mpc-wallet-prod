// Fast threshold signing using presignatures
//
// Uses pre-generated presignatures for sub-second signing
// Falls back to full signing protocol if no presignature is available

use crate::message_board_client::MessageBoardClient;
use crate::presignature;
use crate::presignature_pool;
use crate::signing;
use anyhow::Result;
use cggmp24::signing::{DataToSign, PartialSignature};
use cggmp24::KeyShare;
use log::info;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::time::{sleep, Duration};

/// Sign a message using a presignature from the pool (fast path)
///
/// If no presignature is available, falls back to full signing protocol (slow path)
pub async fn sign_message_fast<E, L>(
    request_id: String,
    message: &str,
    my_index: u16,
    participants: &[u16],
    key_share: &KeyShare<E, L>,
    client: MessageBoardClient,
    pool: presignature_pool::PresignaturePoolHandle<E, L>,
) -> Result<String>
where
    E: generic_ec::Curve,
    generic_ec::Point<E>: generic_ec::coords::HasAffineX<E>,
    L: cggmp24::security_level::SecurityLevel,
{
    // Check pool status
    let stats = pool.stats().await;
    info!(
        "Presignature pool status: {}/{} ({}% full)",
        stats.current_size, stats.target_size, stats.utilization
    );

    // Try to get a presignature from the pool
    if let Some(stored_presig) = pool.take().await {
        info!("Using FAST signing with presignature! (non-interactive)");

        // Use the presignature-based signing which is much faster
        // This still requires some coordination but is significantly faster than full protocol
        match sign_with_presignature(
            request_id.clone(),
            message,
            my_index,
            participants,
            key_share,
            client.clone(),
            stored_presig,
        )
        .await
        {
            Ok(signature) => {
                info!("Fast signing completed successfully!");
                return Ok(signature);
            }
            Err(e) => {
                info!("Fast signing failed: {}, falling back to full protocol", e);
                // Fall through to full protocol
            }
        }
    } else {
        info!("No presignature available, using full signing protocol");
    }

    // Fallback: use full signing protocol
    signing::sign_message(
        request_id,
        message,
        my_index,
        participants,
        key_share,
        client,
    )
    .await
}

/// Wrapper for serializing PartialSignature
#[derive(Serialize, Deserialize)]
struct SerializablePartialSignature {
    s: String, // base64-encoded scalar
}

/// Sign using a pre-generated presignature (fast, requires partial signature exchange)
async fn sign_with_presignature<E, L>(
    request_id: String,
    message: &str,
    my_index: u16,
    participants: &[u16],
    _key_share: &KeyShare<E, L>,
    client: MessageBoardClient,
    stored_presig: presignature::StoredPresignature<E, L>,
) -> Result<String>
where
    E: generic_ec::Curve,
    generic_ec::Point<E>: generic_ec::coords::HasAffineX<E>,
    L: cggmp24::security_level::SecurityLevel,
{
    info!("Computing partial signature locally (instant)...");

    // Step 1: Compute the message hash as DataToSign
    let data_to_sign = DataToSign::digest::<sha2::Sha256>(message.as_bytes());

    // Step 2: Locally issue partial signature using presignature (INSTANT - no network)
    let my_partial_sig = presignature::issue_partial_signature(
        stored_presig.presignature.clone(),
        data_to_sign.clone(),
    );

    info!("Partial signature computed locally! Now exchanging with other nodes...");

    // Step 3: Serialize partial signature for transmission
    // We need to convert PartialSignature to bytes for JSON transmission
    // PartialSignature contains a scalar which we can serialize via serde_json
    let partial_sig_json = serde_json::to_string(&my_partial_sig)
        .map_err(|e| anyhow::anyhow!("Failed to serialize partial signature: {}", e))?;

    // Step 4: Post our partial signature to MessageBoard using dedicated endpoint
    info!("Posting partial signature to MessageBoard...");
    let post_url = format!("{}/partial-signatures", client.base_url);
    let post_body = serde_json::json!({
        "request_id": request_id,
        "from_node": client.node_id,
        "payload": partial_sig_json
    });

    client
        .client
        .post(&post_url)
        .json(&post_body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to post partial signature: {}", e))?;

    // Step 5: Collect partial signatures from all participants (including ourselves)
    info!(
        "Collecting partial signatures from {} participants...",
        participants.len()
    );
    let mut partial_sigs: HashMap<u16, PartialSignature<E>> = HashMap::new();
    partial_sigs.insert(my_index, my_partial_sig);

    // Poll for other participants' partial signatures
    let max_wait = Duration::from_secs(10);
    let start = std::time::Instant::now();

    while partial_sigs.len() < participants.len() && start.elapsed() < max_wait {
        // Fetch partial signatures from dedicated endpoint
        let get_url = format!(
            "{}/partial-signatures?request_id={}",
            client.base_url, request_id
        );
        let response = client.client.get(&get_url).send().await?;

        if response.status().is_success() {
            let data: serde_json::Value = response.json().await?;
            if let Some(partial_sig_array) = data["partial_signatures"].as_array() {
                for partial_sig_msg in partial_sig_array {
                    let from_node = partial_sig_msg["from_node"].as_str().unwrap_or("");
                    let sender_index = parse_node_index(from_node)?;

                    // Skip if we already have this participant's signature
                    if partial_sigs.contains_key(&sender_index) {
                        continue;
                    }

                    // Skip if sender is not a participant
                    if !participants.contains(&sender_index) {
                        continue;
                    }

                    // Parse the partial signature payload
                    let payload = partial_sig_msg["payload"].as_str().unwrap_or("");
                    match serde_json::from_str::<PartialSignature<E>>(payload) {
                        Ok(partial_sig) => {
                            info!("Received partial signature from node-{}", sender_index + 1);
                            partial_sigs.insert(sender_index, partial_sig);
                        }
                        Err(e) => {
                            info!(
                                "Failed to parse partial signature from node-{}: {}",
                                sender_index + 1,
                                e
                            );
                        }
                    }
                }
            }
        }

        if partial_sigs.len() < participants.len() {
            sleep(Duration::from_millis(100)).await;
        }
    }

    if partial_sigs.len() < participants.len() {
        anyhow::bail!(
            "Only received {}/{} partial signatures within timeout",
            partial_sigs.len(),
            participants.len()
        );
    }

    info!(
        "Collected all {} partial signatures! Combining...",
        partial_sigs.len()
    );

    // Step 6: Combine partial signatures into final signature
    // Important: The partial signatures must be ordered by participant index
    let mut partial_sig_vec: Vec<PartialSignature<E>> = Vec::new();
    for &participant_idx in participants {
        if let Some(partial_sig) = partial_sigs.get(&participant_idx) {
            partial_sig_vec.push(partial_sig.clone());
        } else {
            anyhow::bail!(
                "Missing partial signature from participant {}",
                participant_idx
            );
        }
    }

    info!(
        "Combining {} partial signatures in participant order: {:?}",
        partial_sig_vec.len(),
        participants
    );
    let signature = presignature::combine_partial_signatures(
        &partial_sig_vec,
        &stored_presig.public_data,
        data_to_sign,
    )
    .map_err(|e| anyhow::anyhow!("Combine failed: {}", e))?;

    info!("Fast signing: Signature combined successfully!");

    // Convert signature to JSON string
    let signature_json = serde_json::to_string(&signature)
        .map_err(|e| anyhow::anyhow!("Failed to serialize signature: {}", e))?;

    Ok(signature_json)
}

/// Parse node index from node_id string ("node-1" -> 0)
fn parse_node_index(node_id: &str) -> Result<u16> {
    let parts: Vec<&str> = node_id.split('-').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid node ID format: {}", node_id);
    }
    let num: u16 = parts[1].parse()?;
    if num == 0 {
        anyhow::bail!("Node ID must be 1-based");
    }
    Ok(num - 1)
}
