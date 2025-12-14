#!/usr/bin/env bash
# Fast-format-x (ffx) installer
# Usage: curl -LsSf https://fast-format-x.briansigafoos.com/install.sh | bash
#
# This script downloads and installs the ffx binary for your platform.

set -euo pipefail

REPO="BrianSigafoos/fast-format-x"
BINARY_NAME="ffx"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

info() {
    echo -e "${BLUE}info:${NC} $1"
}

warn() {
    echo -e "${YELLOW}warn:${NC} $1"
}

error() {
    echo -e "${RED}error:${NC} $1" >&2
    exit 1
}

success() {
    echo -e "${GREEN}success:${NC} $1"
}

# Detect OS
detect_os() {
    case "$(uname -s)" in
        Darwin*)
            echo "darwin"
            ;;
        Linux*)
            echo "linux"
            ;;
        MINGW*|MSYS*|CYGWIN*)
            echo "windows"
            ;;
        *)
            error "Unsupported operating system: $(uname -s)"
            ;;
    esac
}

# Detect architecture
detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)
            echo "x86_64"
            ;;
        arm64|aarch64)
            echo "aarch64"
            ;;
        *)
            error "Unsupported architecture: $(uname -m)"
            ;;
    esac
}

# Get the latest release tag from GitHub
get_latest_version() {
    local latest
    latest=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
    if [ -z "$latest" ]; then
        error "Could not determine latest version. Check https://github.com/${REPO}/releases"
    fi
    echo "$latest"
}

# Determine install directory
get_install_dir() {
    # Prefer ~/.cargo/bin if it exists (Rust convention)
    if [ -d "$HOME/.cargo/bin" ]; then
        echo "$HOME/.cargo/bin"
    # Otherwise use ~/.local/bin (XDG convention)
    elif [ -d "$HOME/.local/bin" ]; then
        echo "$HOME/.local/bin"
    else
        # Create ~/.local/bin if needed
        mkdir -p "$HOME/.local/bin"
        echo "$HOME/.local/bin"
    fi
}

# Global tmp_dir for cleanup trap (local vars go out of scope before EXIT trap runs)
TMP_DIR=""

main() {
    echo ""
    echo "  ╭─────────────────────────────────────────╮"
    echo "  │     fast-format-x (ffx) installer       │"
    echo "  ╰─────────────────────────────────────────╯"
    echo ""

    local os arch version install_dir target download_url

    os=$(detect_os)
    arch=$(detect_arch)
    
    info "Detected platform: ${arch}-${os}"

    # Build target triple
    case "$os" in
        darwin)
            target="${arch}-apple-darwin"
            ;;
        linux)
            target="${arch}-unknown-linux-gnu"
            ;;
        *)
            error "Prebuilt binaries not available for ${os}. Please build from source."
            ;;
    esac

    # Check if target is supported
    if [[ "$os" == "darwin" ]] && [[ "$arch" != "aarch64" && "$arch" != "x86_64" ]]; then
        error "Unsupported macOS architecture: $arch"
    fi

    # Get latest version
    version=${FFX_VERSION:-$(get_latest_version)}
    info "Installing ffx ${version}"

    # Build download URL
    download_url="https://github.com/${REPO}/releases/download/${version}/${BINARY_NAME}-${target}.tar.gz"
    info "Downloading from: $download_url"

    # Create temp directory
    TMP_DIR=$(mktemp -d)
    trap 'rm -rf "$TMP_DIR"' EXIT

    # Download and extract
    if ! curl -fsSL "$download_url" -o "$TMP_DIR/ffx.tar.gz"; then
        error "Failed to download ffx. Check that version ${version} exists at https://github.com/${REPO}/releases"
    fi

    tar -xzf "$TMP_DIR/ffx.tar.gz" -C "$TMP_DIR"

    # Determine install location
    install_dir=$(get_install_dir)
    info "Installing to: $install_dir"

    # Install binary
    mv "$TMP_DIR/${BINARY_NAME}" "$install_dir/${BINARY_NAME}"
    chmod +x "$install_dir/${BINARY_NAME}"

    success "ffx ${version} installed successfully!"
    echo ""

    # Check if install dir is in PATH
    if [[ ":$PATH:" != *":$install_dir:"* ]]; then
        warn "$install_dir is not in your PATH"
        echo ""
        echo "Add it to your shell config:"
        echo ""
        echo "  # For bash (~/.bashrc or ~/.bash_profile):"
        echo "  export PATH=\"$install_dir:\$PATH\""
        echo ""
        echo "  # For zsh (~/.zshrc):"
        echo "  export PATH=\"$install_dir:\$PATH\""
        echo ""
        echo "Then restart your shell or run: source ~/.zshrc"
        echo ""
    else
        echo "Run 'ffx --help' to get started."
        echo ""
    fi
}

main "$@"
