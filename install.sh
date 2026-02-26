#!/bin/bash

set -e

# bal one-line installer
# Downloads and installs the latest release from GitHub

REPO="${REPO:-parkjangwon/bal}"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
BINARY_NAME="${BINARY_NAME:-bal}"
CONFIG_DIR="$HOME/.bal"
PID_FILE="$CONFIG_DIR/bal.pid"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Check if bal is already installed
check_existing_installation() {
    if command -v "$BINARY_NAME" &> /dev/null; then
        return 0
    fi
    return 1
}

# Get installed version
get_installed_version() {
    "$BINARY_NAME" --version 2>/dev/null | awk '{print $2}' || echo "unknown"
}

# Uninstall bal
uninstall_bal() {
    # Stop service
    if [[ -f "$PID_FILE" ]]; then
        PID=$(cat "$PID_FILE" 2>/dev/null)
        if [[ -n "$PID" ]] && kill -0 "$PID" 2>/dev/null; then
            echo "Stopping bal service..."
            "$BINARY_NAME" stop 2>/dev/null || kill "$PID" 2>/dev/null || true
            sleep 1
            if kill -0 "$PID" 2>/dev/null; then
                kill -9 "$PID" 2>/dev/null || true
            fi
        fi
    fi
    
    # Check sudo
    if [[ -w "$INSTALL_DIR" ]]; then
        SUDO=""
    else
        SUDO="sudo"
    fi
    
    # Remove binary
    if [[ -f "$INSTALL_DIR/$BINARY_NAME" ]]; then
        echo "Removing $BINARY_NAME..."
        $SUDO rm -f "$INSTALL_DIR/$BINARY_NAME"
    fi
    
    # Remove config
    if [[ -d "$CONFIG_DIR" ]]; then
        echo "Removing config folder..."
        rm -rf "$CONFIG_DIR"
    fi
    
    echo ""
    echo -e "${GREEN}Done! bal is removed.${NC}"
}

# Show help
show_help() {
    echo "bal installer"
    echo ""
    echo "Usage:"
    echo "  install.sh              Install or update bal"
    echo "  install.sh --uninstall  Uninstall bal"
    echo "  install.sh --help       Show this help"
    echo ""
    echo "Options:"
    echo "  --uninstall    Remove bal"
    echo "  --help, -h     Show help"
}

# Handle commands
if [[ "$1" == "--uninstall" ]]; then
    if ! check_existing_installation; then
        echo -e "${RED}bal is not installed.${NC}"
        exit 1
    fi
    uninstall_bal
    exit 0
fi

if [[ "$1" == "--help" || "$1" == "-h" ]]; then
    show_help
    exit 0
fi

if [[ -n "$1" ]]; then
    echo "Unknown option: $1"
    echo "Run './install.sh --help' for usage."
    exit 1
fi

# Detect OS
if [[ "$OSTYPE" == "darwin"* ]]; then
    OS="macos"
elif [[ "$OSTYPE" == "linux"* ]]; then
    OS="linux"
else
    echo -e "${RED}Unsupported OS: $OSTYPE${NC}"
    exit 1
fi

# Detect architecture
ARCH=$(uname -m)
case "$ARCH" in
    "arm64"|"aarch64")
        if [[ "$OS" == "macos" ]]; then
            TARGET="macos-arm64"
        else
            echo -e "${RED}Unsupported architecture: $ARCH on Linux${NC}"
            exit 1
        fi
        ;;
    "x86_64"|"amd64")
        if [[ "$OS" == "linux" ]]; then
            TARGET="linux-amd64"
        else
            echo -e "${RED}macOS x86_64 not supported. Only Apple Silicon (arm64) is available.${NC}"
            exit 1
        fi
        ;;
    "i386"|"i686")
        if [[ "$OS" == "linux" ]]; then
            TARGET="linux-i386"
        else
            echo -e "${RED}Unsupported architecture: $ARCH on macOS${NC}"
            exit 1
        fi
        ;;
    *)
        echo -e "${RED}Unsupported architecture: $ARCH${NC}"
        exit 1
        ;;
esac

# Check if already installed for update flow
IS_UPDATE=false
if check_existing_installation; then
    INSTALLED_VERSION=$(get_installed_version)
    IS_UPDATE=true
    echo -e "${YELLOW}bal is already installed (version: $INSTALLED_VERSION)${NC}"
    echo "This will update to the latest version."
    echo ""
fi

# Get latest release version
echo "Fetching latest release..."
LATEST_RELEASE=$(curl -s "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

if [[ -z "$LATEST_RELEASE" ]]; then
    echo -e "${RED}Failed to fetch latest release${NC}"
    exit 1
fi

# If updating, check if already on latest version
if [[ "$IS_UPDATE" == true ]]; then
    if [[ "$INSTALLED_VERSION" == "$LATEST_RELEASE" ]]; then
        echo -e "${GREEN}You already have the latest version ($LATEST_RELEASE)${NC}"
        exit 0
    fi
    echo "Updating from $INSTALLED_VERSION to $LATEST_RELEASE..."
else
    echo "Latest release: $LATEST_RELEASE"
fi

# Create temp directory
TMP_DIR=$(mktemp -d)
trap "rm -rf $TMP_DIR" EXIT

# Download binary
DOWNLOAD_URL="https://github.com/$REPO/releases/download/$LATEST_RELEASE/bal-$TARGET.tar.gz"
echo "Downloading bal-$TARGET.tar.gz..."
if ! curl -L -o "$TMP_DIR/bal.tar.gz" "$DOWNLOAD_URL"; then
    echo -e "${RED}Failed to download bal${NC}"
    exit 1
fi

# Extract
echo "Extracting..."
if ! tar -xzf "$TMP_DIR/bal.tar.gz" -C "$TMP_DIR"; then
    echo -e "${RED}Failed to extract archive${NC}"
    exit 1
fi

# Check if we need sudo
if [[ -w "$INSTALL_DIR" ]]; then
    SUDO=""
else
    echo "Installation requires sudo access..."
    SUDO="sudo"
fi

# If updating, stop the service first
if [[ "$IS_UPDATE" == true ]]; then
    if [[ -f "$PID_FILE" ]]; then
        PID=$(cat "$PID_FILE" 2>/dev/null)
        if [[ -n "$PID" ]] && kill -0 "$PID" 2>/dev/null; then
            echo "Stopping bal service for update..."
            "$BINARY_NAME" stop 2>/dev/null || true
            sleep 1
        fi
    fi
fi

# Install binary
echo "Installing to $INSTALL_DIR..."
$SUDO mv "$TMP_DIR/bal" "$INSTALL_DIR/$BINARY_NAME"
$SUDO chmod +x "$INSTALL_DIR/$BINARY_NAME"

# Create default config file if it doesn't exist
if [[ ! -f "$HOME/.bal/config.yaml" ]]; then
    echo "Creating default configuration file..."
    mkdir -p "$HOME/.bal"
    cat > "$HOME/.bal/config.yaml" << 'CONFIGEOF'
# bal service port
port: 9295

# Load balancing method
method: "round_robin"

# Log level (debug, info, warn, error)
log_level: "info"

# Backend server list
backends:
  - host: "127.0.0.1"
    port: 9000
  - host: "127.0.0.1"
    port: 9100
CONFIGEOF
    echo "Default config created at ~/.bal/config.yaml"
    echo "Please update the backend addresses before starting bal."
fi

# Verify installation
if command -v "$BINARY_NAME" &> /dev/null; then
    NEW_VERSION=$("$BINARY_NAME" --version 2>/dev/null | awk '{print $2}')
    echo ""
    if [[ "$IS_UPDATE" == true ]]; then
        echo -e "${GREEN}Updated! ($INSTALLED_VERSION → $NEW_VERSION)${NC}"
    else
        echo -e "${GREEN}Installed! (version: $NEW_VERSION)${NC}"
    fi
    echo ""
    echo "Usage: bal --help"
    
    # If was running before update, suggest restart
    if [[ "$IS_UPDATE" == true ]] && [[ -f "$PID_FILE" ]]; then
        echo ""
        echo -e "${YELLOW}Note: bal was stopped during update.${NC}"
        echo "Run 'bal start' to start again."
    fi
else
    echo -e "${RED}Installed, but $BINARY_NAME not found in PATH${NC}"
    echo "You may need to restart your shell"
fi

