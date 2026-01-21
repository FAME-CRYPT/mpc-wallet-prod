#!/bin/bash
# Integration test runner for MPC Wallet
# Usage: ./run_tests.sh [test_name]

set -e

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Check Docker is running
echo -e "${BLUE}Checking Docker...${NC}"
if ! docker ps >/dev/null 2>&1; then
    echo -e "${RED}❌ Docker is not running. Please start Docker and try again.${NC}"
    exit 1
fi
echo -e "${GREEN}✅ Docker is running${NC}"

# Test suites
TESTS=(
    "storage_integration"
    "consensus_integration"
    "bitcoin_integration"
    "api_integration"
    "protocols_integration"
    "network_integration"
)

# Function to run a single test
run_test() {
    local test_name=$1
    echo -e "\n${BLUE}Running ${test_name}...${NC}"
    if cargo test --test "$test_name" -- --test-threads=1; then
        echo -e "${GREEN}✅ ${test_name} passed${NC}"
        return 0
    else
        echo -e "${RED}❌ ${test_name} failed${NC}"
        return 1
    fi
}

# Function to run all tests
run_all_tests() {
    local failed_tests=()
    local passed_tests=()

    echo -e "${YELLOW}Running all integration tests...${NC}\n"

    for test in "${TESTS[@]}"; do
        if run_test "$test"; then
            passed_tests+=("$test")
        else
            failed_tests+=("$test")
        fi
    done

    # Summary
    echo -e "\n${BLUE}================================${NC}"
    echo -e "${BLUE}Test Summary${NC}"
    echo -e "${BLUE}================================${NC}"
    echo -e "${GREEN}Passed: ${#passed_tests[@]}/${#TESTS[@]}${NC}"

    if [ ${#failed_tests[@]} -gt 0 ]; then
        echo -e "${RED}Failed: ${#failed_tests[@]}/${#TESTS[@]}${NC}"
        echo -e "\n${RED}Failed tests:${NC}"
        for test in "${failed_tests[@]}"; do
            echo -e "  - ${test}"
        done
        exit 1
    else
        echo -e "\n${GREEN}🎉 All tests passed!${NC}"
    fi
}

# Main
if [ $# -eq 0 ]; then
    # No arguments - run all tests
    run_all_tests
else
    # Run specific test
    test_name=$1

    # Check if test exists
    if [[ " ${TESTS[@]} " =~ " ${test_name} " ]]; then
        run_test "$test_name"
    else
        echo -e "${RED}Unknown test: ${test_name}${NC}"
        echo -e "\nAvailable tests:"
        for test in "${TESTS[@]}"; do
            echo -e "  - ${test}"
        done
        exit 1
    fi
fi
