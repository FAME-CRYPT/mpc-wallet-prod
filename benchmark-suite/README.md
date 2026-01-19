# MPC Wallet Benchmark Suite

Comprehensive benchmark suite for comparing MPC wallet implementations:
- **p2p-comm**: libp2p-based networking (Noise, GossipSub, Kademlia)
- **mtls-comm**: Pure mTLS (TLS 1.3) networking with rustls

## Features

### Benchmark Categories

#### 1. Network Performance
- **Throughput**: Votes/second, messages/second, bytes/second
- **Latency**: p50, p95, p99, max, mean, stddev
- **Bandwidth**: Network bytes sent/received

#### 2. Consensus Performance
- Vote processing time
- Byzantine detection latency
- etcd operation latency
- State transition speed

#### 3. Scalability
- Performance under load (100 → 10,000 votes)
- Multi-node scaling (3, 5, 7, 10 nodes)
- Connection establishment time
- Maximum connections per node

#### 4. Resource Usage
- CPU usage (%)
- Memory usage (MB)
- Resource efficiency score

#### 5. Security Metrics
- TLS handshake time
- Certificate validation overhead
- Encryption/decryption overhead

#### 6. Reliability
- Message delivery success rate
- Error rate
- Timeout count
- Recovery time after failure

## Installation

```bash
cd benchmark-suite
cargo build --release
```

## Usage

### Run All Benchmarks

```bash
cargo run --release -- run-all --votes 1000 --concurrent 10 --output results.json
```

### Run Specific Benchmarks

#### Throughput Test
```bash
cargo run --release -- throughput --votes 5000
cargo run --release -- throughput --system mtls-comm --votes 5000
```

#### Latency Test
```bash
cargo run --release -- latency --samples 1000
cargo run --release -- latency --system p2p-comm --samples 1000
```

#### Scalability Test
```bash
cargo run --release -- scalability --node-counts 3 5 7 10
```

#### Security Overhead Test
```bash
cargo run --release -- security
```

### Generate Comparison Report

```bash
cargo run --release -- compare --input results.json
```

## Benchmark Metrics

### Throughput Metrics

| Metric | Description | Unit |
|--------|-------------|------|
| `votes_per_second` | Number of votes processed per second | votes/s |
| `messages_per_second` | Total network messages per second | msg/s |
| `bytes_per_second` | Network bandwidth utilization | bytes/s |

### Latency Metrics

| Metric | Description | Unit |
|--------|-------------|------|
| `latency_p50` | 50th percentile (median) latency | μs |
| `latency_p95` | 95th percentile latency | μs |
| `latency_p99` | 99th percentile latency | μs |
| `latency_max` | Maximum latency observed | μs |
| `latency_mean` | Average latency | μs |
| `latency_stddev` | Standard deviation of latency | μs |

### Consensus Metrics

| Metric | Description | Unit |
|--------|-------------|------|
| `vote_processing_time_us` | Time to process a single vote | μs |
| `byzantine_check_time_us` | Time to check Byzantine violations | μs |
| `etcd_write_time_us` | Time to write to etcd | μs |
| `state_transition_time_us` | FSM state transition time | μs |

### Resource Metrics

| Metric | Description | Unit |
|--------|-------------|------|
| `cpu_usage_percent` | CPU utilization | % |
| `memory_usage_mb` | Memory consumption | MB |
| `network_bytes_sent` | Total bytes transmitted | bytes |
| `network_bytes_received` | Total bytes received | bytes |

### Reliability Metrics

| Metric | Description | Unit |
|--------|-------------|------|
| `total_votes_sent` | Total votes transmitted | count |
| `total_votes_received` | Total votes received | count |
| `delivery_success_rate` | Percentage of successful deliveries | % |
| `error_count` | Number of errors encountered | count |
| `timeout_count` | Number of timeouts | count |

### Security Metrics

| Metric | Description | Unit |
|--------|-------------|------|
| `tls_handshake_time_us` | TLS connection establishment time | μs |
| `cert_validation_time_us` | Certificate validation overhead | μs |
| `encryption_overhead_percent` | Encryption/decryption CPU overhead | % |

## Output Format

Results are saved in JSON format:

```json
[
  {
    "system": "P2pComm",
    "metrics": {
      "votes_per_second": 1250.5,
      "latency_p50": 850,
      "latency_p95": 1200,
      "latency_p99": 1500,
      "cpu_usage_percent": 35.2,
      "memory_usage_mb": 256.8,
      ...
    },
    "timestamp": "2026-01-19T12:00:00Z"
  },
  {
    "system": "MtlsComm",
    "metrics": {
      "votes_per_second": 1420.3,
      "latency_p50": 720,
      "latency_p95": 980,
      "latency_p99": 1250,
      "cpu_usage_percent": 28.5,
      "memory_usage_mb": 198.4,
      ...
    },
    "timestamp": "2026-01-19T12:15:00Z"
  }
]
```

## Comparison Report

The comparison report highlights performance differences:

```
📊 Comparison Report

  Throughput (votes/sec)    | sharedmem: 1250.50 | with-mtls: 1420.30 | ↑ 13.6% | Winner: mtls-comm
  Latency p50 (μs)          | sharedmem: 850.00  | with-mtls: 720.00  | ↓ 15.3% | Winner: mtls-comm
  Latency p95 (μs)          | sharedmem: 1200.00 | with-mtls: 980.00  | ↓ 18.3% | Winner: mtls-comm
  CPU Usage (%)             | sharedmem: 35.20   | with-mtls: 28.50   | ↓ 19.0% | Winner: mtls-comm
  Memory Usage (MB)         | sharedmem: 256.80  | with-mtls: 198.40  | ↓ 22.7% | Winner: mtls-comm
  TLS Handshake (μs)        | sharedmem: 1800.00 | with-mtls: 2200.00 | ↑ 22.2% | Winner: p2p-comm
```

## Running Criterion Benchmarks

For detailed statistical analysis using Criterion:

```bash
# Run all Criterion benchmarks
cargo bench

# Run specific benchmark
cargo bench network_throughput
cargo bench consensus_latency
cargo bench scalability
cargo bench security_overhead
```

Criterion generates HTML reports in `target/criterion/`.

## Docker Integration

The benchmark suite automatically:
1. Starts the target system's Docker containers
2. Waits for initialization (15s warmup)
3. Runs the benchmark tests
4. Collects Docker stats (CPU, memory, network)
5. Stops and cleans up containers

### Manual Docker Control

```bash
# Start p2p-comm
cd ../p2p-comm
docker-compose up -d

# Start mtls-comm
cd ../mtls-comm
docker-compose up -d

# Check container stats
docker stats --no-stream mtls-node-1
```

## Example Benchmark Run

```bash
$ cargo run --release -- run-all --votes 5000 --concurrent 20

🚀 Starting Comprehensive Benchmark Suite

📊 Benchmarking: p2p-comm (libp2p)

  ⏱️  Running throughput test...
  ⏱️  Running latency test...
  ⏱️  Running scalability test...
  ⏱️  Running security overhead test...
  ⏱️  Running resource usage test...

  Results:
    Throughput:      1250.50 votes/sec
    Latency (p50):   850 μs
    Latency (p95):   1200 μs
    Latency (p99):   1500 μs
    CPU Usage:       35.20%
    Memory Usage:    256.80 MB
    TLS Handshake:   1800.00 μs

📊 Benchmarking: mtls-comm (pure mTLS)

  ⏱️  Running throughput test...
  ⏱️  Running latency test...
  ⏱️  Running scalability test...
  ⏱️  Running security overhead test...
  ⏱️  Running resource usage test...

  Results:
    Throughput:      1420.30 votes/sec
    Latency (p50):   720 μs
    Latency (p95):   980 μs
    Latency (p99):   1250 μs
    CPU Usage:       28.50%
    Memory Usage:    198.40 MB
    TLS Handshake:   2200.00 μs

✅ Results saved to: results.json

📊 Comparison Report
[... comparison table ...]
```

## Advanced Usage

### Custom Vote Counts

Test with different loads:

```bash
cargo run --release -- run-all --votes 100    # Light load
cargo run --release -- run-all --votes 1000   # Medium load
cargo run --release -- run-all --votes 10000  # Heavy load
```

### Custom Concurrency

Adjust parallel vote submissions:

```bash
cargo run --release -- run-all --concurrent 5   # Conservative
cargo run --release -- run-all --concurrent 20  # Aggressive
cargo run --release -- run-all --concurrent 50  # Stress test
```

### CSV Export

Convert JSON results to CSV for analysis:

```bash
# Install jq if needed
cat results.json | jq -r '.[] | [.system, .metrics.votes_per_second, .metrics.latency_p50, .metrics.cpu_usage_percent] | @csv'
```

## Troubleshooting

### Docker Issues

If containers fail to start:

```bash
# Clean up existing containers
docker-compose -f ../p2p-comm/docker-compose.yml down -v
docker-compose -f ../mtls-comm/docker-compose.yml down -v

# Remove orphaned containers
docker system prune -f
```

### Permission Errors

Ensure Docker is running and you have permissions:

```bash
docker ps
sudo usermod -aG docker $USER  # Linux
```

### Port Conflicts

If ports are already in use, stop conflicting services or modify `docker-compose.yml`.

## Contributing

To add new benchmarks:

1. Create a new file in `benches/your_benchmark.rs`
2. Implement using Criterion framework
3. Add metrics to `PerformanceMetrics` struct
4. Update comparison logic in `src/main.rs`

## License

MIT License - see LICENSE file for details.
