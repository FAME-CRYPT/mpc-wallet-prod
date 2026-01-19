#!/bin/bash

# Comprehensive rename script: mtls-sharedmem → p2p-comm, mtls-with-mtls → mtls-comm
# Run this AFTER manually renaming the folders in Windows Explorer

set -e

echo "========================================"
echo "  Project Rename: Updating References"
echo "========================================"
echo ""

# Check if folders were renamed
if [ ! -d "p2p-comm" ]; then
    echo "❌ Error: p2p-comm folder not found"
    echo "   Please rename 'mtls-sharedmem' to 'p2p-comm' first"
    exit 1
fi

if [ ! -d "mtls-comm" ]; then
    echo "❌ Error: mtls-comm folder not found"
    echo "   Please rename 'mtls-with-mtls' to 'mtls-comm' first"
    exit 1
fi

echo "✓ Folders found: p2p-comm and mtls-comm"
echo ""

# Function to replace in file
replace_in_file() {
    local file=$1
    echo "  Updating: $file"

    # Use sed for replacements
    sed -i 's/mtls-sharedmem/p2p-comm/g' "$file"
    sed -i 's/mtls-with-mtls/mtls-comm/g' "$file"
    sed -i 's/MtlsSharedmem/P2pComm/g' "$file"
    sed -i 's/MtlsWithMtls/MtlsComm/g' "$file"

    # Fix display names
    sed -i 's/p2p-comm (libp2p)/p2p-comm (libp2p)/g' "$file"
    sed -i 's/mtls-comm (pure mTLS)/mtls-comm (pure mTLS)/g' "$file"
}

echo "Updating benchmark-suite files..."

# Core Rust files
replace_in_file "benchmark-suite/src/lib.rs"
replace_in_file "benchmark-suite/src/main.rs"
replace_in_file "benchmark-suite/src/integration_bench.rs"
replace_in_file "benchmark-suite/benches/network_throughput.rs"

# Bash scripts
replace_in_file "benchmark-suite/run_simple_benchmark.sh"
replace_in_file "benchmark-suite/run_benchmarks.sh"

# Python script
replace_in_file "benchmark-suite/analyze_results.py"

# Documentation
replace_in_file "benchmark-suite/README.md"
replace_in_file "benchmark-suite/BENCHMARK_REPORT.md"
replace_in_file "benchmark-suite/BENCHMARK_SUMMARY.md"

echo ""
echo "Updating root documentation files..."

# Root level docs
replace_in_file "BENCHMARK_README.md"
replace_in_file "BENCHMARK_INSTRUCTIONS.md"
replace_in_file "BENCHMARK_COMPARISON.md"
replace_in_file "INTEGRATION-PLAN.md"
replace_in_file "PROJELER-OZET.md"

echo ""
echo "Updating project READMEs..."

# Project-specific READMEs
replace_in_file "p2p-comm/README.md"
replace_in_file "p2p-comm/QUICK_REFERENCE.md"
replace_in_file "p2p-comm/TEST_COMMANDS.md"
replace_in_file "p2p-comm/CURRENT_IMPLEMENTATION_STATUS.md"
replace_in_file "p2p-comm/SECURITY_TESTING.md"
replace_in_file "p2p-comm/QUICKSTART-WINDOWS.md"
replace_in_file "p2p-comm/src/benchmark.rs"

replace_in_file "mtls-comm/README.md"
replace_in_file "mtls-comm/Cargo.toml"
replace_in_file "mtls-comm/src/benchmark.rs"

echo ""
echo "✅ All references updated successfully!"
echo ""
echo "Next steps:"
echo "  1. Rebuild projects: cd p2p-comm && cargo build --release"
echo "  2. Rebuild projects: cd mtls-comm && cargo build --release"
echo "  3. Update Docker Compose if needed"
echo ""
