#!/usr/bin/env bash
set -euo pipefail

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info() { echo -e "${GREEN}▸${NC} $1"; }
warn() { echo -e "${YELLOW}▸${NC} $1"; }

echo ""
echo "  DreamSwarm Development Setup 🐝"
echo "  ─────────────────────────────────"
echo ""

# Check Rust
if command -v rustc &>/dev/null; then
    RUST_VERSION=$(rustc --version | awk '{print $2}')
    info "Rust $RUST_VERSION found"
else
    info "Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

# Install components
info "Installing Rust components..."
rustup component add rustfmt clippy 2>/dev/null || true

# System dependencies
info "Checking system dependencies..."
install_if_missing() {
    local cmd=$1
    local pkg_apt=$2
    local pkg_brew=$3
    if ! command -v "$cmd" &>/dev/null; then
        info "Installing $cmd..."
        if [[ "$(uname)" == "Darwin" ]]; then
            brew install "$pkg_brew"
        elif command -v apt-get &>/dev/null; then
            sudo apt-get install -y "$pkg_apt"
        else
            warn "Please install $cmd manually"
        fi
    else
        info "$cmd ✓"
    fi
}

install_if_missing tmux tmux tmux
install_if_missing rg ripgrep ripgrep
install_if_missing git git git

# Dev tools
info "Installing development tools..."
cargo binstall cargo-watch cargo-nextest cargo-llvm-cov --no-confirm --force 2>/dev/null || true

# Create data directories
info "Creating data directories..."
mkdir -p ~/.dreamswarm/{memory/topics,memory/transcripts,daemon/logs,teams}

# Compile check
info "Running compile check..."
cargo check

# Run tests
info "Running tests..."
cargo test --lib

echo ""
info "✅ Development environment ready!"
echo ""
echo "Next steps:"
echo "  1. Set an API key: export ANTHROPIC_API_KEY=sk-ant-..."
echo "  2. Run DreamSwarm: cargo run"
echo "  3. Run tests: make test"
echo "  4. Watch mode: cargo watch -x run"
echo ""
echo "Read CONTRIBUTING.md for contribution guidelines."
echo "Read ARCHITECTURE.md for system design."
echo ""
