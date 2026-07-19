#!/usr/bin/env bash
# Fast-format-x (ffx) installer
# Usage: curl -LsSf https://ffx.bfoos.net/install.sh | bash
# Pin a version: curl -LsSf https://ffx.bfoos.net/install.sh | FFX_VERSION=v0.2.0 bash
#
# This script downloads, verifies, and installs the ffx binary for your platform.

set -euo pipefail

REPO="BrianSigafoos/fast-format-x"
BINARY_NAME="ffx"
RELEASES_URL="https://github.com/${REPO}/releases"
CHECKSUM_FILE="SHA256SUMS.txt"

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

# Download an HTTPS release asset without relying on the GitHub REST API.
download() {
    curl --proto '=https' --tlsv1.2 -fsSL "$@"
}

validate_version() {
    case "$1" in
        ""|*[!A-Za-z0-9._-]*)
            error "Invalid FFX_VERSION: $1"
            ;;
    esac
}

# The stable latest-download URL redirects to an immutable versioned release URL.
version_from_redirect_headers() {
    local headers="$1"
    local line location version
    local prefix="${RELEASES_URL}/download/"
    local suffix="/${CHECKSUM_FILE}"

    while IFS= read -r line; do
        line=${line%$'\r'}
        case "$line" in
            [Ll]ocation:\ *)
                location=${line#*: }
                case "$location" in
                    "${prefix}"*"${suffix}")
                        version=${location#"${prefix}"}
                        version=${version%"${suffix}"}
                        echo "$version"
                        return 0
                        ;;
                esac
                ;;
        esac
    done < "$headers"

    return 1
}

calculate_sha256() {
    local file="$1"

    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$file" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$file" | awk '{print $1}'
    else
        error "Cannot verify download: sha256sum or shasum is required"
    fi
}

verify_checksum() {
    local archive="$1"
    local asset_name="$2"
    local checksums="$3"
    local expected actual

    expected=$(awk -v asset="$asset_name" '$2 == asset {print $1; exit}' "$checksums")
    if [ -z "$expected" ]; then
        error "No checksum found for ${asset_name} in ${CHECKSUM_FILE}"
    fi

    if [ "${#expected}" -ne 64 ] || printf '%s' "$expected" | grep -q '[^0-9A-Fa-f]'; then
        error "Invalid checksum for ${asset_name} in ${CHECKSUM_FILE}"
    fi

    actual=$(calculate_sha256 "$archive")
    if [ "$(printf '%s' "$actual" | tr '[:upper:]' '[:lower:]')" != "$(printf '%s' "$expected" | tr '[:upper:]' '[:lower:]')" ]; then
        error "Checksum verification failed for ${asset_name}"
    fi

    info "Verified SHA-256 checksum"
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

    local os arch version install_dir target asset_name release_base
    local checksum_url download_url headers_file checksum_path archive_path

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

    # Create temp directory
    TMP_DIR=$(mktemp -d)
    trap 'rm -rf "$TMP_DIR"' EXIT

    asset_name="${BINARY_NAME}-${target}.tar.gz"
    headers_file="$TMP_DIR/headers"
    checksum_path="$TMP_DIR/${CHECKSUM_FILE}"
    archive_path="$TMP_DIR/$asset_name"

    if [ -n "${FFX_VERSION:-}" ]; then
        version="$FFX_VERSION"
        validate_version "$version"
        release_base="${RELEASES_URL}/download/${version}"
        checksum_url="${release_base}/${CHECKSUM_FILE}"

        if ! download "$checksum_url" -o "$checksum_path"; then
            error "Failed to download checksums for ffx ${version}. Check that the release exists at ${RELEASES_URL}"
        fi
    else
        info "Resolving latest release"
        checksum_url="${RELEASES_URL}/latest/download/${CHECKSUM_FILE}"

        if ! download -D "$headers_file" "$checksum_url" -o "$checksum_path"; then
            error "Failed to resolve the latest ffx release at ${RELEASES_URL}"
        fi

        if ! version=$(version_from_redirect_headers "$headers_file"); then
            error "Could not determine the version from GitHub's latest-release redirect"
        fi
        validate_version "$version"
        release_base="${RELEASES_URL}/download/${version}"
    fi

    info "Installing ffx ${version}"

    download_url="${release_base}/${asset_name}"
    info "Downloading from: $download_url"

    if ! download "$download_url" -o "$archive_path"; then
        error "Failed to download ffx ${version} for ${target} from ${RELEASES_URL}"
    fi

    verify_checksum "$archive_path" "$asset_name" "$checksum_path"

    if ! tar -xzf "$archive_path" -C "$TMP_DIR" "$BINARY_NAME"; then
        error "Failed to extract ${asset_name}"
    fi

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
