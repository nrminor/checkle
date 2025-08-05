#!/usr/bin/env bash
set -euo pipefail

# checkle installation script
# This script attempts to download a pre-built binary from GitHub releases,
# falling back to building from source if necessary.

REPO="nrminor/checkle"
BINARY_NAME="checkle"
INSTALL_DIR="${HOME}/.local/bin"
USE_SIMD=false

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Helper functions
info() { echo -e "${GREEN}[INFO]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1" >&2; }

# Detect OS and architecture
detect_platform() {
	local os arch

	case "$(uname -s)" in
	Linux) os="linux" ;;
	Darwin) os="darwin" ;;
	MINGW* | MSYS* | CYGWIN*) os="windows" ;;
	*)
		error "Unsupported OS: $(uname -s)"
		exit 1
		;;
	esac

	case "$(uname -m)" in
	x86_64 | amd64) arch="x86_64" ;;
	aarch64 | arm64) arch="aarch64" ;;
	*)
		error "Unsupported architecture: $(uname -m)"
		exit 1
		;;
	esac

	# Map to Rust target triple
	case "${os}-${arch}" in
	linux-x86_64) echo "x86_64-unknown-linux-musl" ;;
	linux-aarch64) echo "aarch64-unknown-linux-musl" ;;
	darwin-x86_64) echo "x86_64-apple-darwin" ;;
	darwin-aarch64) echo "aarch64-apple-darwin" ;;
	windows-x86_64) echo "x86_64-pc-windows-msvc" ;;
	windows-aarch64) echo "aarch64-pc-windows-msvc" ;;
	*)
		error "Unsupported platform: ${os}-${arch}"
		exit 1
		;;
	esac
}

# Get latest release tag from GitHub
get_latest_release() {
	local url="https://api.github.com/repos/${REPO}/releases/latest"
	if command -v curl &>/dev/null; then
		curl -fsSL "$url" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/'
	elif command -v wget &>/dev/null; then
		wget -qO- "$url" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/'
	else
		error "Neither curl nor wget found. Please install one of them."
		exit 1
	fi
}

# Download and install binary
install_binary() {
	local version="$1"
	local platform="$2"
	local archive_ext="tar.gz"

	if [[ "$platform" == *"windows"* ]]; then
		archive_ext="zip"
	fi

	# Add -simd suffix if SIMD build requested
	local binary_suffix=""
	if [[ "$USE_SIMD" == "true" ]]; then
		binary_suffix="-simd"
		info "Using SIMD-optimized build"
	fi

	local download_url="https://github.com/${REPO}/releases/download/${version}/${BINARY_NAME}-${platform}${binary_suffix}.${archive_ext}"
	local temp_dir=$(mktemp -d)

	info "Downloading ${BINARY_NAME} ${version} for ${platform}..."

	cd "$temp_dir"
	if command -v curl &>/dev/null; then
		curl -fsSL -o "archive.${archive_ext}" "$download_url" || return 1
	else
		wget -q -O "archive.${archive_ext}" "$download_url" || return 1
	fi

	info "Extracting binary..."
	if [[ "$archive_ext" == "zip" ]]; then
		unzip -q "archive.${archive_ext}"
	else
		tar -xzf "archive.${archive_ext}"
	fi

	info "Installing to ${INSTALL_DIR}..."
	mkdir -p "$INSTALL_DIR"

	if [[ -f "${BINARY_NAME}" ]]; then
		chmod +x "${BINARY_NAME}"
		mv "${BINARY_NAME}" "${INSTALL_DIR}/"
	elif [[ -f "${BINARY_NAME}.exe" ]]; then
		mv "${BINARY_NAME}.exe" "${INSTALL_DIR}/"
	else
		error "Binary not found in archive"
		return 1
	fi

	cd - >/dev/null
	rm -rf "$temp_dir"

	info "Successfully installed ${BINARY_NAME} to ${INSTALL_DIR}"
	return 0
}

# Build from source
build_from_source() {
	info "Building from source..."

	# Check for Rust
	if ! command -v cargo &>/dev/null; then
		warn "Rust not found. Installing Rust toolchain..."
		curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
		export PATH="$HOME/.cargo/bin:$PATH"
	fi

	# Clone and build
	local temp_dir=$(mktemp -d)
	cd "$temp_dir"

	info "Cloning repository..."
	git clone "https://github.com/${REPO}.git" .

	info "Building ${BINARY_NAME}..."
	cargo build --release

	info "Installing to ${INSTALL_DIR}..."
	mkdir -p "$INSTALL_DIR"
	cp "target/release/${BINARY_NAME}" "$INSTALL_DIR/"

	cd - >/dev/null
	rm -rf "$temp_dir"

	info "Successfully built and installed ${BINARY_NAME}"
}

# Parse command line arguments
parse_args() {
	while [[ $# -gt 0 ]]; do
		case $1 in
		--simd)
			USE_SIMD=true
			shift
			;;
		-h | --help)
			echo "Usage: $0 [--simd] [--help]"
			echo "  --simd    Install SIMD-optimized build (requires modern CPU)"
			echo "  --help    Show this help message"
			exit 0
			;;
		*)
			warn "Unknown option: $1"
			echo "Use --help for usage information"
			exit 1
			;;
		esac
	done
}

# Main installation logic
main() {
	parse_args "$@"
	
	info "Installing ${BINARY_NAME}..."

	local platform=$(detect_platform)
	info "Detected platform: ${platform}"

	# Try to download pre-built binary
	local version=$(get_latest_release)
	if [[ -n "$version" ]]; then
		info "Latest release: ${version}"
		if install_binary "$version" "$platform"; then
			# Success
			:
		else
			warn "Failed to download pre-built binary. Falling back to source build..."
			build_from_source
		fi
	else
		warn "Could not determine latest release. Building from source..."
		build_from_source
	fi

	# Add to PATH if needed
	if [[ ":$PATH:" != *":${INSTALL_DIR}:"* ]]; then
		warn "${INSTALL_DIR} is not in your PATH"
		info "Add the following to your shell configuration file:"
		echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
	fi

	# Verify installation
	if command -v "${BINARY_NAME}" &>/dev/null; then
		info "Installation complete! Run '${BINARY_NAME} --help' to get started."
	else
		warn "Installation complete, but ${BINARY_NAME} is not in PATH."
		info "You can run it directly: ${INSTALL_DIR}/${BINARY_NAME}"
	fi
}

main "$@"
