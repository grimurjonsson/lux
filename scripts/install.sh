#!/usr/bin/env bash
set -euo pipefail

# Installer script for lux
# Downloads pre-built binaries from GitHub releases

REPO="grimurjonsson/lux"
BINARY_NAME="lux"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

# Colors
BOLD='\033[1m'
DIM='\033[2m'
RESET='\033[0m'
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
MAGENTA='\033[0;35m'
CYAN='\033[0;36m'

echo ""
echo -e "${MAGENTA}${BOLD}╭─────────────────────────────────────╮${RESET}"
echo -e "${MAGENTA}${BOLD}│${RESET}         ${CYAN}${BOLD}lux${RESET} installer                ${MAGENTA}${BOLD}│${RESET}"
echo -e "${MAGENTA}${BOLD}╰─────────────────────────────────────╯${RESET}"
echo ""

# Detect platform
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

# Map architecture names
case "$ARCH" in
    x86_64)
        ARCH="x86_64"
        ;;
    aarch64|arm64)
        ARCH="aarch64"
        ;;
    *)
        echo -e "${RED}${BOLD}✗${RESET} Unsupported architecture: ${BOLD}$ARCH${RESET}"
        exit 1
        ;;
esac

# Map OS names
case "$OS" in
    darwin)
        PLATFORM="apple-darwin"
        ;;
    linux)
        PLATFORM="unknown-linux-gnu"
        ;;
    *)
        echo -e "${RED}${BOLD}✗${RESET} Unsupported OS: ${BOLD}$OS${RESET}"
        echo -e "   Supported: macOS (darwin), Linux"
        exit 1
        ;;
esac

TARGET="${ARCH}-${PLATFORM}"
echo -e "Detected platform: ${CYAN}${BOLD}$TARGET${RESET}"
echo ""

# Get the latest release tag
echo -e "${DIM}Fetching latest release...${RESET}"
API_RESPONSE=$(curl -s "https://api.github.com/repos/${REPO}/releases/latest")
LATEST_TAG=$(echo "$API_RESPONSE" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/' || true)

if [ -z "$LATEST_TAG" ]; then
    echo -e "${RED}${BOLD}✗${RESET} Could not fetch latest release"
    echo ""
    if echo "$API_RESPONSE" | grep -q '"message": "Not Found"'; then
        echo -e "   No releases found. Please check ${BLUE}https://github.com/${REPO}/releases${RESET}"
    else
        echo "   API Response:"
        echo "$API_RESPONSE" | head -5
    fi
    exit 1
fi

echo -e "Latest version: ${GREEN}${BOLD}$LATEST_TAG${RESET}"
TARGET_VERSION="${LATEST_TAG#v}"
echo ""

# Helper function to compare versions (returns 0 if v1 > v2)
version_gt() {
    test "$(printf '%s\n' "$@" | sort -V | head -n 1)" != "$1"
}

# Show changelog for upgrades
show_changelog() {
    local from_version="$1"
    local to_version="$2"

    CHANGELOG_URL="https://raw.githubusercontent.com/${REPO}/main/CHANGELOG.md"
    CHANGELOG_CONTENT=$(curl -s "$CHANGELOG_URL" 2>/dev/null || true)

    if [ -z "$CHANGELOG_CONTENT" ]; then
        return
    fi

    echo -e "${CYAN}${BOLD}What's new since v${from_version}:${RESET}"
    echo ""

    local printing=false
    local found_any=false

    while IFS= read -r line; do
        if echo "$line" | grep -qE '^\#\# \[[0-9]+\.[0-9]+\.[0-9]+\]'; then
            version=$(echo "$line" | grep -oE '\[([0-9]+\.[0-9]+\.[0-9]+)\]' | tr -d '[]')

            if [ "$version" = "$from_version" ]; then
                printing=false
                break
            fi

            if version_gt "$version" "$from_version" 2>/dev/null || [ "$version" = "$to_version" ]; then
                printing=true
                found_any=true
                echo -e "${BOLD}${line}${RESET}"
            else
                printing=false
            fi
        elif [ "$printing" = true ]; then
            if echo "$line" | grep -qE '^\#\#\# '; then
                echo -e "${YELLOW}${line}${RESET}"
            elif [ -n "$line" ]; then
                echo "  $line"
            else
                echo ""
            fi
        fi
    done <<< "$CHANGELOG_CONTENT"

    if [ "$found_any" = true ]; then
        echo ""
    fi
}

# Check installed version
INSTALLED_VERSION=""
EXISTING_BIN=$(command -v "$BINARY_NAME" 2>/dev/null || true)
if [ -n "$EXISTING_BIN" ]; then
    INSTALLED_VERSION=$("$EXISTING_BIN" --version 2>/dev/null | awk '{print $2}' || true)
fi

if [ -n "$INSTALLED_VERSION" ]; then
    if [ "$INSTALLED_VERSION" = "$TARGET_VERSION" ]; then
        echo -e "${DIM}Currently installed:${RESET}"
        echo -e "  lux: ${GREEN}v${INSTALLED_VERSION}${RESET} ${DIM}(up to date)${RESET}"
        echo ""
        echo -e "${GREEN}${BOLD}✓${RESET} Already up to date!"
        echo ""
        exit 0
    else
        echo -e "${DIM}Currently installed:${RESET}"
        echo -e "  lux: ${YELLOW}v${INSTALLED_VERSION}${RESET} ${DIM}(update available)${RESET}"
        echo ""
        show_changelog "$INSTALLED_VERSION" "$TARGET_VERSION"
    fi
fi

# Check for existing installation in different location
EXISTING_PATH=""
if [ -n "$EXISTING_BIN" ]; then
    EXISTING_PATH=$(realpath "$EXISTING_BIN" 2>/dev/null || echo "$EXISTING_BIN")
    EXISTING_DIR=$(dirname "$EXISTING_PATH")

    if [ "$EXISTING_DIR" != "$INSTALL_DIR" ]; then
        echo -e "${YELLOW}${BOLD}⚠  Found existing installation in different location:${RESET}"
        echo ""
        echo -e "   ${BOLD}lux${RESET}: ${DIM}$EXISTING_PATH${RESET}"
        echo ""
        echo -e "New install directory: ${CYAN}$INSTALL_DIR${RESET}"
        echo ""
        echo -e "${BOLD}What would you like to do?${RESET}"
        echo ""
        echo -e "  ${CYAN}1)${RESET} Delete old binary and install to $INSTALL_DIR ${DIM}(default)${RESET}"
        echo -e "  ${CYAN}2)${RESET} Install to existing location instead ${DIM}($EXISTING_DIR)${RESET}"
        echo -e "  ${CYAN}3)${RESET} Keep both ${DIM}(install to $INSTALL_DIR anyway)${RESET}"
        echo -e "  ${CYAN}4)${RESET} Cancel installation"
        echo ""
        if [ -t 0 ]; then
            read -p "Choose [1/2/3/4] (default: 1): " -n 1 -r EXISTING_CHOICE
        else
            read -p "Choose [1/2/3/4] (default: 1): " -n 1 -r EXISTING_CHOICE </dev/tty
        fi
        echo ""
        echo ""

        case "$EXISTING_CHOICE" in
            2)
                INSTALL_DIR="$EXISTING_DIR"
                echo -e "${BLUE}→${RESET} Installing to existing location: ${CYAN}$INSTALL_DIR${RESET}"
                ;;
            3)
                echo -e "${BLUE}→${RESET} Installing to ${CYAN}$INSTALL_DIR${RESET} ${DIM}(keeping existing binary)${RESET}"
                ;;
            4)
                echo -e "${YELLOW}Installation cancelled.${RESET}"
                exit 0
                ;;
            *)
                echo -e "${RED}→${RESET} Removing old binary: ${DIM}$EXISTING_PATH${RESET}"
                if [ -w "$EXISTING_DIR" ]; then
                    rm -f "$EXISTING_PATH"
                else
                    sudo rm -f "$EXISTING_PATH"
                fi
                echo ""
                ;;
        esac
    fi
fi

# Ensure install directory exists
mkdir -p "$INSTALL_DIR"

# Check if we need sudo
NEED_SUDO=false
if [ ! -w "$INSTALL_DIR" ]; then
    NEED_SUDO=true
    echo -e "${DIM}Note: Will need sudo to install to $INSTALL_DIR${RESET}"
    echo ""
fi

DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/${BINARY_NAME}-${TARGET}.tar.gz"

echo -e "${BLUE}↓${RESET} Downloading ${BOLD}${BINARY_NAME}${RESET}..."

TEMP_DIR=$(mktemp -d)
TEMP_ARCHIVE="${TEMP_DIR}/archive.tar.gz"

if ! curl -L -f -o "$TEMP_ARCHIVE" "$DOWNLOAD_URL" 2>/dev/null; then
    echo -e "${RED}${BOLD}✗${RESET} Download failed"
    echo -e "   ${DIM}URL: $DOWNLOAD_URL${RESET}"
    echo -e "   ${DIM}This might mean no binary exists for your platform ($TARGET)${RESET}"
    rm -rf "$TEMP_DIR"
    exit 1
fi

echo -e "${BLUE}↓${RESET} Extracting ${BOLD}${BINARY_NAME}${RESET}..."
tar -xzf "$TEMP_ARCHIVE" -C "$TEMP_DIR" || {
    echo -e "${RED}${BOLD}✗${RESET} Failed to extract archive"
    rm -rf "$TEMP_DIR"
    exit 1
}

EXTRACTED_BINARY="${TEMP_DIR}/${BINARY_NAME}"
DEST="${INSTALL_DIR}/${BINARY_NAME}"

if [ ! -f "$EXTRACTED_BINARY" ]; then
    echo -e "${RED}${BOLD}✗${RESET} Binary not found in archive"
    rm -rf "$TEMP_DIR"
    exit 1
fi

if [ "$NEED_SUDO" = true ]; then
    sudo mv "$EXTRACTED_BINARY" "$DEST"
    sudo chmod +x "$DEST"
else
    mv "$EXTRACTED_BINARY" "$DEST"
    chmod +x "$DEST"
fi

rm -rf "$TEMP_DIR"

echo -e "${GREEN}${BOLD}✓${RESET} Installed ${BOLD}${BINARY_NAME}${RESET} to ${CYAN}${DEST}${RESET}"
echo ""

echo -e "${GREEN}${BOLD}╭─────────────────────────────────────╮${RESET}"
echo -e "${GREEN}${BOLD}│${RESET}       ${GREEN}${BOLD}Installation complete!${RESET}        ${GREEN}${BOLD}│${RESET}"
echo -e "${GREEN}${BOLD}╰─────────────────────────────────────╯${RESET}"
echo ""

# Check if install dir is in PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo -e "${YELLOW}${BOLD}⚠  $INSTALL_DIR is not in your PATH${RESET}"
    echo ""
    echo -e "Add it to your shell config:"
    echo -e "  ${DIM}echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc${RESET}"
    echo -e "  ${DIM}# or for zsh:${RESET}"
    echo -e "  ${DIM}echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.zshrc${RESET}"
    echo ""
    echo -e "Then restart your terminal or run: ${CYAN}source ~/.bashrc${RESET} (or ${CYAN}~/.zshrc${RESET})"
    echo ""
fi

echo -e "Run '${CYAN}${BOLD}lux --help${RESET}' to get started"
echo ""
echo -e "Documentation: ${BLUE}https://github.com/${REPO}${RESET}"
echo ""
