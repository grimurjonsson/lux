default:
    @just --list

build-install: build install
    @echo "Done."

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
        else
            cross build --release --target "$target"
        fi

        cp "target/$target/release/lux" "release-binaries/lux-$target"
        echo "✓ Built: release-binaries/lux-$target"
        echo ""
    done

    echo "✓ All binaries built successfully"
    echo ""
    echo "Binaries are in the release-binaries/ directory:"
    ls -lh release-binaries/
    echo ""
    echo "Upload these to your GitHub release"

# Bump patch version (0.1.0 → 0.1.1). Pass -y to skip prompts.
release-patch *args="": (_release "patch" args)

# Bump minor version (0.1.0 → 0.2.0). Pass -y to skip prompts.
release-minor *args="": (_release "minor" args)

# Bump major version (0.1.0 → 1.0.0). Pass -y to skip prompts.
release-major *args="": (_release "major" args)

# Undo a cancelled release: delete tag + branch, restore Cargo.toml, checkout main
release-abort:
    #!/usr/bin/env bash
    set -euo pipefail

    CURRENT=$(git branch --show-current)

    if [[ ! "$CURRENT" =~ ^release/ ]]; then
        echo "Not on a release branch (on '$CURRENT'). Nothing to abort."
        exit 1
    fi

    VERSION="${CURRENT#release/}"
    echo "Aborting release $VERSION..."

    # Delete the tag if it exists locally
    if git tag -l "$VERSION" | grep -q .; then
        git tag -d "$VERSION"
        echo "✓ Deleted local tag $VERSION"
    fi

    # Delete the remote tag if it was pushed
    if git ls-remote --tags origin "$VERSION" | grep -q .; then
        read -p "Remote tag $VERSION exists. Delete it? [y/N] " -n 1 -r
        echo
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            git push origin ":refs/tags/$VERSION"
            echo "✓ Deleted remote tag $VERSION"
        fi
    fi

    # Switch to main and restore Cargo.toml/CHANGELOG.md
    git checkout main
    git checkout -- Cargo.toml CHANGELOG.md Cargo.lock 2>/dev/null || true
    echo "✓ Switched to main, restored versioned files"

    # Delete the release branch
    git branch -D "$CURRENT"
    echo "✓ Deleted branch $CURRENT"

    echo ""
    echo "Release aborted. You're back on main."

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
        PROMPT="Write a concise TL;DR (one sentence) for these changelog commits. Focus only on user-facing changes. No quotes or prefix. Commits: $CHANGES"
        TLDR=$(claude -p "$PROMPT" 2>/dev/null || true)
        echo "TL;DR: $TLDR"
        echo ""
    else
        echo "=== Claude not available or no changes, skipping TL;DR ==="
        echo ""
    fi

    ADDED=$(echo "$CHANGES" | grep -iE '^- (feat|add)' | sed 's/^- [Ff][Ee][Aa][Tt][:(] */- /; s/^- [Aa][Dd][Dd][:(] */- /' || true)
    FIXED=$(echo "$CHANGES" | grep -iE '^- fix' | sed 's/^- [Ff][Ii][Xx][:(] */- /' || true)
    CHANGED=$(echo "$CHANGES" | grep -iE '^- (refactor|change|update)' | sed 's/^- [Rr][Ee][Ff][Aa][Cc][Tt][Oo][Rr][:(] */- /; s/^- [Cc][Hh][Aa][Nn][Gg][Ee][:(] */- /; s/^- [Uu][Pp][Dd][Aa][Tt][Ee][:(] */- /' || true)
    OTHER=$(echo "$CHANGES" | grep -ivE '^- (feat|add|fix|refactor|change|update)' | grep -v '^$' || true)

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
    if [ -n "$OTHER" ]; then
        echo "### Other"
        echo "$OTHER"
        echo ""
    fi

_release bump *args="":
    #!/usr/bin/env bash
    set -euo pipefail

    AUTO_YES=false
    MSG=""
    for arg in {{ args }}; do
        case "$arg" in
            -y|--yes) AUTO_YES=true ;;
            *) MSG="$arg" ;;
        esac
    done

    confirm() {
        if [ "$AUTO_YES" = true ]; then
            return 0
        fi
        read -p "$1 [Y/n] " -n 1 -r
        echo
        [[ ! $REPLY =~ ^[Nn]$ ]]
    }

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

        # If no commits found but a release message was provided, use it
        if [ -z "$CHANGES" ] && [ -n "$MSG" ]; then
            CHANGES="- $MSG"
        fi

        TLDR=""
        if command -v claude &> /dev/null && [ -n "$CHANGES" ]; then
            PROMPT="Write a concise TL;DR (one sentence) for these changelog commits. Focus only on user-facing changes. No quotes or prefix. Commits: $CHANGES"
            TLDR=$(claude -p "$PROMPT" 2>/dev/null || true)
        fi

        ADDED=$(echo "$CHANGES" | grep -iE '^- (feat|add)' | sed 's/^- [Ff][Ee][Aa][Tt][:(] */- /; s/^- [Aa][Dd][Dd][:(] */- /' || true)
        FIXED=$(echo "$CHANGES" | grep -iE '^- fix' | sed 's/^- [Ff][Ii][Xx][:(] */- /' || true)
        CHANGED=$(echo "$CHANGES" | grep -iE '^- (refactor|change|update)' | sed 's/^- [Rr][Ee][Ff][Aa][Cc][Tt][Oo][Rr][:(] */- /; s/^- [Cc][Hh][Aa][Nn][Gg][Ee][:(] */- /; s/^- [Uu][Pp][Dd][Aa][Tt][Ee][:(] */- /' || true)
        # Catch uncategorized commits (don't match feat/add/fix/refactor/change/update)
        OTHER=$(echo "$CHANGES" | grep -ivE '^- (feat|add|fix|refactor|change|update)' | grep -v '^$' || true)

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

        # Include uncategorized commits so nothing is silently dropped
        if [ -n "$OTHER" ]; then
            printf '%s\n%s\n\n' "### Other" "$OTHER" >> "$TMPFILE"
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

    # Update README.md reference section from --help output
    README="README.md"
    if [ -f "$README" ] && grep -q 'BEGIN REFERENCE' "$README"; then
        cargo build --quiet --release 2>/dev/null || cargo build --quiet
        HELP=$(cargo run --quiet -- --help 2>/dev/null || target/release/lux --help)
        # Extract subcommands summary
        SUBCMDS=$(cargo run --quiet -- help 2>/dev/null | grep '^ ' | grep -v '^  help' || true)

        BEGIN=$(grep -n 'BEGIN REFERENCE' "$README" | head -1 | cut -d: -f1)
        END=$(grep -n 'END REFERENCE' "$README" | head -1 | cut -d: -f1)
        if [ -n "$BEGIN" ] && [ -n "$END" ]; then
            OUTFILE=$(mktemp)
            head -n "$BEGIN" "$README" > "$OUTFILE"
            printf '```\n%s\n```\n' "$HELP" >> "$OUTFILE"
            tail -n +"$END" "$README" >> "$OUTFILE"
            mv "$OUTFILE" "$README"
            echo "✓ Updated README.md reference section"
        fi
    fi

    if confirm "Create release branch, commit, and tag?"; then
        git checkout -b "$RELEASE_BRANCH"
        echo "✓ Created branch $RELEASE_BRANCH"

        git add Cargo.toml Cargo.lock
        if [ -f "$CHANGELOG_FILE" ]; then
            git add "$CHANGELOG_FILE"
        fi
        if [ -f "$README" ]; then
            git add "$README"
        fi
        if [ -n "$MSG" ]; then
            git commit -m "Release v$NEW_VERSION" -m "$MSG"
        else
            git commit -m "Release v$NEW_VERSION"
        fi
        git tag "v$NEW_VERSION"
        echo "✓ Created commit and tag v$NEW_VERSION"

        if confirm "Push branch and tag, then create PR?"; then
            git push -u origin "$RELEASE_BRANCH"
            git push origin "v$NEW_VERSION"
            echo "✓ Pushed branch and tag to origin"
            echo ""
            echo "The tag push will trigger the release workflow."
            echo ""

            if command -v gh &> /dev/null; then
                if confirm "Create PR to merge release branch to main?"; then
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

                    if confirm "Merge the PR now?"; then
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
