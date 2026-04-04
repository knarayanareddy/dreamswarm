#!/bin/bash
set -e

# DreamSwarm One-Line Install Script 🐝
# Platform detection, dependency check, and installation.

echo "🐝 Installing DreamSwarm..."

# Binary name
BINARY_NAME="dreamswarm"

# Detect OS
OS="$(uname -s)"
case "${OS}" in
    Linux*)     PLATFORM=linux;;
    Darwin*)    PLATFORM=macos;;
    *)          echo "Unsupported platform: ${OS}"; exit 1
esac

echo "Detected platform: ${PLATFORM}"

# Check for Rust
if ! command -v cargo &> /dev/null; then
    echo "Rust not found. Installing rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
else
    echo "Rust found: $(rustc --version)"
fi

# Check for dependencies
echo "Checking for system dependencies..."
if [ "$PLATFORM" == "linux" ]; then
    sudo apt-get update && sudo apt-get install -y tmux ripgrep git
elif [ "$PLATFORM" == "macos" ]; then
    brew install tmux ripgrep git
fi

# Build for release
echo "Building DreamSwarm in release mode..."
cargo build --release

# Installation
INSTALL_DIR="/usr/local/bin"
if [ -w "$INSTALL_DIR" ]; then
    cp target/release/${BINARY_NAME} "${INSTALL_DIR}/${BINARY_NAME}"
else
    sudo cp target/release/${BINARY_NAME} "${INSTALL_DIR}/${BINARY_NAME}"
fi

echo "✅ DreamSwarm installed successfully!"
echo "Run 'dreamswarm help' to get started."
