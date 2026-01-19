use anyhow::Result;
use std::fs;

fn main() -> Result<()> {
    let key_share_json = fs::read_to_string("/tmp/key_share_node-1.json")?;
    let key_share: serde_json::Value = serde_json::from_str(&key_share_json)?;
    
    // Extract the shared_public_key and save it in the format expected by verify-signature
    let public_key = &key_share["core"]["shared_public_key"];
    let public_key_json = serde_json::to_string_pretty(public_key)?;
    
    fs::write("/tmp/public_key_exported.json", public_key_json)?;
    println!("Public key exported to /tmp/public_key_exported.json");
    Ok(())
}
