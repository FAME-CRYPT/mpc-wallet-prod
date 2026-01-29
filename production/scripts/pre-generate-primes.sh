#!/bin/bash
set -e

# Pre-generate primes for all parties (0-4)
# This prevents the timing issue where all nodes generate primes simultaneously

echo "=================================="
echo "PRIME PRE-GENERATION SCRIPT"
echo "=================================="
echo ""
echo "This will generate primes for all 5 parties (0-4)"
echo "Each generation takes 30-120 seconds"
echo "Total time: ~5-10 minutes"
echo ""

cd "$(dirname "$0")/.."

# Build the binary if needed
if [ ! -f "target/release/mpc-wallet-server" ]; then
    echo "Building mpc-wallet-server..."
    cargo build --release --bin mpc-wallet-server
fi

# Create data directory if it doesn't exist
mkdir -p data

# Generate primes for each party
for party in 0 1 2 3 4; do
    echo ""
    echo "========================================"
    echo "Generating primes for Party $party..."
    echo "========================================"

    # Check if primes already exist
    if [ -f "data/primes-party-$party.json" ]; then
        echo "⚠️  Primes for party $party already exist. Skipping..."
        continue
    fi

    # Run the binary with a special env var to trigger prime generation
    # We'll create a small Rust program for this
    echo "Generating... (this may take 30-120 seconds)"

    # Use the primes generation from the crates/protocols
    PARTY_INDEX=$party cargo run --release --bin pre-generate-primes

    if [ -f "data/primes-party-$party.json" ]; then
        echo "✅ Successfully generated primes for party $party"
        # Show file size
        size=$(du -h "data/primes-party-$party.json" | cut -f1)
        echo "   File size: $size"
    else
        echo "❌ Failed to generate primes for party $party"
        exit 1
    fi
done

echo ""
echo "=================================="
echo "✅ ALL PRIMES GENERATED!"
echo "=================================="
echo ""
echo "Generated files:"
ls -lh data/primes-party-*.json

echo ""
echo "Next steps:"
echo "1. Copy primes to Docker volumes:"
echo "   docker cp data/primes-party-0.json mpc-node-1:/data/"
echo "   docker cp data/primes-party-1.json mpc-node-2:/data/"
echo "   docker cp data/primes-party-2.json mpc-node-3:/data/"
echo "   docker cp data/primes-party-3.json mpc-node-4:/data/"
echo "   docker cp data/primes-party-4.json mpc-node-5:/data/"
echo ""
echo "2. Restart containers:"
echo "   docker compose restart"
