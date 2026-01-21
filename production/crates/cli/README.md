# Threshold Wallet CLI

Production-ready command-line interface for the MPC threshold wallet system.

## Features

- **Wallet Operations**: Check balance, get receiving address
- **Transaction Management**: Send Bitcoin, check status, list transactions
- **Cluster Monitoring**: View cluster health and node status
- **DKG Support**: Initialize distributed key generation (CGGMP24/FROST)
- **Presignature Generation**: Pre-compute signatures for fast signing
- **Flexible Output**: Table or JSON format with optional colorization
- **Configuration Management**: Store settings in `~/.threshold-wallet/config.toml`

## Installation

### From Source

```bash
cd production
cargo build --release --package threshold-cli
```

The binary will be available at: `target/release/threshold-wallet`

### Add to PATH

```bash
# Linux/macOS
sudo cp target/release/threshold-wallet /usr/local/bin/

# Or add to your PATH
export PATH="$PATH:$(pwd)/target/release"
```

## Configuration

On first run, a default configuration file is created at `~/.threshold-wallet/config.toml`:

```toml
api_endpoint = "http://localhost:8080"
timeout_secs = 30
output_format = "table"
colored = true
```

### Configure the CLI

```bash
# View current configuration
threshold-wallet config show

# Set API endpoint
threshold-wallet config set-endpoint http://api.example.com:8080

# Set node ID
threshold-wallet config set-node-id 1

# Set output format
threshold-wallet config set-format json
```

### Environment Variables

You can also override settings via environment variables:

```bash
export THRESHOLD_API_ENDPOINT=http://localhost:8080
```

## Usage

### Global Flags

```bash
--api-endpoint <URL>    # Override API endpoint
--output <FORMAT>       # Output format: table, json
--json                  # Enable JSON output (shorthand)
--no-color              # Disable colored output
```

### Wallet Commands

#### Get Balance

```bash
threshold-wallet wallet balance

# JSON output
threshold-wallet wallet balance --json
```

#### Get Receiving Address

```bash
threshold-wallet wallet address
```

### Transaction Commands

#### Send Bitcoin

```bash
# Send 50,000 sats to an address
threshold-wallet send \
  --to tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx \
  --amount 50000

# Send with OP_RETURN metadata
threshold-wallet send \
  --to tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx \
  --amount 50000 \
  --metadata "Hello Bitcoin!"

# Monitor transaction progress
threshold-wallet send --to <address> --amount 50000
# Will prompt to monitor progress automatically
```

#### Check Transaction Status

```bash
threshold-wallet tx status <txid>
```

#### List All Transactions

```bash
threshold-wallet tx list

# JSON output
threshold-wallet tx list --json
```

### Cluster Commands

#### Get Cluster Status

```bash
threshold-wallet cluster status
```

#### List Cluster Nodes

```bash
threshold-wallet cluster nodes
```

### DKG Commands

**Note**: DKG endpoints are not yet implemented in the API. These commands show what will be available.

#### Start DKG Ceremony

```bash
# CGGMP24 (SegWit/ECDSA) with 2-of-3 threshold
threshold-wallet dkg start \
  --protocol cggmp24 \
  --threshold 2 \
  --total 3

# FROST (Taproot/Schnorr) with 3-of-5 threshold
threshold-wallet dkg start \
  --protocol frost \
  --threshold 3 \
  --total 5
```

#### Check DKG Status

```bash
threshold-wallet dkg status --session-id <uuid>
```

### Presignature Commands

**Note**: Presignature endpoints are not yet implemented in the API. These commands show what will be available.

#### Generate Presignatures

```bash
# Generate 10 presignatures
threshold-wallet presig generate --count 10

# Generate 50 presignatures
threshold-wallet presig generate --count 50
```

#### List Presignatures

```bash
threshold-wallet presig list
```

#### Get Presignature Pool Status

```bash
threshold-wallet presig status
```

## Examples

### Complete Workflow

```bash
# 1. Check wallet balance
threshold-wallet wallet balance

# 2. Get receiving address
threshold-wallet wallet address

# 3. Send Bitcoin
threshold-wallet send \
  --to tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx \
  --amount 100000

# 4. Monitor transaction
threshold-wallet tx status <txid>

# 5. List all transactions
threshold-wallet tx list

# 6. Check cluster health
threshold-wallet cluster status
```

### JSON Mode for Scripting

```bash
# Get balance in JSON
BALANCE=$(threshold-wallet wallet balance --json | jq -r '.total')
echo "Balance: $BALANCE sats"

# List transactions and filter
threshold-wallet tx list --json | jq '.[] | select(.state == "confirmed")'

# Check cluster status
HEALTHY=$(threshold-wallet cluster status --json | jq -r '.healthy_nodes')
echo "Healthy nodes: $HEALTHY"
```

### Configuration Management

```bash
# Show current config
threshold-wallet config show

# Update endpoint for production
threshold-wallet config set-endpoint https://wallet.production.com:8080

# Switch to JSON output by default
threshold-wallet config set-format json

# Disable colors for CI/CD
threshold-wallet --no-color cluster status
```

## Output Formats

### Table Format (Default)

```
Wallet Balance
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  Confirmed: 1,500,000 sats
  Confirmed (BTC): 0.01500000 BTC
  Total: 1,500,000 sats
  Total (BTC): 0.01500000 BTC
```

### JSON Format

```json
{
  "confirmed": 1500000,
  "unconfirmed": 0,
  "total": 1500000
}
```

## Transaction States

The CLI color-codes transaction states:

- **Green**: `confirmed` - Transaction confirmed on-chain
- **Cyan**: `signed`, `submitted`, `broadcasting` - Transaction being broadcast
- **Blue**: `approved`, `threshold_reached` - Consensus reached
- **Yellow**: `pending`, `voting`, `collecting` - Waiting for votes
- **Magenta**: `signing` - Threshold signing in progress
- **Red**: `failed`, `rejected`, `aborted_byzantine` - Transaction failed

## Error Handling

The CLI provides user-friendly error messages:

```bash
# Invalid address
$ threshold-wallet send --to invalid --amount 1000
✗ Error: Bad request: Invalid recipient address

# API server down
$ threshold-wallet wallet balance
✗ Error: Failed to connect to API server at http://localhost:8080

# Transaction not found
$ threshold-wallet tx status nonexistent
✗ Error: Resource not found: Transaction not found
```

## Development

### Build

```bash
cargo build --package threshold-cli
```

### Run Tests

```bash
cargo test --package threshold-cli
```

### Run with Debug Logging

```bash
RUST_LOG=debug threshold-wallet wallet balance
```

## Architecture

```
production/crates/cli/
├── Cargo.toml          # Dependencies and binary configuration
└── src/
    ├── main.rs         # Entry point, command routing
    ├── commands/       # Command implementations
    │   ├── mod.rs
    │   ├── wallet.rs   # Wallet operations
    │   ├── send.rs     # Send transactions
    │   ├── tx.rs       # Transaction status
    │   ├── cluster.rs  # Cluster monitoring
    │   ├── dkg.rs      # DKG operations
    │   └── presig.rs   # Presignature generation
    ├── client.rs       # REST API client wrapper
    ├── config.rs       # Configuration management
    └── output.rs       # Output formatting
```

## API Integration

The CLI integrates with these REST API endpoints:

### Wallet Endpoints
- `GET /api/v1/wallet/balance` - Get wallet balance
- `GET /api/v1/wallet/address` - Get receiving address

### Transaction Endpoints
- `POST /api/v1/transactions` - Create transaction
- `GET /api/v1/transactions/:txid` - Get transaction status
- `GET /api/v1/transactions` - List transactions

### Cluster Endpoints
- `GET /api/v1/cluster/status` - Get cluster status
- `GET /api/v1/cluster/nodes` - List nodes

### Future Endpoints (Not Yet Implemented)
- `POST /api/v1/dkg/start` - Start DKG ceremony
- `GET /api/v1/dkg/status/:session_id` - Get DKG status
- `POST /api/v1/presig/generate` - Generate presignatures
- `GET /api/v1/presig/list` - List presignatures
- `GET /api/v1/presig/status` - Get presignature pool status

## Dependencies

- **clap** (v4) - Command-line argument parsing with derive macros
- **reqwest** - HTTP client for REST API calls
- **serde/serde_json** - Serialization for JSON output
- **tokio** - Async runtime
- **colored** - Colorized terminal output
- **tabled** - Table formatting
- **indicatif** - Progress bars and spinners
- **dialoguer** - Interactive prompts
- **dirs** - Cross-platform config directory detection
- **toml** - TOML configuration parsing

## Troubleshooting

### API Connection Issues

```bash
# Check if API server is running
curl http://localhost:8080/health

# Override API endpoint
threshold-wallet --api-endpoint http://localhost:8080 wallet balance

# Check configuration
threshold-wallet config show
```

### Permission Issues

```bash
# Ensure config directory is writable
ls -la ~/.threshold-wallet/

# Check file permissions
chmod 644 ~/.threshold-wallet/config.toml
```

## License

MIT
