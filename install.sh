#!/bin/bash

set -e

# bal one-line installer
# Downloads and installs the latest release from GitHub

REPO="parkjangwon/bal"
INSTALL_DIR="/usr/local/bin"
BINARY_NAME="bal"

# Detect OS
if [[ "$OSTYPE" == "darwin"* ]]; then
    OS="macos"
elif [[ "$OSTYPE" == "linux"* ]]; then
    OS="linux"
else
    echo "Unsupported OS: $OSTYPE"
    exit 1
fi

# Detect architecture
ARCH=$(uname -m)
case "$ARCH" in
    "arm64"|"aarch64")
        if [[ "$OS" == "macos" ]]; then
            TARGET="macos-arm64"
        else
            echo "Unsupported architecture: $ARCH on Linux"
            exit 1
        fi
        ;;
    "x86_64"|"amd64")
        if [[ "$OS" == "linux" ]]; then
            TARGET="linux-amd64"
        else
            echo "macOS x86_64 not supported. Only Apple Silicon (arm64) is available."
            exit 1
        fi
        ;;
    "i386"|"i686")
        if [[ "$OS" == "linux" ]]; then
            TARGET="linux-i386"
        else
            echo "Unsupported architecture: $ARCH on macOS"
            exit 1
        fi
        ;;
    *)
        echo "Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

# Get latest release version
echo "Fetching latest release..."
LATEST_RELEASE=$(curl -s "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

if [[ -z "$LATEST_RELEASE" ]]; then
    echo "Failed to fetch latest release"
    exit 1
fi

echo "Latest release: $LATEST_RELEASE"

# Create temp directory
TMP_DIR=$(mktemp -d)
trap "rm -rf $TMP_DIR" EXIT

# Download binary
DOWNLOAD_URL="https://github.com/$REPO/releases/download/$LATEST_RELEASE/bal-$TARGET.tar.gz"
echo "Downloading bal-$TARGET.tar.gz..."
curl -L -o "$TMP_DIR/bal.tar.gz" "$DOWNLOAD_URL"

# Extract
echo "Extracting..."
tar -xzf "$TMP_DIR/bal.tar.gz" -C "$TMP_DIR"

# Check if we need sudo
if [[ -w "$INSTALL_DIR" ]]; then
    SUDO=""
else
    echo "Installation requires sudo access..."
    SUDO="sudo"
fi

# Install binary
echo "Installing to $INSTALL_DIR..."
$SUDO mv "$TMP_DIR/bal" "$INSTALL_DIR/$BINARY_NAME"
$SUDO chmod +x "$INSTALL_DIR/$BINARY_NAME"

# Verify installation
if command -v "$BINARY_NAME" &> /dev/null; then
    echo ""
    echo "✓ bal installed successfully!"
    echo ""
    "$BINARY_NAME" --version
    echo ""
    echo "Usage: bal --help"
else
    echo "Installation complete, but $BINARY_NAME is not in your PATH"
    echo "You may need to add $INSTALL_DIR to your PATH or restart your shell"
fi
