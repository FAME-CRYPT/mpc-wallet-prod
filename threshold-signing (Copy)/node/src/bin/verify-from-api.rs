// Verify signatures retrieved from the API
//
// Usage:
//   cargo run --bin verify-from-api -- \
//     --message "Hello, world!" \
//     --request-id abc123

use anyhow::Result;
use cggmp24::signing::DataToSign;
use cggmp24::supported_curves::Secp256k1;
use cggmp24::Signature;
use sha2::Sha256;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "verify-from-api", about = "Verify signatures from the API Gateway")]
struct Args {
    /// The message that was signed
    #[structopt(short, long)]
    message: String,

    /// The request ID (to fetch signature from API)
    #[structopt(short, long)]
    request_id: String,

    /// API Gateway URL
    #[structopt(long, default_value = "http://localhost:8000")]
    api_url: String,
}

fn main() -> Result<()> {
    let args = Args::from_args();

    // Fetch public key from API
    println!("Fetching public key from API...");
    let public_key_url = format!("{}/publickey", args.api_url);
    let public_key_response = reqwest::blocking::get(&public_key_url)?;
    let public_key_json: serde_json::Value = public_key_response.json()?;
    let public_key_hex = public_key_json["public_key"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No public key found"))?;
    
    println!("Public key: {}", public_key_hex);

    // Parse the public key from hex - this is where we need proper deserialization
    use generic_ec::{NonZero, Point};
    let public_key: NonZero<Point<Secp256k1>> = 
        serde_json::from_str(&format!("\"{}\"", public_key_hex))
        .map_err(|e| anyhow::anyhow!("Failed to parse public key: {}. The compressed format may not be directly deserializable.", e))?;

    // Fetch signature from API
    println!("Fetching signature from API...");
    let status_url = format!("{}/status/{}", args.api_url, args.request_id);
    let status_response = reqwest::blocking::get(&status_url)?;
    let status_json: serde_json::Value = status_response.json()?;
    
    let status = status_json["status"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No status found"))?;
    
    if status != "completed" {
        anyhow::bail!("Signature not ready yet. Status: {}", status);
    }

    let signature_str = status_json["signature"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No signature found"))?;
    
    let signature: Signature<Secp256k1> = serde_json::from_str(signature_str)?;
    println!("Signature retrieved successfully");

    // Hash the message
    println!("Hashing message: {:?}", args.message);
    let message_bytes = args.message.as_bytes();
    let data_to_sign = DataToSign::digest::<Sha256>(message_bytes);

    // Verify the signature
    println!("Verifying signature...");
    match signature.verify(&public_key, &data_to_sign) {
        Ok(_) => {
            println!("\n✓ Signature is VALID");
            println!("  Message: {}", args.message);
            println!("  Request ID: {}", args.request_id);
            Ok(())
        }
        Err(e) => {
            println!("\n✗ Signature is INVALID");
            println!("  Error: {:?}", e);
            anyhow::bail!("Signature verification failed")
        }
    }
}
