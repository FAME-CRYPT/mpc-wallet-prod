#!/bin/bash
set -e

echo "Setting up protobuf compiler..."

if command -v protoc &> /dev/null; then
    echo "protoc is already installed: $(protoc --version)"
    exit 0
fi

OS=$(uname -s)

case "$OS" in
    Linux)
        echo "Detected Linux"
        if command -v apt-get &> /dev/null; then
            echo "Installing via apt-get..."
            sudo apt-get update
            sudo apt-get install -y protobuf-compiler
        elif command -v yum &> /dev/null; then
            echo "Installing via yum..."
            sudo yum install -y protobuf-compiler
        else
            echo "Please install protobuf-compiler manually"
            echo "See: https://github.com/protocolbuffers/protobuf/releases"
            exit 1
        fi
        ;;
    Darwin)
        echo "Detected macOS"
        if command -v brew &> /dev/null; then
            echo "Installing via Homebrew..."
            brew install protobuf
        else
            echo "Please install Homebrew first: https://brew.sh/"
            exit 1
        fi
        ;;
    *)
        echo "Unsupported OS: $OS"
        echo "Please install protobuf-compiler manually"
        echo "See: https://github.com/protocolbuffers/protobuf/releases"
        exit 1
        ;;
esac

echo "protobuf installation complete!"
protoc --version
