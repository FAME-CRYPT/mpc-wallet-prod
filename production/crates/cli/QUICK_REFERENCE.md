# Threshold Wallet CLI - Quick Reference

## Installation
```bash
cargo build --release --package threshold-cli
export PATH="$PATH:$(pwd)/target/release"
```

## Quick Start
```bash
# First time setup
threshold-wallet config show

# Get wallet info
threshold-wallet wallet balance
threshold-wallet wallet address

# Send Bitcoin
threshold-wallet send --to <address> --amount <sats>

# Check transaction
threshold-wallet tx status <txid>
threshold-wallet tx list
```

## Global Flags
```
--api-endpoint <URL>   # Override API endpoint
--output <FORMAT>      # Output: table, json
--json                 # Enable JSON output
--no-color             # Disable colors
```

## Commands

### Wallet
```bash
threshold-wallet wallet balance              # Get balance
threshold-wallet wallet address              # Get address
```

### Send
```bash
threshold-wallet send \
  --to <address> \
  --amount <sats> \
  [--metadata <text>]                       # Optional metadata
```

### Transactions
```bash
threshold-wallet tx status <txid>           # Get status
threshold-wallet tx list                    # List all
threshold-wallet tx list --json             # JSON output
```

### Cluster
```bash
threshold-wallet cluster status             # Cluster health
threshold-wallet cluster nodes              # List nodes
```

### DKG (Distributed Key Generation)
```bash
threshold-wallet dkg start \
  --protocol <cggmp24|frost> \
  --threshold <n> \
  --total <m>

threshold-wallet dkg status [--session-id <id>]
```

### Presignatures
```bash
threshold-wallet presig generate --count <n>  # Generate
threshold-wallet presig list                  # List all
threshold-wallet presig status                # Pool status
```

### Configuration
```bash
threshold-wallet config show                    # Show config
threshold-wallet config set-endpoint <url>      # Set endpoint
threshold-wallet config set-node-id <id>        # Set node ID
threshold-wallet config set-format <format>     # Set format
```

## Examples

### Check Balance & Send
```bash
# Get balance
$ threshold-wallet wallet balance

Wallet Balance
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  Confirmed: 1,500,000 sats
  Confirmed (BTC): 0.01500000 BTC
  Total: 1,500,000 sats
  Total (BTC): 0.01500000 BTC

# Send 50,000 sats
$ threshold-wallet send \
  --to tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx \
  --amount 50000
```

### Monitor Transaction
```bash
# Check status
$ threshold-wallet tx status abc123...

Transaction Details
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  Transaction ID: abc123...
  State: confirmed
  Recipient: tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx
  Amount: 50,000 sats
  Fee: 1,500 sats
  Created: 10 minutes ago

✓ Transaction confirmed on-chain
```

### JSON Mode for Scripting
```bash
# Get balance as JSON
$ threshold-wallet wallet balance --json
{
  "confirmed": 1500000,
  "unconfirmed": 0,
  "total": 1500000
}

# Parse with jq
$ threshold-wallet wallet balance --json | jq '.total'
1500000

# List confirmed transactions
$ threshold-wallet tx list --json | jq '.[] | select(.state == "confirmed")'
```

### Cluster Monitoring
```bash
# Check cluster health
$ threshold-wallet cluster status

Cluster Status
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  Status: healthy
  Total Nodes: 5
  Healthy Nodes: 5
  Threshold: 3
  Checked At: just now

✓ Cluster has sufficient healthy nodes for consensus

# List all nodes
$ threshold-wallet cluster nodes
```

## Transaction States

| State | Color | Meaning |
|-------|-------|---------|
| pending | Yellow | Awaiting vote collection |
| voting | Yellow | Currently collecting votes |
| collecting | Yellow | Collecting signatures |
| threshold_reached | Blue | Consensus reached |
| approved | Blue | Transaction approved |
| signing | Magenta | Multi-party signing in progress |
| signed | Cyan | Signature complete |
| submitted | Cyan | Submitted to network |
| broadcasting | Cyan | Broadcasting to network |
| confirmed | Green | Confirmed on blockchain |
| rejected | Red | Transaction rejected |
| failed | Red | Transaction failed |
| aborted_byzantine | Red | Aborted due to Byzantine behavior |

## Configuration File

Location: `~/.threshold-wallet/config.toml`

```toml
api_endpoint = "http://localhost:8080"
node_id = null
timeout_secs = 30
output_format = "table"
colored = true
```

## Environment Variables

```bash
# Override API endpoint (future enhancement)
export THRESHOLD_API_ENDPOINT=http://localhost:8080
```

## Troubleshooting

### API Connection Failed
```bash
# Check if API server is running
curl http://localhost:8080/health

# Override endpoint
threshold-wallet --api-endpoint http://localhost:8080 wallet balance
```

### Config Issues
```bash
# Show config location
threshold-wallet config show

# Reset config (delete and recreate)
rm ~/.threshold-wallet/config.toml
threshold-wallet config show
```

### No Color in Output
```bash
# Force no color
threshold-wallet --no-color wallet balance

# Or set in config
threshold-wallet config set-format json
```

## Help

```bash
# General help
threshold-wallet --help

# Command help
threshold-wallet wallet --help
threshold-wallet send --help
threshold-wallet tx --help

# Subcommand help
threshold-wallet tx status --help
```

## API Endpoints

The CLI calls these REST API endpoints:

- `GET /health` - Health check
- `GET /api/v1/wallet/balance` - Get balance
- `GET /api/v1/wallet/address` - Get address
- `POST /api/v1/transactions` - Create transaction
- `GET /api/v1/transactions/:txid` - Get transaction
- `GET /api/v1/transactions` - List transactions
- `GET /api/v1/cluster/status` - Cluster status
- `GET /api/v1/cluster/nodes` - List nodes

## Tips

1. **Use JSON for scripting**: `--json` flag enables machine-readable output
2. **Monitor long operations**: CLI shows progress for DKG and signing
3. **Check config first**: `config show` displays current settings
4. **Color coding helps**: Transaction states are color-coded for quick recognition
5. **Use table mode for humans**: Default table output is optimized for readability

## More Information

- Full documentation: `production/crates/cli/README.md`
- Implementation details: `production/crates/cli/IMPLEMENTATION.md`
- Demo scripts: `production/crates/cli/examples/`
