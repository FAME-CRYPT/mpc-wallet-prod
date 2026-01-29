// Pre-generate primes for a specific party
// Usage: PARTY_INDEX=0 cargo run --bin pre-generate-primes

use std::env;
use std::path::PathBuf;
use protocols::cggmp24::primes;

fn main() {
    env_logger::init();

    let party_index: u16 = env::var("PARTY_INDEX")
        .expect("PARTY_INDEX environment variable must be set")
        .parse()
        .expect("PARTY_INDEX must be a valid u16");

    println!("========================================");
    println!("  PRE-GENERATING PRIMES FOR PARTY {}", party_index);
    println!("========================================");
    println!();
    println!("This may take 30-120 seconds...");
    println!();

    let path = PathBuf::from(format!("data/primes-party-{}.json", party_index));

    // Generate primes
    match primes::generate_primes(party_index) {
        Ok(stored) => {
            println!("✅ Prime generation completed!");
            println!("   Primes size: {} bytes", stored.primes_data.len());

            // Save to disk
            match primes::save_primes(&path, &stored) {
                Ok(()) => {
                    println!("✅ Primes saved to {:?}", path);
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("❌ Failed to save primes: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("❌ Prime generation failed: {}", e);
            std::process::exit(1);
        }
    }
}
