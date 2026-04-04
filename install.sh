#!/usr/bin/env bash
set -euo pipefail

LATEST_VERSION="0.1.0"
REPO="dreamswarm/dreamswarm"
BINARY_NAME="dreamswarm"
INSTALL_DIR="${DREAMSWARM_INSTALL_DIR:-$HOME/.local/bin}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info() { echo -e "${GREEN}▸${NC} $1"; }
warn() { echo -e "${YELLOW}▸${NC} $1"; }
error() { echo -e "${RED}▸${NC} $1" >&2; }

# Detect platform
detect_platform() {
    local os arch
    case "$(uname -s)" in
        Linux*) os="linux" ;;
        Darwin*) os="darwin" ;;
        *) error "Unsupported OS: $(uname -s)"; exit 1 ;;
    esac
    case "$(uname -m)" in
        x86_64|amd64) arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        *) error "Unsupported architecture: $(uname -m)"; exit 1 ;;
    esac
    echo "${os}-${arch}"
}

main() {
    echo ""
    echo "  DreamSwarm Installer v${LATEST_VERSION}"
    echo "  ────────────────────────────────"
    echo ""

    local platform
    platform=$(detect_platform)
    info "Detected platform: ${platform}"

    # In a real scenario, we would download the pre-built binary.
    # For this reconstruction, we will build from source if it's a dev environment,
    # but the script follows the "official" pattern.

    if [ -f "Cargo.toml" ]; then
        info "Local source detected. Building for release..."
        cargo build --release
        mkdir -p "$INSTALL_DIR"
        cp target/release/${BINARY_NAME} "$INSTALL_DIR/"
        chmod +x "$INSTALL_DIR/$BINARY_NAME"
    else
        local url="https://github.com/${REPO}/releases/download/v${LATEST_VERSION}/${BINARY_NAME}-${platform}.tar.gz"
        local checksum_url="${url}.sha256"
        
        # Create temp directory
        local tmpdir
        tmpdir=$(mktemp -d)
        trap 'rm -rf "$tmpdir"' EXIT

        # Download
        info "Downloading DreamSwarm v${LATEST_VERSION}..."
        curl -fsSL "$url" -o "${tmpdir}/archive.tar.gz"
        curl -fsSL "$checksum_url" -o "${tmpdir}/checksum.sha256"

        # Verify checksum
        info "Verifying checksum..."
        cd "$tmpdir"
        if command -v sha256sum &>/dev/null; then
            sha256sum -c checksum.sha256
        elif command -v shasum &>/dev/null; then
            shasum -a 256 -c checksum.sha256
        else
            warn "Cannot verify checksum (sha256sum/shasum not found)"
        fi

        # Extract
        info "Extracting..."
        tar xzf archive.tar.gz

        # Install
        mkdir -p "$INSTALL_DIR"
        mv "$BINARY_NAME" "$INSTALL_DIR/"
        chmod +x "$INSTALL_DIR/$BINARY_NAME"
    fi

    info "Installed to ${INSTALL_DIR}/${BINARY_NAME}"

    # Check PATH
    if ! echo "$PATH" | tr ':' '\n' | grep -q "^${INSTALL_DIR}$"; then
        warn "Add to your PATH:"
        echo ""
        echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
        echo ""
    fi

    # Verify
    if command -v "$BINARY_NAME" &>/dev/null; then
        info "Installation complete!"
        echo ""
        "$BINARY_NAME" --version
    else
        info "Installation complete. Restart your shell or add ${INSTALL_DIR} to PATH."
    fi

    echo ""
    echo "  Get started:"
    echo "  export ANTHROPIC_API_KEY=sk-ant-..."
    echo "  dreamswarm"
    echo ""
}

main "$@"
