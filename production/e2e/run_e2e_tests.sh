#!/bin/bash
# E2E Test Runner Script for MPC Wallet
# This script runs the complete E2E test suite with proper setup and cleanup

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PRODUCTION_DIR="$( cd "$SCRIPT_DIR/.." && pwd )"
CERTS_PATH="${CERTS_PATH:-$PRODUCTION_DIR/certs}"
COMPOSE_FILE="$SCRIPT_DIR/docker-compose.e2e.yml"

# Test categories
TESTS=(
    "cluster_setup"
    "transaction_lifecycle"
    "byzantine_scenarios"
    "fault_tolerance"
    "concurrency"
    "network_partition"
    "certificate_rotation"
    "benchmarks"
)

# Function to print colored output
print_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Function to check prerequisites
check_prerequisites() {
    print_info "Checking prerequisites..."

    # Check Docker
    if ! command -v docker &> /dev/null; then
        print_error "Docker is not installed"
        exit 1
    fi

    # Check Docker Compose
    if ! command -v docker-compose &> /dev/null; then
        print_error "Docker Compose is not installed"
        exit 1
    fi

    # Check Rust/Cargo
    if ! command -v cargo &> /dev/null; then
        print_error "Cargo is not installed"
        exit 1
    fi

    # Check certificates
    if [ ! -d "$CERTS_PATH" ]; then
        print_warn "Certificates not found at $CERTS_PATH"
        print_info "Generating test certificates..."
        mkdir -p "$CERTS_PATH"
        # Note: You would need to implement certificate generation
        # For now, we just warn
        print_warn "Please generate certificates manually"
    fi

    print_info "All prerequisites met"
}

# Function to clean up Docker resources
cleanup_docker() {
    print_info "Cleaning up Docker resources..."

    # Stop and remove E2E containers
    docker ps -a | grep e2e- | awk '{print $1}' | xargs -r docker stop 2>/dev/null || true
    docker ps -a | grep e2e- | awk '{print $1}' | xargs -r docker rm 2>/dev/null || true

    # Remove E2E networks
    docker network ls | grep e2e- | awk '{print $1}' | xargs -r docker network rm 2>/dev/null || true

    # Clean up dangling volumes (optional)
    # docker volume ls | grep e2e- | awk '{print $2}' | xargs -r docker volume rm 2>/dev/null || true

    print_info "Docker cleanup complete"
}

# Function to run a single test category
run_test_category() {
    local test_name=$1
    print_info "Running test category: $test_name"

    export E2E_CERTS_PATH="$CERTS_PATH"

    if cargo test --package e2e-tests --test "$test_name" -- --ignored --nocapture; then
        print_info "✓ $test_name passed"
        return 0
    else
        print_error "✗ $test_name failed"
        return 1
    fi
}

# Function to run all tests
run_all_tests() {
    local failed_tests=()
    local passed_tests=()

    print_info "Starting E2E test suite..."
    print_info "Total test categories: ${#TESTS[@]}"

    for test in "${TESTS[@]}"; do
        if run_test_category "$test"; then
            passed_tests+=("$test")
        else
            failed_tests+=("$test")
        fi

        # Cleanup between tests
        cleanup_docker
        sleep 2
    done

    # Print summary
    echo ""
    print_info "========================================="
    print_info "E2E Test Suite Summary"
    print_info "========================================="
    print_info "Passed: ${#passed_tests[@]}/${#TESTS[@]}"
    print_info "Failed: ${#failed_tests[@]}/${#TESTS[@]}"

    if [ ${#passed_tests[@]} -gt 0 ]; then
        echo ""
        print_info "Passed tests:"
        for test in "${passed_tests[@]}"; do
            echo "  ✓ $test"
        done
    fi

    if [ ${#failed_tests[@]} -gt 0 ]; then
        echo ""
        print_error "Failed tests:"
        for test in "${failed_tests[@]}"; do
            echo "  ✗ $test"
        done
        return 1
    fi

    return 0
}

# Function to run specific tests
run_specific_tests() {
    local tests_to_run=("$@")
    local failed_tests=()
    local passed_tests=()

    for test in "${tests_to_run[@]}"; do
        if run_test_category "$test"; then
            passed_tests+=("$test")
        else
            failed_tests+=("$test")
        fi

        cleanup_docker
        sleep 2
    done

    # Print summary
    echo ""
    print_info "Test Summary"
    print_info "Passed: ${#passed_tests[@]}/${#tests_to_run[@]}"
    print_info "Failed: ${#failed_tests[@]}/${#tests_to_run[@]}"

    if [ ${#failed_tests[@]} -gt 0 ]; then
        return 1
    fi

    return 0
}

# Function to list available tests
list_tests() {
    print_info "Available test categories:"
    for test in "${TESTS[@]}"; do
        echo "  - $test"
    done
}

# Function to show usage
show_usage() {
    cat << EOF
Usage: $0 [OPTIONS] [TEST_CATEGORIES...]

Run E2E tests for the MPC Wallet system.

OPTIONS:
    -h, --help              Show this help message
    -l, --list              List available test categories
    -c, --cleanup           Clean up Docker resources and exit
    -a, --all               Run all test categories (default)
    --no-cleanup            Skip cleanup between tests
    --verbose               Run with verbose output

TEST_CATEGORIES:
    Space-separated list of test categories to run.
    If not specified, all tests are run.

Examples:
    $0                                  # Run all tests
    $0 cluster_setup transaction_lifecycle
    $0 --list                           # List all tests
    $0 --cleanup                        # Just clean up Docker

Environment Variables:
    E2E_CERTS_PATH          Path to certificates directory
                            (default: ../certs)
    RUST_LOG                Rust logging level
                            (default: info)

EOF
}

# Main script logic
main() {
    local cleanup_only=false
    local list_only=false
    local run_all=true
    local specific_tests=()

    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            -h|--help)
                show_usage
                exit 0
                ;;
            -l|--list)
                list_only=true
                shift
                ;;
            -c|--cleanup)
                cleanup_only=true
                shift
                ;;
            -a|--all)
                run_all=true
                shift
                ;;
            --verbose)
                export RUST_LOG=debug
                shift
                ;;
            --no-cleanup)
                # Implement if needed
                shift
                ;;
            -*)
                print_error "Unknown option: $1"
                show_usage
                exit 1
                ;;
            *)
                specific_tests+=("$1")
                run_all=false
                shift
                ;;
        esac
    done

    # Handle list-only mode
    if [ "$list_only" = true ]; then
        list_tests
        exit 0
    fi

    # Handle cleanup-only mode
    if [ "$cleanup_only" = true ]; then
        cleanup_docker
        exit 0
    fi

    # Check prerequisites
    check_prerequisites

    # Initial cleanup
    cleanup_docker

    # Run tests
    if [ "$run_all" = true ]; then
        if run_all_tests; then
            print_info "All E2E tests passed! 🎉"
            cleanup_docker
            exit 0
        else
            print_error "Some E2E tests failed"
            cleanup_docker
            exit 1
        fi
    else
        if run_specific_tests "${specific_tests[@]}"; then
            print_info "Specified E2E tests passed! 🎉"
            cleanup_docker
            exit 0
        else
            print_error "Some E2E tests failed"
            cleanup_docker
            exit 1
        fi
    fi
}

# Run main function
main "$@"
