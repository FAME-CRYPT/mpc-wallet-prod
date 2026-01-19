//! Protocol benchmark tests.
//!
//! Run with: cargo test --package protocols benchmark_ --release -- --nocapture
//!
//! These tests simulate multi-party MPC protocols locally and measure their
//! performance without network overhead.

use super::simulation::{Cggmp24Relay, FrostSigningRelay, MessageRelay};
use super::BenchmarkReport;
use rand::Rng;
use sha2::Digest;
use std::time::Instant;

/// Results from a full protocol benchmark run.
#[derive(Debug)]
pub struct ProtocolBenchmarkResult {
    pub protocol_name: String,
    pub num_parties: u16,
    pub threshold: u16,
    pub keygen_duration_ms: f64,
    pub signing_duration_ms: f64,
    pub total_duration_ms: f64,
    pub keygen_reports: Vec<BenchmarkReport>,
    pub signing_reports: Vec<BenchmarkReport>,
}

impl ProtocolBenchmarkResult {
    pub fn print_summary(&self) {
        println!("\n{}", "=".repeat(60));
        println!("  BENCHMARK RESULTS: {}", self.protocol_name);
        println!("{}", "=".repeat(60));
        println!(
            "Configuration: {}-of-{} threshold",
            self.threshold, self.num_parties
        );
        println!("{}", "-".repeat(60));
        println!("  Key Generation:  {:>10.2} ms", self.keygen_duration_ms);
        println!("  Signing:         {:>10.2} ms", self.signing_duration_ms);
        println!("{}", "-".repeat(60));
        println!("  TOTAL:           {:>10.2} ms", self.total_duration_ms);
        println!("{}", "=".repeat(60));

        // Print detailed breakdown from party 0
        if let Some(keygen_report) = self.keygen_reports.first() {
            println!("\nKeygen breakdown (party 0):");
            for step in &keygen_report.steps {
                let pct = if keygen_report.total_duration_ms > 0.0 {
                    (step.duration_ms / keygen_report.total_duration_ms) * 100.0
                } else {
                    0.0
                };
                println!(
                    "  {:45} {:>8.2} ms ({:>5.1}%)",
                    step.name, step.duration_ms, pct
                );
            }
        }

        if let Some(signing_report) = self.signing_reports.first() {
            println!("\nSigning breakdown (party 0):");
            for step in &signing_report.steps {
                let pct = if signing_report.total_duration_ms > 0.0 {
                    (step.duration_ms / signing_report.total_duration_ms) * 100.0
                } else {
                    0.0
                };
                println!(
                    "  {:45} {:>8.2} ms ({:>5.1}%)",
                    step.name, step.duration_ms, pct
                );
            }
        }
        println!();
    }
}

/// Generate a random session ID.
fn random_session_id(prefix: &str) -> String {
    let random_id: u64 = rand::thread_rng().gen();
    format!("{}-{}", prefix, random_id)
}

/// Run FROST keygen + signing benchmark.
pub async fn run_frost_benchmark(
    num_parties: u16,
    threshold: u16,
) -> Result<ProtocolBenchmarkResult, String> {
    use crate::frost::keygen::run_frost_keygen;
    use crate::frost::signing::run_frost_signing_with_benchmark;

    println!(
        "\n[FROST] Starting benchmark: {}-of-{}",
        threshold, num_parties
    );

    let session_id = random_session_id("frost-bench");

    // =========================================================================
    // KEYGEN PHASE
    // =========================================================================
    println!("[FROST] Running key generation...");
    let keygen_start = Instant::now();

    // Create message relay for keygen
    let (relay, party_channels) = MessageRelay::new_frost_keygen(num_parties, 10000);

    // Spawn the relay
    let relay_handle = tokio::spawn(relay.run_frost_keygen());

    // Spawn keygen tasks for each party
    let mut keygen_handles = Vec::new();
    for (party_idx, (out_tx, in_rx)) in party_channels.into_iter().enumerate() {
        let session_id = session_id.clone();
        let handle = tokio::spawn(async move {
            run_frost_keygen(
                party_idx as u16,
                num_parties,
                threshold,
                &session_id,
                in_rx,
                out_tx,
            )
            .await
        });
        keygen_handles.push(handle);
    }

    // Wait for all keygen tasks
    let mut keygen_results = Vec::new();
    for handle in keygen_handles {
        match handle.await {
            Ok(result) => keygen_results.push(result),
            Err(e) => return Err(format!("Keygen task failed: {}", e)),
        }
    }

    // Abort the relay (channels are closed)
    relay_handle.abort();

    let keygen_duration = keygen_start.elapsed();
    println!(
        "[FROST] Keygen completed in {:.2}ms",
        keygen_duration.as_secs_f64() * 1000.0
    );

    // Verify all parties succeeded
    let key_shares: Vec<_> = keygen_results
        .iter()
        .filter_map(|r| {
            if r.success {
                r.key_share_data.clone()
            } else {
                None
            }
        })
        .collect();

    if key_shares.len() != num_parties as usize {
        return Err(format!(
            "Keygen failed: only {}/{} parties succeeded",
            key_shares.len(),
            num_parties
        ));
    }

    // Verify all parties have the same public key
    let public_keys: Vec<_> = keygen_results
        .iter()
        .filter_map(|r| r.public_key.clone())
        .collect();

    if public_keys.windows(2).any(|w| w[0] != w[1]) {
        return Err("Public keys don't match across parties".to_string());
    }

    println!("[FROST] Public key: {}", hex::encode(&public_keys[0]));

    // =========================================================================
    // SIGNING PHASE
    // =========================================================================
    println!("[FROST] Running signing...");
    let signing_start = Instant::now();

    // Create a message to sign
    let message_hash: [u8; 32] = {
        let mut hasher = sha2::Sha256::new();
        hasher.update(b"benchmark test message");
        let result = hasher.finalize();
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&result);
        arr
    };

    // For signing, we use threshold parties (0..threshold)
    let signing_parties: Vec<u16> = (0..threshold).collect();
    let signing_session_id = random_session_id("frost-sign");

    // Create message relay for signing
    let (sign_relay, sign_channels) = FrostSigningRelay::new(threshold, 10000);

    // Spawn the relay
    let sign_relay_handle = tokio::spawn(sign_relay.run());

    // Spawn signing tasks for threshold parties
    let mut signing_handles = Vec::new();
    for (signing_idx, (out_tx, in_rx)) in sign_channels.into_iter().enumerate() {
        let session_id = signing_session_id.clone();
        let key_share_data = key_shares[signing_idx].clone();
        let parties = signing_parties.clone();

        let handle = tokio::spawn(async move {
            run_frost_signing_with_benchmark(
                signing_idx as u16, // signing index (0..threshold)
                &parties,           // list of original party indices
                &session_id,
                &message_hash,
                &key_share_data,
                in_rx,
                out_tx,
                true, // enable benchmarking
            )
            .await
        });
        signing_handles.push(handle);
    }

    // Wait for all signing tasks
    let mut signing_results = Vec::new();
    for handle in signing_handles {
        match handle.await {
            Ok(result) => signing_results.push(result),
            Err(e) => return Err(format!("Signing task failed: {}", e)),
        }
    }

    sign_relay_handle.abort();

    let signing_duration = signing_start.elapsed();
    println!(
        "[FROST] Signing completed in {:.2}ms",
        signing_duration.as_secs_f64() * 1000.0
    );

    // Verify signing succeeded
    let signatures: Vec<_> = signing_results
        .iter()
        .filter_map(|r| {
            if r.success {
                r.signature.as_ref().map(|s| s.to_bytes())
            } else {
                None
            }
        })
        .collect();

    if signatures.is_empty() {
        let errors: Vec<_> = signing_results
            .iter()
            .filter_map(|r| r.error.clone())
            .collect();
        return Err(format!("Signing failed: {:?}", errors));
    }

    // All parties should produce the same signature
    if signatures.windows(2).any(|w| w[0] != w[1]) {
        return Err("Signatures don't match across parties".to_string());
    }

    println!("[FROST] Signature: {}", hex::encode(&signatures[0]));

    // Collect benchmark reports
    let signing_reports: Vec<_> = signing_results
        .iter()
        .filter_map(|r| r.benchmark.clone())
        .collect();

    let total_duration = keygen_duration + signing_duration;

    Ok(ProtocolBenchmarkResult {
        protocol_name: "FROST (Schnorr/Taproot)".to_string(),
        num_parties,
        threshold,
        keygen_duration_ms: keygen_duration.as_secs_f64() * 1000.0,
        signing_duration_ms: signing_duration.as_secs_f64() * 1000.0,
        total_duration_ms: total_duration.as_secs_f64() * 1000.0,
        keygen_reports: vec![], // FROST keygen doesn't have detailed benchmarks yet
        signing_reports,
    })
}

/// Helper function to generate CGGMP24 keys and aux info.
async fn generate_cggmp24_keys_and_aux(
    num_parties: u16,
    threshold: u16,
    cache_dir: Option<&std::path::Path>,
) -> Result<(Vec<Vec<u8>>, Vec<Vec<u8>>, Vec<u8>, std::time::Duration), String> {
    use crate::cggmp24::keygen::run_keygen;
    use crate::cggmp24::primes::{generate_primes, load_primes, save_primes};
    use crate::cggmp24::runner::run_aux_info_gen;

    let total_start = Instant::now();

    // =========================================================================
    // PRIMES LOADING/GENERATION
    // =========================================================================
    let primes_start = Instant::now();
    let mut all_primes_data = Vec::new();

    for party_idx in 0..num_parties {
        let primes_data = if let Some(dir) = cache_dir {
            let primes_path = dir.join(format!("primes-party-{}.json", party_idx));

            if primes_path.exists() {
                println!("[CGGMP24] Loading cached primes for party {}...", party_idx);
                match load_primes(&primes_path) {
                    Ok(stored) => stored.primes_data,
                    Err(e) => {
                        println!("[CGGMP24] Cache load failed: {}, generating...", e);
                        let stored = generate_primes(party_idx).map_err(|e| {
                            format!("Failed to generate primes for party {}: {}", party_idx, e)
                        })?;
                        let _ = save_primes(&primes_path, &stored);
                        stored.primes_data
                    }
                }
            } else {
                println!(
                    "[CGGMP24] Generating primes for party {} (will be cached)...",
                    party_idx
                );
                let stored = generate_primes(party_idx).map_err(|e| {
                    format!("Failed to generate primes for party {}: {}", party_idx, e)
                })?;
                if let Some(parent) = primes_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = save_primes(&primes_path, &stored);
                stored.primes_data
            }
        } else {
            println!(
                "[CGGMP24] Generating primes for party {} (no cache)...",
                party_idx
            );
            let stored = generate_primes(party_idx)
                .map_err(|e| format!("Failed to generate primes for party {}: {}", party_idx, e))?;
            stored.primes_data
        };

        all_primes_data.push(primes_data);
    }

    println!(
        "[CGGMP24] Primes ready in {:.2}s",
        primes_start.elapsed().as_secs_f64()
    );

    // =========================================================================
    // KEYGEN PHASE
    // =========================================================================
    println!("[CGGMP24] Running key generation...");
    let keygen_start = Instant::now();

    let keygen_session_id = random_session_id("cggmp-keygen");
    let (relay, party_channels) = Cggmp24Relay::new(num_parties, 10000);
    let relay_handle = tokio::spawn(relay.run());

    let mut keygen_handles = Vec::new();
    for (party_idx, (out_tx, in_rx)) in party_channels.into_iter().enumerate() {
        let session_id = keygen_session_id.clone();
        let handle = tokio::spawn(async move {
            run_keygen(
                party_idx as u16,
                num_parties,
                threshold,
                &session_id,
                in_rx,
                out_tx,
            )
            .await
        });
        keygen_handles.push(handle);
    }

    let mut keygen_results = Vec::new();
    for handle in keygen_handles {
        match handle.await {
            Ok(result) => keygen_results.push(result),
            Err(e) => return Err(format!("Keygen task failed: {}", e)),
        }
    }
    relay_handle.abort();

    println!(
        "[CGGMP24] Keygen completed in {:.2}ms",
        keygen_start.elapsed().as_secs_f64() * 1000.0
    );

    let key_shares: Vec<_> = keygen_results
        .iter()
        .filter_map(|r| {
            if r.success {
                r.key_share_data.clone()
            } else {
                None
            }
        })
        .collect();

    if key_shares.len() != num_parties as usize {
        let errors: Vec<_> = keygen_results
            .iter()
            .filter_map(|r| r.error.clone())
            .collect();
        return Err(format!("Keygen failed: {:?}", errors));
    }

    let public_keys: Vec<_> = keygen_results
        .iter()
        .filter_map(|r| r.public_key.clone())
        .collect();
    if public_keys.windows(2).any(|w| w[0] != w[1]) {
        return Err("Public keys don't match across parties".to_string());
    }

    println!("[CGGMP24] Public key: {}", hex::encode(&public_keys[0]));

    // =========================================================================
    // AUX INFO GENERATION PHASE
    // =========================================================================
    println!("[CGGMP24] Running aux info generation...");
    let aux_start = Instant::now();

    let aux_session_id = random_session_id("cggmp-aux");
    let (aux_relay, aux_channels) = Cggmp24Relay::new(num_parties, 10000);
    let aux_relay_handle = tokio::spawn(aux_relay.run());

    let mut aux_handles = Vec::new();
    for (party_idx, (out_tx, in_rx)) in aux_channels.into_iter().enumerate() {
        let session_id = aux_session_id.clone();
        let primes_data = all_primes_data[party_idx].clone();
        let handle = tokio::spawn(async move {
            run_aux_info_gen(
                party_idx as u16,
                num_parties,
                &session_id,
                &primes_data,
                in_rx,
                out_tx,
            )
            .await
        });
        aux_handles.push(handle);
    }

    let mut aux_results = Vec::new();
    for handle in aux_handles {
        match handle.await {
            Ok(result) => aux_results.push(result),
            Err(e) => return Err(format!("Aux info task failed: {}", e)),
        }
    }
    aux_relay_handle.abort();

    println!(
        "[CGGMP24] Aux info completed in {:.2}ms",
        aux_start.elapsed().as_secs_f64() * 1000.0
    );

    let aux_infos: Vec<_> = aux_results
        .iter()
        .filter_map(|r| {
            if r.success {
                r.aux_info_data.clone()
            } else {
                None
            }
        })
        .collect();

    if aux_infos.len() != num_parties as usize {
        let errors: Vec<_> = aux_results.iter().filter_map(|r| r.error.clone()).collect();
        return Err(format!("Aux info failed: {:?}", errors));
    }

    Ok((
        key_shares,
        aux_infos,
        public_keys[0].clone(),
        total_start.elapsed(),
    ))
}

/// Run CGGMP24 keygen + aux_info + signing benchmark.
pub async fn run_cggmp24_benchmark(
    num_parties: u16,
    threshold: u16,
) -> Result<ProtocolBenchmarkResult, String> {
    run_cggmp24_benchmark_with_cache(num_parties, threshold, None).await
}

/// Cached CGGMP24 data for a specific configuration.
#[derive(serde::Serialize, serde::Deserialize)]
struct Cggmp24Cache {
    key_shares: Vec<Vec<u8>>,
    aux_infos: Vec<Vec<u8>>,
    public_key: Vec<u8>,
}

/// Run CGGMP24 benchmark with optional cache directory.
///
/// Caches primes, key shares, and aux_info for fast subsequent runs.
pub async fn run_cggmp24_benchmark_with_cache(
    num_parties: u16,
    threshold: u16,
    cache_dir: Option<&std::path::Path>,
) -> Result<ProtocolBenchmarkResult, String> {
    use crate::cggmp24::signing::run_signing_with_benchmark;

    println!(
        "\n[CGGMP24] Starting benchmark: {}-of-{}",
        threshold, num_parties
    );

    // =========================================================================
    // CHECK FOR CACHED KEY SHARES + AUX INFO
    // =========================================================================
    let config_key = format!("{}-of-{}", threshold, num_parties);
    let cache_path = cache_dir.map(|d| d.join(format!("cggmp24-cache-{}.json", config_key)));

    let (key_shares, aux_infos, keygen_duration): (
        Vec<Vec<u8>>,
        Vec<Vec<u8>>,
        std::time::Duration,
    ) = if let Some(ref path) = cache_path {
        if path.exists() {
            // Load from cache
            println!("[CGGMP24] Loading cached key shares and aux info...");
            let load_start = Instant::now();

            let cache_data = std::fs::read_to_string(path)
                .map_err(|e| format!("Failed to read cache: {}", e))?;
            let cache: Cggmp24Cache = serde_json::from_str(&cache_data)
                .map_err(|e| format!("Failed to parse cache: {}", e))?;

            println!("[CGGMP24] Public key: {}", hex::encode(&cache.public_key));
            println!(
                "[CGGMP24] Loaded from cache in {:.2}ms",
                load_start.elapsed().as_secs_f64() * 1000.0
            );

            (cache.key_shares, cache.aux_infos, load_start.elapsed())
        } else {
            // Generate and cache
            let (ks, aux, pk, dur) =
                generate_cggmp24_keys_and_aux(num_parties, threshold, cache_dir).await?;

            // Save to cache
            let cache = Cggmp24Cache {
                key_shares: ks.clone(),
                aux_infos: aux.clone(),
                public_key: pk,
            };
            if let Ok(json) = serde_json::to_string_pretty(&cache) {
                let _ = std::fs::write(path, json);
                println!("[CGGMP24] Cached key shares and aux info for future runs");
            }

            (ks, aux, dur)
        }
    } else {
        // No cache
        let (ks, aux, _pk, dur) =
            generate_cggmp24_keys_and_aux(num_parties, threshold, cache_dir).await?;
        (ks, aux, dur)
    };

    // =========================================================================
    // SIGNING PHASE
    // =========================================================================
    println!("[CGGMP24] Running signing...");
    let signing_start = Instant::now();

    // Create a message to sign
    let message_hash: [u8; 32] = {
        let mut hasher = sha2::Sha256::new();
        hasher.update(b"benchmark test message");
        let result = hasher.finalize();
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&result);
        arr
    };

    // For signing, we use threshold parties (0..threshold)
    let signing_parties: Vec<u16> = (0..threshold).collect();
    let signing_session_id = random_session_id("cggmp-sign");

    // Create message relay for signing
    let (sign_relay, sign_channels) = Cggmp24Relay::new(threshold, 10000);

    // Spawn the relay
    let sign_relay_handle = tokio::spawn(sign_relay.run());

    // Spawn signing tasks for threshold parties
    let mut signing_handles = Vec::new();
    for (signing_idx, (out_tx, in_rx)) in sign_channels.into_iter().enumerate() {
        let session_id = signing_session_id.clone();
        let key_share_data = key_shares[signing_idx].clone();
        let aux_info_data = aux_infos[signing_idx].clone();
        let parties = signing_parties.clone();

        let handle = tokio::spawn(async move {
            run_signing_with_benchmark(
                signing_idx as u16,
                &parties,
                &session_id,
                &message_hash,
                &key_share_data,
                &aux_info_data,
                in_rx,
                out_tx,
                true, // enable benchmarking
            )
            .await
        });
        signing_handles.push(handle);
    }

    // Wait for all signing tasks
    let mut signing_results = Vec::new();
    for handle in signing_handles {
        match handle.await {
            Ok(result) => signing_results.push(result),
            Err(e) => return Err(format!("Signing task failed: {}", e)),
        }
    }

    sign_relay_handle.abort();

    let signing_duration = signing_start.elapsed();
    println!(
        "[CGGMP24] Signing completed in {:.2}ms",
        signing_duration.as_secs_f64() * 1000.0
    );

    // Verify signing succeeded
    let signatures: Vec<_> = signing_results
        .iter()
        .filter_map(|r| {
            if r.success {
                r.signature.as_ref().map(|s| s.to_compact())
            } else {
                None
            }
        })
        .collect();

    if signatures.is_empty() {
        let errors: Vec<_> = signing_results
            .iter()
            .filter_map(|r| r.error.clone())
            .collect();
        return Err(format!("Signing failed: {:?}", errors));
    }

    // All parties should produce the same signature
    if signatures.windows(2).any(|w| w[0] != w[1]) {
        return Err("Signatures don't match across parties".to_string());
    }

    println!("[CGGMP24] Signature: {}", hex::encode(signatures[0]));

    // Collect benchmark reports
    let signing_reports: Vec<_> = signing_results
        .iter()
        .filter_map(|r| r.benchmark.clone())
        .collect();

    // keygen_duration includes primes + keygen + aux_info (or cache load time)
    let total_duration = keygen_duration + signing_duration;

    Ok(ProtocolBenchmarkResult {
        protocol_name: "CGGMP24 (ECDSA)".to_string(),
        num_parties,
        threshold,
        keygen_duration_ms: keygen_duration.as_secs_f64() * 1000.0,
        signing_duration_ms: signing_duration.as_secs_f64() * 1000.0,
        total_duration_ms: total_duration.as_secs_f64() * 1000.0,
        keygen_reports: vec![],
        signing_reports,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Benchmark FROST protocol with 2-of-3 threshold.
    ///
    /// Run with: cargo test --package protocols benchmark_frost_2of3 --release -- --nocapture
    #[tokio::test]
    async fn benchmark_frost_2of3() {
        let result = run_frost_benchmark(3, 2).await;
        match result {
            Ok(r) => r.print_summary(),
            Err(e) => panic!("FROST benchmark failed: {}", e),
        }
    }

    /// Benchmark FROST protocol with 3-of-5 threshold.
    ///
    /// Run with: cargo test --package protocols benchmark_frost_3of5 --release -- --nocapture
    #[tokio::test]
    async fn benchmark_frost_3of5() {
        let result = run_frost_benchmark(5, 3).await;
        match result {
            Ok(r) => r.print_summary(),
            Err(e) => panic!("FROST benchmark failed: {}", e),
        }
    }

    /// Benchmark CGGMP24 protocol with 2-of-3 threshold.
    ///
    /// Run with: cargo test --package protocols benchmark_cggmp24_2of3 --release -- --nocapture
    ///
    /// First run is slow (~30-60s per party for prime generation).
    /// Subsequent runs use cached primes and are fast.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn benchmark_cggmp24_2of3() {
        // Use a cache directory for primes to speed up subsequent runs
        // Path is relative to crates/protocols/ where cargo test runs
        let cache_dir = std::path::PathBuf::from("src/bench/cache");
        std::fs::create_dir_all(&cache_dir).expect("Failed to create cache dir");

        let result = run_cggmp24_benchmark_with_cache(3, 2, Some(&cache_dir)).await;
        match result {
            Ok(r) => r.print_summary(),
            Err(e) => panic!("CGGMP24 benchmark failed: {}", e),
        }
    }

    /// Benchmark CGGMP24 protocol with 3-of-4 threshold.
    ///
    /// Run with: cargo test --package protocols benchmark_cggmp24_3of4 --release -- --nocapture
    ///
    /// First run is slow (~30-60s per party for prime generation).
    /// Subsequent runs use cached primes and are fast.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn benchmark_cggmp24_3of4() {
        // Use a cache directory for primes to speed up subsequent runs
        let cache_dir = std::path::PathBuf::from("src/bench/cache");
        std::fs::create_dir_all(&cache_dir).expect("Failed to create cache dir");

        let result = run_cggmp24_benchmark_with_cache(4, 3, Some(&cache_dir)).await;
        match result {
            Ok(r) => r.print_summary(),
            Err(e) => panic!("CGGMP24 benchmark failed: {}", e),
        }
    }

    /// Quick FROST benchmark for CI.
    #[tokio::test]
    async fn benchmark_frost_quick() {
        println!("\n========== FROST PROTOCOL BENCHMARK ==========\n");

        let configs = vec![
            (2, 2), // 2-of-2
            (3, 2), // 2-of-3
        ];

        for (n, t) in configs {
            match run_frost_benchmark(n, t).await {
                Ok(r) => {
                    println!(
                        "{}-of-{}: keygen={:.0}ms, sign={:.0}ms, total={:.0}ms",
                        t, n, r.keygen_duration_ms, r.signing_duration_ms, r.total_duration_ms
                    );
                }
                Err(e) => println!("{}-of-{}: FAILED - {}", t, n, e),
            }
        }

        println!();
    }
}
