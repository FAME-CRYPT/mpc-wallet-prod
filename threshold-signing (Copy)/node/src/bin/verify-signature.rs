// Standalone tool to verify ECDSA signatures produced by threshold signing
//
// Usage:
//   cargo run --bin verify-signature -- \
//     --public-key-hex 03de333b... \
//     --message "Hello, world!" \
//     --signature signature.json

use anyhow::Result;
use cggmp24::signing::DataToSign;
use cggmp24::supported_curves::Secp256k1;
use cggmp24::Signature;
use generic_ec::{NonZero, Point};
use sha2::Sha256;
use std::fs;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "verify-signature", about = "Verify threshold ECDSA signatures")]
struct Args {
    /// Public key as hex string (compressed format: 03... or 02...)
    #[structopt(long)]
    public_key_hex: Option<String>,

    /// Path to the public key JSON file (alternative to --public-key-hex)
    #[structopt(short, long, parse(from_os_str))]
    public_key: Option<PathBuf>,

    /// The message that was signed
    #[structopt(short, long)]
    message: String,

    /// Path to the signature JSON file
    #[structopt(short, long, parse(from_os_str))]
    signature: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::from_args();

    // Load the public key - either from hex or from file
    println!("Loading public key...");
    let public_key: NonZero<Point<Secp256k1>> = if let Some(hex) = args.public_key_hex {
        // Parse from hex string (compressed format)
        serde_json::from_str(&format!("\"{}\"", hex))?
    } else if let Some(path) = args.public_key {
        // Load from file
        let public_key_json = fs::read_to_string(&path)?;
        serde_json::from_str(&public_key_json)?
    } else {
        anyhow::bail!("Must provide either --public-key-hex or --public-key");
    };
    println!("Public key loaded successfully");

    // Load the signature
    println!("Loading signature from {:?}...", args.signature);
    let signature_json = fs::read_to_string(&args.signature)?;
    let signature: Signature<Secp256k1> = serde_json::from_str(&signature_json)?;
    println!("Signature loaded successfully");

    // Hash the message the same way it was hashed during signing
    println!("Hashing message: {:?}", args.message);
    let message_bytes = args.message.as_bytes();
    let data_to_sign = DataToSign::digest::<Sha256>(message_bytes);

    // Verify the signature
    println!("Verifying signature...");
    match signature.verify(&public_key, &data_to_sign) {
        Ok(_) => {
            println!("✓ Signature is VALID");
            println!("  Message: {}", args.message);
            Ok(())
        }
        Err(e) => {
            println!("✗ Signature is INVALID");
            println!("  Error: {:?}", e);
            anyhow::bail!("Signature verification failed")
        }
    }
}
