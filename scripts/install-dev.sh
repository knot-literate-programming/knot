#!/usr/bin/env bash
#
# Install the CLI, LSP and VS Code extension from the same checkout.
# The script resolves paths relative to itself, regardless of the current directory.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
VSCODE_DIR="$REPO_DIR/editors/vscode"

for tool in cargo node npm code; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "Required command not found: $tool" >&2
        exit 1
    fi
done

echo "Installing Knot CLI and LSP from $REPO_DIR..."
# Force replacement even when the development package version has not changed.
cargo install --locked --force --path "$REPO_DIR/crates/knot-cli"
cargo install --locked --force --path "$REPO_DIR/crates/knot-lsp"

cd "$VSCODE_DIR"
VSIX_FILE="$(node -p 'const p = require("./package.json"); `${p.name}-${p.version}.vsix`')"
npm ci
npm run package -- --out "$VSIX_FILE"
code --install-extension "$VSIX_FILE" --force

echo "Done. Run 'Developer: Reload Window' in VS Code to load the updated extension and LSP."
echo "If knot.lsp.path is customized, ensure it points to the knot-lsp binary just installed by Cargo."
