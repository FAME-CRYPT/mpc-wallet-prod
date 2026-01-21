# threshold-bitcoin

Bitcoin integration crate for the MPC wallet system. Provides blockchain interaction, transaction building, and OP_RETURN metadata embedding support.

## Features

- **Bitcoin Blockchain Client**: Esplora-compatible API client for interacting with Bitcoin nodes
- **Transaction Building**: Flexible transaction builder with UTXO selection and fee calculation
- **OP_RETURN Support**: Embed up to 80 bytes of metadata in Bitcoin transactions
- **Multi-Signature Support**: Compatible with both SegWit (P2WPKH/ECDSA) and Taproot (P2TR/Schnorr)
- **Network Support**: Mainnet, Testnet, and Regtest

## Architecture

### Core Components

1. **BitcoinClient** (`src/client.rs`)
   - Fetches address information and balances
   - Retrieves UTXOs for transaction building
   - Broadcasts signed transactions
   - Checks transaction confirmations
   - Estimates network fees

2. **TransactionBuilder** (`src/tx_builder.rs`)
   - Creates unsigned Bitcoin transactions
   - Handles UTXO selection (greedy algorithm)
   - Calculates fees and manages change outputs
   - Supports OP_RETURN outputs for metadata embedding
   - Computes sighashes for MPC signing

3. **Types** (`src/types.rs`)
   - Common data structures (UTXOs, transactions, addresses)
   - Request/response types
   - Fee estimation structures

## Integration Flow

```
┌─────────────────┐
│  1. UTXO Fetch  │  BitcoinClient::get_utxos()
└────────┬────────┘
         │
         ▼
┌─────────────────────────────────┐
│  2. Transaction Building        │  TransactionBuilder
│     - Add outputs               │    .add_output()
│     - Add OP_RETURN (optional)  │    .add_op_return()
│     - Build unsigned TX         │    .build_p2wpkh() / .build_p2tr()
└────────┬────────────────────────┘
         │
         ▼
┌─────────────────────────────────┐
│  3. MPC Signing                 │  CGGMP24 (SegWit) or FROST (Taproot)
│     - Sign sighashes            │  (handled by protocols crate)
└────────┬────────────────────────┘
         │
         ▼
┌─────────────────────────────────┐
│  4. Transaction Finalization    │  finalize_p2wpkh_transaction()
│     - Combine signatures        │  or finalize_taproot_transaction()
└────────┬────────────────────────┘
         │
         ▼
┌─────────────────────────────────┐
│  5. Broadcasting                │  BitcoinClient::broadcast_tx()
└────────┬────────────────────────┘
         │
         ▼
┌─────────────────────────────────┐
│  6. Confirmation Checking       │  BitcoinClient::get_tx_confirmation()
└─────────────────────────────────┘
```

## Usage Examples

### Basic Transaction

```rust
use threshold_bitcoin::{BitcoinClient, BitcoinNetwork, TransactionBuilder};

async fn send_bitcoin() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Create client
    let client = BitcoinClient::new(BitcoinNetwork::Testnet)?;

    // 2. Fetch UTXOs
    let utxos = client.get_utxos("tb1q...sender").await?;

    // 3. Get fee estimate
    let fees = client.get_fee_estimates().await?;
    let fee_rate = fees.recommended();

    // 4. Build unsigned transaction
    let unsigned_tx = TransactionBuilder::new(
        utxos,
        "tb1q...change".to_string(),
        sender_script_pubkey,
        fee_rate,
    )
    .add_output("tb1q...recipient".to_string(), 50_000)
    .build_p2wpkh()?;

    // 5. Sign with MPC (handled by protocols crate)
    // let signatures = mpc_sign(&unsigned_tx.sighashes).await?;

    // 6. Finalize transaction
    // let signed_tx = finalize_p2wpkh_transaction(
    //     &unsigned_tx.unsigned_tx_hex,
    //     &signatures,
    //     &public_keys,
    // )?;

    // 7. Broadcast
    // let txid = client.broadcast_tx(&signed_tx).await?;

    Ok(())
}
```

### Transaction with OP_RETURN Metadata

```rust
use threshold_bitcoin::{TransactionBuilder, MAX_OP_RETURN_SIZE};

async fn send_with_metadata() -> Result<(), Box<dyn std::error::Error>> {
    let metadata = b"Document hash: abc123...".to_vec();

    // Validate size
    assert!(metadata.len() <= MAX_OP_RETURN_SIZE);

    // Build transaction with OP_RETURN
    let unsigned_tx = TransactionBuilder::new(utxos, change_addr, script_pubkey, fee_rate)
        .add_output(recipient_addr, amount)
        .add_op_return(metadata)?  // Embeds metadata in OP_RETURN output
        .build_p2wpkh()?;

    // The transaction now has 3 outputs:
    // 1. Payment to recipient
    // 2. OP_RETURN with metadata (0 sats, unspendable)
    // 3. Change back to sender (if needed)

    Ok(())
}
```

### Taproot Transaction

```rust
use threshold_bitcoin::TransactionBuilder;

async fn send_taproot() -> Result<(), Box<dyn std::error::Error>> {
    // Build Taproot (P2TR) transaction
    let unsigned_tx = TransactionBuilder::new(utxos, change_addr, script_pubkey, fee_rate)
        .add_output(recipient_addr, amount)
        .build_p2tr()?;  // Uses Schnorr signatures

    // MPC signing with FROST
    // let schnorr_signatures = frost_sign(&unsigned_tx.sighashes).await?;

    // Finalize with Schnorr signatures
    // let signed_tx = finalize_taproot_transaction(
    //     &unsigned_tx.unsigned_tx_hex,
    //     &schnorr_signatures,
    // )?;

    Ok(())
}
```

## OP_RETURN Specification

### Overview
OP_RETURN is a Bitcoin script opcode that marks a transaction output as provably unspendable. This allows arbitrary data to be embedded in the blockchain without bloating the UTXO set.

### Constraints
- **Maximum Size**: 80 bytes (Bitcoin consensus rule)
- **Cost**: Minimal - only transaction space, no UTXO set impact
- **Value**: Always 0 satoshis (unspendable)

### Use Cases
- Document timestamping
- Proof of existence
- Asset metadata
- Application-specific data (e.g., token transfers, NFT references)

### Example OP_RETURN Data

```rust
// Document hash
let data = sha256("Important Document").to_vec();

// Text message
let data = b"Hello, Bitcoin!".to_vec();

// JSON metadata
let metadata = serde_json::to_vec(&json!({
    "type": "asset_transfer",
    "asset_id": "ABC123",
    "amount": 100
}))?;

// Binary protocol data
let data = vec![0x01, 0x02, 0x03, ...];
```

## Transaction Size and Fee Calculation

### SegWit (P2WPKH)
- Base size: ~10 vB
- Per input: ~68 vB (includes witness data)
- Per output: ~31 vB
- OP_RETURN output: ~43 vB + data length

### Taproot (P2TR)
- Base size: ~10 vB
- Per input: ~58 vB (key-path spend with Schnorr signature)
- Per output: ~43 vB
- OP_RETURN output: ~43 vB + data length

### Fee Calculation
```
Total Fee = Fee Rate (sat/vB) × Transaction Size (vB)
```

The `TransactionBuilder` automatically:
1. Selects sufficient UTXOs
2. Calculates transaction size
3. Computes required fees
4. Creates change output (if amount > dust limit)

## Error Handling

The crate provides two main error types:

### BitcoinError
- `ApiRequest`: Network or HTTP errors
- `ApiError`: API returned an error response
- `ParseResponse`: Failed to parse API response
- `Broadcast`: Transaction broadcast failed
- `Configuration`: Invalid configuration

### TxBuilderError
- `InsufficientFunds`: Not enough UTXOs to cover amount + fees
- `InvalidAddress`: Malformed Bitcoin address
- `OpReturnTooLarge`: Metadata exceeds 80 bytes
- `SighashError`: Failed to compute sighash
- `NoUtxos`: No UTXOs available

## Testing

```bash
# Run all tests
cargo test -p threshold-bitcoin

# Run with output
cargo test -p threshold-bitcoin -- --nocapture

# Run specific test
cargo test -p threshold-bitcoin test_op_return_builder
```

## Integration with MPC Protocols

### SegWit (CGGMP24 ECDSA)
1. Build transaction with `build_p2wpkh()`
2. Extract `sighashes` from `UnsignedTransaction`
3. Sign each sighash using CGGMP24 protocol
4. Combine DER-encoded signatures with public keys
5. Finalize with `finalize_p2wpkh_transaction()`

### Taproot (FROST Schnorr)
1. Build transaction with `build_p2tr()`
2. Extract `sighashes` from `UnsignedTransaction`
3. Sign each sighash using FROST protocol
4. Combine 64-byte Schnorr signatures
5. Finalize with `finalize_taproot_transaction()`

## Network Configuration

### Testnet (Default)
```rust
let client = BitcoinClient::new(BitcoinNetwork::Testnet)?;
// Uses: https://blockstream.info/testnet/api
```

### Mainnet
```rust
let client = BitcoinClient::new(BitcoinNetwork::Mainnet)?;
// Uses: https://blockstream.info/api
```

### Regtest (Local Development)
```rust
// Regtest requires RPC, not supported by this client
// Use a different client implementation
```

### Custom Esplora Instance
```rust
let client = BitcoinClient::with_api_url(
    BitcoinNetwork::Testnet,
    "http://localhost:3000".to_string()
);
```

## Dependencies

- `bitcoin` (v0.32): Core Bitcoin types and primitives
- `reqwest`: HTTP client for Esplora API
- `serde`: Serialization/deserialization
- `thiserror`: Error handling
- `hex`: Hex encoding/decoding
- `tokio`: Async runtime

## Future Enhancements

- [ ] Support for custom fee estimation strategies
- [ ] Batch transaction building
- [ ] RPC client for Regtest support
- [ ] Replace-by-fee (RBF) support
- [ ] CPFP (Child-Pays-For-Parent) utilities
- [ ] Multi-input, multi-output optimizations
- [ ] Coin selection strategies (Branch and Bound, etc.)
- [ ] Partial transaction signing (PSBT)

## License

MIT
