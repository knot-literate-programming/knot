#!/usr/bin/env bash
#
# Knot installer
# Installs the knot CLI, knot-lsp, and the VS Code extension.
#
# Usage:
#   curl -sSf https://raw.githubusercontent.com/knot-literate-programming/knot/master/install.sh | bash
#   bash install.sh [--prefix DIR]   (default: ~/.local/bin)
#

set -euo pipefail

REPO="knot-literate-programming/knot"
INSTALL_DIR="${KNOT_INSTALL_DIR:-$HOME/.local/bin}"

# ── Parse arguments ────────────────────────────────────────────────────────────
while [ $# -gt 0 ]; do
    case "$1" in
        --prefix)   shift; INSTALL_DIR="$1" ;;
        --prefix=*) INSTALL_DIR="${1#*=}" ;;
        --help|-h)
            echo "Usage: install.sh [--prefix DIR]"
            echo "  --prefix DIR   Install binaries to DIR (default: ~/.local/bin)"
            exit 0
            ;;
        *) echo "Unknown argument: $1" >&2; exit 1 ;;
    esac
    shift
done

# ── Terminal colors ────────────────────────────────────────────────────────────
if [ -t 1 ]; then
    RED='\033[0;31m'; YELLOW='\033[1;33m'; GREEN='\033[0;32m'
    CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'
else
    RED=''; YELLOW=''; GREEN=''; CYAN=''; BOLD=''; RESET=''
fi

step()  { printf "\n${BOLD}${CYAN}==> %s${RESET}\n" "$1"; }
ok()    { printf "    ${GREEN}✓${RESET} %s\n" "$1"; }
info()  { printf "      %s\n" "$1"; }
warn()  { printf "    ${YELLOW}!${RESET} %s\n" "$1"; }
err()   { printf "    ${RED}✗${RESET} %s\n" "$1" >&2; }
fatal() { err "$1"; exit 1; }

# ── Detect platform ────────────────────────────────────────────────────────────
step "Detecting platform"

OS=$(uname -s)
ARCH=$(uname -m)

case "$OS" in
    Darwin) OS_TRIPLE="apple-darwin" ;;
    Linux)  OS_TRIPLE="unknown-linux-gnu" ;;
    *) fatal "Unsupported OS: $OS (only macOS and Linux are supported)." ;;
esac

case "$ARCH" in
    x86_64)          ARCH_TRIPLE="x86_64" ;;
    arm64 | aarch64) ARCH_TRIPLE="aarch64" ;;
    *) fatal "Unsupported architecture: $ARCH." ;;
esac

TARGET="${ARCH_TRIPLE}-${OS_TRIPLE}"
ok "Platform: $TARGET"

# ── Choose download tool ───────────────────────────────────────────────────────
if command -v curl >/dev/null 2>&1; then
    fetch() { curl -sSfL "$1"; }
    fetch_to() { curl -sSfL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
    fetch() { wget -qO- "$1"; }
    fetch_to() { wget -qO "$2" "$1"; }
else
    fatal "Neither curl nor wget found. Please install one of them."
fi

# ── Fetch latest release metadata ─────────────────────────────────────────────
step "Fetching latest release"

RELEASE_JSON=$(fetch "https://api.github.com/repos/$REPO/releases/latest")
VERSION=$(printf '%s' "$RELEASE_JSON" \
    | grep '"tag_name"' | head -1 \
    | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')

[ -n "$VERSION" ] || fatal "Could not determine latest version. Check your internet connection."
ok "Version: $VERSION"

# Extract the download URL for an asset matching a pattern
asset_url() {
    printf '%s' "$RELEASE_JSON" \
        | grep '"browser_download_url"' \
        | grep "$1" \
        | head -1 \
        | sed 's/.*"browser_download_url": *"\([^"]*\)".*/\1/'
}

# ── Download and install binaries ──────────────────────────────────────────────
step "Installing binaries to $INSTALL_DIR"

mkdir -p "$INSTALL_DIR"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

install_binary() {
    local crate="$1"   # e.g. knot-cli
    local binary="$2"  # e.g. knot

    local url
    url=$(asset_url "${crate}.*${TARGET}")
    if [ -z "$url" ]; then
        warn "No prebuilt binary for ${crate} on ${TARGET}."
        info "Build from source: cargo install --path crates/${crate}"
        return
    fi

    info "Downloading ${binary}..."
    fetch_to "$url" "$TMP/${crate}.tar.gz"
    tar -xzf "$TMP/${crate}.tar.gz" -C "$TMP"

    local bin_path
    bin_path=$(find "$TMP" -name "$binary" -type f | head -1)
    if [ -z "$bin_path" ]; then
        warn "Binary '${binary}' not found in archive."
        return
    fi

    chmod +x "$bin_path"
    mv "$bin_path" "$INSTALL_DIR/$binary"
    ok "Installed: $INSTALL_DIR/$binary"
}

install_binary "knot-cli" "knot"
install_binary "knot-lsp" "knot-lsp"

# Warn if INSTALL_DIR is not in PATH
case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        warn "$INSTALL_DIR is not in your PATH."
        info "Add to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
        info "  export PATH=\"\$HOME/.local/bin:\$PATH\""
        ;;
esac

# ── Install VS Code extension ──────────────────────────────────────────────────
step "Installing VS Code extension"

if command -v code >/dev/null 2>&1; then
    VSIX_URL=$(asset_url "\.vsix")
    if [ -n "$VSIX_URL" ]; then
        info "Downloading extension..."
        fetch_to "$VSIX_URL" "$TMP/knot.vsix"
        code --install-extension "$TMP/knot.vsix" --force >/dev/null 2>&1
        ok "VS Code extension installed"
    else
        warn "No .vsix found in this release."
        info "Download manually: https://github.com/$REPO/releases/latest"
    fi
else
    warn "'code' command not found — skipping VS Code extension."
    info "To install manually:"
    info "  1. Download the .vsix from: https://github.com/$REPO/releases/latest"
    info "  2. In VS Code: Extensions panel → '...' menu → 'Install from VSIX...'"
    info "  macOS: run 'Shell Command: Install code command in PATH' in VS Code first."
fi

# ── Check prerequisites ────────────────────────────────────────────────────────
step "Checking prerequisites"

HAS_R=false
HAS_PYTHON=false
BLOCKING_OK=true

# Typst — required to compile documents
if command -v typst >/dev/null 2>&1; then
    ok "typst $(typst --version 2>/dev/null | head -1)"
else
    err "typst not found — Knot cannot compile documents without it."
    info "Install: https://github.com/typst/typst/releases"
    BLOCKING_OK=false
fi

# Tinymist — required for VS Code live preview
if command -v tinymist >/dev/null 2>&1; then
    ok "tinymist found"
else
    err "tinymist not found — VS Code preview will not work."
    info "Install: https://github.com/Myriad-Dreamin/tinymist/releases"
    info "Or install the 'Tinymist Typst' VS Code extension and add its binary to PATH."
    BLOCKING_OK=false
fi

# R — optional
if command -v Rscript >/dev/null 2>&1; then
    ok "$(Rscript --version 2>&1 | head -1)"
    HAS_R=true
else
    warn "R not found — R chunks will not execute."
    info "Install: https://cran.r-project.org"
fi

# Air — optional, only relevant if R is present
if $HAS_R; then
    if command -v air >/dev/null 2>&1; then
        ok "air found (R formatter)"
    else
        warn "air not found — R code formatting in VS Code will be unavailable."
        info "Install: https://posit-dev.github.io/air"
    fi
fi

# Python — optional
if command -v python3 >/dev/null 2>&1; then
    ok "$(python3 --version)"
    HAS_PYTHON=true
else
    warn "Python not found — Python chunks will not execute."
    info "Install: https://www.python.org/downloads"
fi

# Ruff — optional, only relevant if Python is present
if $HAS_PYTHON; then
    if command -v ruff >/dev/null 2>&1; then
        ok "$(ruff --version) (Python formatter)"
    else
        warn "ruff not found — Python code formatting in VS Code will be unavailable."
        info "Install: pip install ruff  or  https://docs.astral.sh/ruff/installation"
    fi
fi

# Neither R nor Python
if ! $HAS_R && ! $HAS_PYTHON; then
    warn "Neither R nor Python found — no code chunks will execute."
fi

# ── Done ───────────────────────────────────────────────────────────────────────
printf "\n"
if $BLOCKING_OK; then
    printf "${GREEN}${BOLD}Knot %s installed successfully.${RESET}\n\n" "$VERSION"
    printf "Get started:\n"
    printf "  knot init my-project\n"
    printf "  cd my-project && code .\n\n"
else
    printf "${YELLOW}${BOLD}Knot %s installed with warnings.${RESET}\n" "$VERSION"
    printf "Fix the issues above for full functionality.\n\n"
fi
printf "Documentation: https://github.com/%s\n\n" "$REPO"
