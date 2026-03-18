# Release Workflow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add version bumping (patch/minor/major), changelog tracking, install script, and GitHub Actions release workflow — matching the to-tui project's release setup.

**Architecture:** Justfile commands drive the release process: bump version in Cargo.toml, auto-generate CHANGELOG.md entries from git commits, create release branch/tag/PR. A `scripts/install.sh` downloads pre-built binaries from GitHub releases. GitHub Actions builds cross-platform binaries on tag push.

**Tech Stack:** just (task runner), bash, GitHub Actions, cross (cross-compilation), gh CLI

---

### Task 1: Create CHANGELOG.md

**Files:**
- Create: `CHANGELOG.md`

- [ ] **Step 1: Create the changelog file**

```markdown
# Changelog

All notable changes to lux will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-03-18

### Added
- Initial release: colored log output with regex pattern matching
- Built-in log level defaults (error, warn, info, debug, trace)
- TOML configuration with named profiles
- Profile auto-selection by filename
- File following by descriptor (-f) and by name (-F) with rotation detection
- Line filtering (--include/--exclude)
- Trigger patterns with context buffer (-t, -b, -a)
- Interactive profile creation wizard
- Syntax highlighting via syntect
- Shell completions (bash, zsh, fish)
- RGB/hex colors and text styles (bold, italic, underline, dim)
- ANSI input stripping
```

- [ ] **Step 2: Commit**

```bash
git add CHANGELOG.md
git commit -m "docs: add CHANGELOG.md"
```

---

### Task 2: Create install script

**Files:**
- Create: `scripts/install.sh`

This is adapted from to-tui's install script, simplified for a single binary (`lux`) with no plugin/marketplace/migration logic.

- [ ] **Step 1: Create the install script**

```bash
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

# Map OS names and set binary extension
BINARY_EXT=""
case "$OS" in
    darwin)
        PLATFORM="apple-darwin"
        ;;
    linux)
        PLATFORM="unknown-linux-gnu"
        ;;
    mingw*|msys*|cygwin*)
        PLATFORM="pc-windows-gnu"
        BINARY_EXT=".exe"
        ;;
    *)
        echo -e "${RED}${BOLD}✗${RESET} Unsupported OS: ${BOLD}$OS${RESET}"
        echo -e "   Supported: macOS (darwin), Linux, Windows"
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

# Determine archive extension
if [ -z "$BINARY_EXT" ]; then
    ARCHIVE_EXT=".tar.gz"
else
    ARCHIVE_EXT=".zip"
fi

DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/${BINARY_NAME}-${TARGET}${ARCHIVE_EXT}"

echo -e "${BLUE}↓${RESET} Downloading ${BOLD}${BINARY_NAME}${RESET}..."

TEMP_DIR=$(mktemp -d)
TEMP_ARCHIVE="${TEMP_DIR}/archive${ARCHIVE_EXT}"

if ! curl -L -f -o "$TEMP_ARCHIVE" "$DOWNLOAD_URL" 2>/dev/null; then
    echo -e "${RED}${BOLD}✗${RESET} Download failed"
    echo -e "   ${DIM}URL: $DOWNLOAD_URL${RESET}"
    echo -e "   ${DIM}This might mean no binary exists for your platform ($TARGET)${RESET}"
    rm -rf "$TEMP_DIR"
    exit 1
fi

echo -e "${BLUE}↓${RESET} Extracting ${BOLD}${BINARY_NAME}${RESET}..."
if [ -z "$BINARY_EXT" ]; then
    tar -xzf "$TEMP_ARCHIVE" -C "$TEMP_DIR" || {
        echo -e "${RED}${BOLD}✗${RESET} Failed to extract archive"
        rm -rf "$TEMP_DIR"
        exit 1
    }
else
    unzip -q "$TEMP_ARCHIVE" -d "$TEMP_DIR" || {
        echo -e "${RED}${BOLD}✗${RESET} Failed to extract archive"
        rm -rf "$TEMP_DIR"
        exit 1
    }
fi

EXTRACTED_BINARY="${TEMP_DIR}/${BINARY_NAME}${BINARY_EXT}"
DEST="${INSTALL_DIR}/${BINARY_NAME}${BINARY_EXT}"

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
```

- [ ] **Step 2: Make it executable**

```bash
chmod +x scripts/install.sh
```

- [ ] **Step 3: Commit**

```bash
git add scripts/install.sh
git commit -m "feat: add install script for pre-built binaries"
```

---

### Task 3: Create GitHub Actions release workflow

**Files:**
- Create: `.github/workflows/release.yml`

- [ ] **Step 1: Create the workflow file**

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

permissions:
  contents: write

jobs:
  build-and-release:
    name: Build and Release
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          # Linux builds
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            use_cross: true
          - os: ubuntu-latest
            target: aarch64-unknown-linux-gnu
            use_cross: true

          # macOS builds
          - os: macos-latest
            target: x86_64-apple-darwin
            use_cross: false
          - os: macos-latest
            target: aarch64-apple-darwin
            use_cross: false

          # Windows build
          - os: ubuntu-latest
            target: x86_64-pc-windows-gnu
            use_cross: true

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - name: Install cross
        if: matrix.use_cross
        run: cargo install cross --git https://github.com/cross-rs/cross

      - name: Build with cross
        if: matrix.use_cross
        run: cross build --release --target ${{ matrix.target }}

      - name: Build with cargo
        if: ${{ !matrix.use_cross }}
        run: cargo build --release --target ${{ matrix.target }}

      - name: Prepare binaries (Unix)
        if: ${{ !contains(matrix.target, 'windows') }}
        run: |
          mkdir -p release-assets
          tar -czvf release-assets/lux-${{ matrix.target }}.tar.gz -C target/${{ matrix.target }}/release lux

      - name: Prepare binaries (Windows)
        if: contains(matrix.target, 'windows')
        run: |
          mkdir -p release-assets
          (cd target/${{ matrix.target }}/release && zip ../../../release-assets/lux-${{ matrix.target }}.zip lux.exe)

      - name: Upload binaries to release
        uses: softprops/action-gh-release@v2
        with:
          files: release-assets/*
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add GitHub Actions release workflow for cross-platform builds"
```

---

### Task 4: Expand justfile with release commands

**Files:**
- Modify: `justfile`

Replace the entire justfile with the expanded version including: `default`, `build`, `install` (with dev version), `install-with-curl`, `install-completions`, `test`, `build-release-binaries`, `release-patch`, `release-minor`, `release-major`, `generate-changelog-test`, and `_release` helper.

- [ ] **Step 1: Write the expanded justfile**

```justfile
default:
    @just --list

# Build release binary
build:
    cargo build --release

# Build and install to ~/.local/bin (dev version with timestamp)
install:
    #!/usr/bin/env bash
    set -euo pipefail

    # Check for required dependencies
    echo "Checking dependencies..."

    if ! command -v cargo &> /dev/null; then
        echo "❌ cargo not found"
        echo ""
        echo "Install Rust and cargo from: https://rustup.rs/"
        echo "Run: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        exit 1
    fi
    echo "✓ cargo found: $(cargo --version)"

    if ! command -v rustc &> /dev/null; then
        echo "❌ rustc not found"
        echo ""
        echo "Install Rust from: https://rustup.rs/"
        exit 1
    fi
    echo "✓ rustc found: $(rustc --version)"

    echo ""

    # Get current version and create dev version with timestamp
    ORIGINAL_VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
    DEV_TIMESTAMP=$(date +%Y%m%d-%H%M%S)
    DEV_VERSION="${ORIGINAL_VERSION}-dev-${DEV_TIMESTAMP}"

    echo "Building dev version: v${DEV_VERSION}"
    echo ""

    # Temporarily modify Cargo.toml with dev version
    sed -i '' "s/^version = \".*\"/version = \"$DEV_VERSION\"/" Cargo.toml

    # Ensure we restore the original version even if build fails
    cleanup() {
        sed -i '' "s/^version = \".*\"/version = \"$ORIGINAL_VERSION\"/" Cargo.toml
        cargo generate-lockfile --quiet 2>/dev/null || true
    }
    trap cleanup EXIT

    echo "Building release binary..."
    cargo build --release

    INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
    BINARY_NAME="lux"

    # Check for existing installation in different location
    EXISTING_PATH=$(command -v "$BINARY_NAME" 2>/dev/null || true)
    if [ -n "$EXISTING_PATH" ]; then
        EXISTING_PATH=$(realpath "$EXISTING_PATH" 2>/dev/null || echo "$EXISTING_PATH")
        EXISTING_DIR=$(dirname "$EXISTING_PATH")

        if [ "$EXISTING_DIR" != "$INSTALL_DIR" ]; then
            echo ""
            echo "⚠️  Found existing installation in different location:"
            echo "   $BINARY_NAME: $EXISTING_PATH"
            echo ""
            echo "New install directory: $INSTALL_DIR"
            echo ""
            echo "What would you like to do?"
            echo ""
            echo "  1) Delete old binary and install to $INSTALL_DIR (default)"
            echo "  2) Install to existing location instead ($EXISTING_DIR)"
            echo "  3) Keep both (install to $INSTALL_DIR anyway)"
            echo "  4) Cancel installation"
            echo ""
            read -p "Choose [1/2/3/4] (default: 1): " -n 1 -r EXISTING_CHOICE
            echo ""
            echo ""

            case "$EXISTING_CHOICE" in
                2)
                    INSTALL_DIR="$EXISTING_DIR"
                    echo "Installing to existing location: $INSTALL_DIR"
                    ;;
                3)
                    echo "Installing to $INSTALL_DIR (keeping existing binary)"
                    ;;
                4)
                    echo "Installation cancelled."
                    exit 0
                    ;;
                *)
                    echo "Removing old binary: $EXISTING_PATH"
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

    BINARY_SRC="$(pwd)/target/release/$BINARY_NAME"
    BINARY_DST="$INSTALL_DIR/$BINARY_NAME"

    # Check if we need sudo
    NEED_SUDO=false
    if [ ! -w "$INSTALL_DIR" ]; then
        NEED_SUDO=true
        echo "Note: Will need sudo to install to $INSTALL_DIR"
        echo ""
    fi

    if [ -f "$BINARY_DST" ] && cmp -s "$BINARY_SRC" "$BINARY_DST"; then
        echo "✓ $BINARY_NAME already installed and up to date"
    else
        if [ "$NEED_SUDO" = true ]; then
            sudo cp "$BINARY_SRC" "$BINARY_DST"
            sudo chmod +x "$BINARY_DST"
            sudo codesign -s - --force "$BINARY_DST" 2>/dev/null || true
        else
            cp "$BINARY_SRC" "$BINARY_DST"
            chmod +x "$BINARY_DST"
            codesign -s - --force "$BINARY_DST" 2>/dev/null || true
        fi
        echo "✓ Installed $BINARY_NAME to $BINARY_DST"
    fi

    echo ""

    # Check if install dir is in PATH
    if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
        echo "⚠️  $INSTALL_DIR is not in your PATH"
        echo ""
        echo "Add it to your shell config:"
        echo "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc"
        echo "  # or for zsh:"
        echo "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.zshrc"
        echo ""
        echo "Then restart your terminal or run: source ~/.bashrc (or ~/.zshrc)"
        echo ""
    fi

    echo "Run 'lux --help' to get started"

# Install via curl from GitHub
install-with-curl:
    curl -fsSL https://raw.githubusercontent.com/grimurjonsson/lux/main/scripts/install.sh | bash

# Install shell completions for zsh
install-completions:
    mkdir -p ~/.zsh/completions
    cargo run -- completions zsh > ~/.zsh/completions/_lux
    @echo "Done. Make sure ~/.zsh/completions is in your fpath."
    @echo "Add to ~/.zshrc if not already:  fpath=(~/.zsh/completions \$fpath)"

# Run all tests
test:
    cargo test

# Build release binaries for all platforms (requires cross)
build-release-binaries:
    #!/usr/bin/env bash
    set -euo pipefail

    echo "Building release binaries for multiple platforms..."
    echo ""

    if ! command -v cross &> /dev/null; then
        echo "❌ 'cross' is not installed"
        echo ""
        echo "Install cross with: cargo install cross"
        exit 1
    fi

    TARGETS=(
        "x86_64-unknown-linux-gnu"
        "aarch64-unknown-linux-gnu"
        "x86_64-apple-darwin"
        "aarch64-apple-darwin"
        "x86_64-pc-windows-gnu"
    )

    echo "Ensuring all targets are installed..."
    for target in "${TARGETS[@]}"; do
        rustup target add "$target" 2>/dev/null || true
    done
    echo ""

    mkdir -p release-binaries

    for target in "${TARGETS[@]}"; do
        echo "Building for $target..."

        if [[ "$target" == *"apple-darwin"* ]]; then
            cargo build --release --target "$target"
            binary_ext=""
        elif [[ "$target" == *"windows"* ]]; then
            cross build --release --target "$target"
            binary_ext=".exe"
        else
            cross build --release --target "$target"
            binary_ext=""
        fi

        cp "target/$target/release/lux${binary_ext}" "release-binaries/lux-$target${binary_ext}"
        echo "✓ Built: release-binaries/lux-$target${binary_ext}"
        echo ""
    done

    echo "✓ All binaries built successfully"
    echo ""
    echo "Binaries are in the release-binaries/ directory:"
    ls -lh release-binaries/
    echo ""
    echo "Upload these to your GitHub release"

# Bump patch version (0.1.0 → 0.1.1)
release-patch msg="": (_release "patch" msg)

# Bump minor version (0.1.0 → 0.2.0)
release-minor msg="": (_release "minor" msg)

# Bump major version (0.1.0 → 1.0.0)
release-major msg="": (_release "major" msg)

# Test changelog generation (dry-run, prints to stdout)
generate-changelog-test:
    #!/usr/bin/env bash
    set -euo pipefail

    VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
    LAST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "")
    TODAY=$(date +%Y-%m-%d)

    echo "=== Changelog Test ==="
    echo "Current version: $VERSION"
    echo "Last tag: ${LAST_TAG:-none}"
    echo ""

    if [ -n "$LAST_TAG" ]; then
        CHANGES=$(git log "$LAST_TAG"..HEAD --pretty=format:"- %s" --no-merges | grep -v "^- Release v" || true)
    else
        CHANGES=$(git log --pretty=format:"- %s" --no-merges | grep -v "^- Release v" || true)
    fi

    echo "=== Raw commits ==="
    echo "$CHANGES"
    echo ""

    TLDR=""
    if command -v claude &> /dev/null && [ -n "$CHANGES" ]; then
        echo "=== Generating TL;DR with Claude... ==="
        PROMPT="Write a concise TL;DR for these changelog commits. Focus only on user-facing changes. No quotes or prefix. Commits: $CHANGES"
        TLDR=$(claude -p "$PROMPT" 2>/dev/null || true)
        echo "TL;DR: $TLDR"
        echo ""
    else
        echo "=== Claude not available, skipping TL;DR ==="
        echo ""
    fi

    ADDED=$(echo "$CHANGES" | grep -iE '^- (feat|add)' | sed 's/^- [Ff][Ee][Aa][Tt][:(] */- /; s/^- [Aa][Dd][Dd][:(] */- /' || true)
    FIXED=$(echo "$CHANGES" | grep -iE '^- fix' | sed 's/^- [Ff][Ii][Xx][:(] */- /' || true)
    CHANGED=$(echo "$CHANGES" | grep -iE '^- (refactor|change|update)' | sed 's/^- [Rr][Ee][Ff][Aa][Cc][Tt][Oo][Rr][:(] */- /; s/^- [Cc][Hh][Aa][Nn][Gg][Ee][:(] */- /; s/^- [Uu][Pp][Dd][Aa][Tt][Ee][:(] */- /' || true)

    echo "=== Generated changelog entry ==="
    echo "## [$VERSION] - $TODAY"
    if [ -n "$TLDR" ]; then
        echo "$TLDR"
        echo ""
    fi
    if [ -n "$ADDED" ]; then
        echo "### Added"
        echo "$ADDED"
        echo ""
    fi
    if [ -n "$FIXED" ]; then
        echo "### Fixed"
        echo "$FIXED"
        echo ""
    fi
    if [ -n "$CHANGED" ]; then
        echo "### Changed"
        echo "$CHANGED"
        echo ""
    fi

_release bump msg="":
    #!/usr/bin/env bash
    set -euo pipefail

    VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
    IFS='.' read -r MAJOR MINOR PATCH <<< "$VERSION"

    case "{{ bump }}" in
        patch) PATCH=$((PATCH + 1)) ;;
        minor) MINOR=$((MINOR + 1)); PATCH=0 ;;
        major) MAJOR=$((MAJOR + 1)); MINOR=0; PATCH=0 ;;
    esac

    NEW_VERSION="$MAJOR.$MINOR.$PATCH"
    RELEASE_BRANCH="release/v$NEW_VERSION"

    sed -i '' "s/^version = \".*\"/version = \"$NEW_VERSION\"/" Cargo.toml
    echo "✓ Cargo.toml version: $VERSION → $NEW_VERSION"

    # Update CHANGELOG.md with git changes since last tag
    CHANGELOG_FILE="CHANGELOG.md"
    if [ -f "$CHANGELOG_FILE" ]; then
        LAST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "")
        TODAY=$(date +%Y-%m-%d)

        if [ -n "$LAST_TAG" ]; then
            CHANGES=$(git log "$LAST_TAG"..HEAD --pretty=format:"- %s" --no-merges | grep -v "^- Release v" || true)
        else
            CHANGES=$(git log --pretty=format:"- %s" --no-merges | grep -v "^- Release v" || true)
        fi

        TLDR=""
        if command -v claude &> /dev/null && [ -n "$CHANGES" ]; then
            PROMPT="Write a concise TL;DR for these changelog commits. Focus only on user-facing changes. No quotes or prefix. Commits: $CHANGES"
            TLDR=$(claude -p "$PROMPT" 2>/dev/null || true)
        fi

        ADDED=$(echo "$CHANGES" | grep -iE '^- (feat|add)' | sed 's/^- [Ff][Ee][Aa][Tt][:(] */- /; s/^- [Aa][Dd][Dd][:(] */- /' || true)
        FIXED=$(echo "$CHANGES" | grep -iE '^- fix' | sed 's/^- [Ff][Ii][Xx][:(] */- /' || true)
        CHANGED=$(echo "$CHANGES" | grep -iE '^- (refactor|change|update)' | sed 's/^- [Rr][Ee][Ff][Aa][Cc][Tt][Oo][Rr][:(] */- /; s/^- [Cc][Hh][Aa][Nn][Gg][Ee][:(] */- /; s/^- [Uu][Pp][Dd][Aa][Tt][Ee][:(] */- /' || true)

        TMPFILE=$(mktemp)
        printf '%s\n' "## [$NEW_VERSION] - $TODAY" >> "$TMPFILE"

        if [ -n "$TLDR" ]; then
            printf '%s\n\n' "$TLDR" >> "$TMPFILE"
        fi

        if [ -n "$ADDED" ]; then
            printf '%s\n%s\n\n' "### Added" "$ADDED" >> "$TMPFILE"
        fi

        if [ -n "$FIXED" ]; then
            printf '%s\n%s\n\n' "### Fixed" "$FIXED" >> "$TMPFILE"
        fi

        if [ -n "$CHANGED" ]; then
            printf '%s\n%s\n\n' "### Changed" "$CHANGED" >> "$TMPFILE"
        fi

        HEADER_END=$(grep -n '^\#\# \[' "$CHANGELOG_FILE" | head -1 | cut -d: -f1)
        if [ -n "$HEADER_END" ]; then
            OUTFILE=$(mktemp)
            head -n $((HEADER_END - 1)) "$CHANGELOG_FILE" > "$OUTFILE"
            cat "$TMPFILE" >> "$OUTFILE"
            tail -n +$HEADER_END "$CHANGELOG_FILE" >> "$OUTFILE"
            mv "$OUTFILE" "$CHANGELOG_FILE"
            echo "✓ Updated CHANGELOG.md with v$NEW_VERSION"
        fi
        rm -f "$TMPFILE"
    fi

    # Update Cargo.lock with new version
    cargo check --quiet

    read -p "Create release branch, commit, and tag? [Y/n] " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Nn]$ ]]; then
        git checkout -b "$RELEASE_BRANCH"
        echo "✓ Created branch $RELEASE_BRANCH"

        git add Cargo.toml Cargo.lock
        if [ -f "$CHANGELOG_FILE" ]; then
            git add "$CHANGELOG_FILE"
        fi
        if [ -n "{{ msg }}" ]; then
            git commit -m "Release v$NEW_VERSION" -m "{{ msg }}"
        else
            git commit -m "Release v$NEW_VERSION"
        fi
        git tag "v$NEW_VERSION"
        echo "✓ Created commit and tag v$NEW_VERSION"

        read -p "Push branch and tag, then create PR? [Y/n] " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Nn]$ ]]; then
            git push -u origin "$RELEASE_BRANCH"
            git push origin "v$NEW_VERSION"
            echo "✓ Pushed branch and tag to origin"
            echo ""
            echo "The tag push will trigger the release workflow."
            echo ""

            if command -v gh &> /dev/null; then
                read -p "Create PR to merge release branch to main? [Y/n] " -n 1 -r
                echo
                if [[ ! $REPLY =~ ^[Nn]$ ]]; then
                    PR_BODY="Release v$NEW_VERSION

    This PR merges the release commit and updates:
    - Cargo.toml version bump
    - CHANGELOG.md updates

    The release workflow has already been triggered by the tag push."
                    PR_URL=$(gh pr create \
                        --title "Release v$NEW_VERSION" \
                        --body "$PR_BODY" \
                        --base main \
                        --head "$RELEASE_BRANCH")
                    echo "✓ Created PR: $PR_URL"
                    echo ""

                    read -p "Merge the PR now? [Y/n] " -n 1 -r
                    echo
                    if [[ ! $REPLY =~ ^[Nn]$ ]]; then
                        gh pr merge "$RELEASE_BRANCH" --merge --delete-branch
                        echo "✓ PR merged and release branch deleted"
                        git checkout main
                        git pull origin main
                        echo "✓ Switched to main and pulled latest"
                    fi
                fi
            else
                echo "gh CLI not found. Please create a PR manually to merge $RELEASE_BRANCH to main."
            fi
        fi
    fi
```

- [ ] **Step 2: Verify justfile syntax**

Run: `just --list`
Expected: All commands listed without errors

- [ ] **Step 3: Commit**

```bash
git add justfile
git commit -m "feat: add release workflow with version bumping, changelog, and cross-compilation"
```

---

### Task 5: Add release-binaries to .gitignore

**Files:**
- Modify: `.gitignore`

- [ ] **Step 1: Append release-binaries/ to .gitignore**

Add `/release-binaries/` to the existing `.gitignore`.

- [ ] **Step 2: Commit**

```bash
git add .gitignore
git commit -m "chore: ignore release-binaries directory"
```
