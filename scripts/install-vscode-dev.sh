#!/usr/bin/env bash
#
# Install the VS Code extension from source (for contributors).
# Run from anywhere inside the knot repository.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VSCODE_DIR="$SCRIPT_DIR/../editors/vscode"

cd "$VSCODE_DIR"
npm install
npm run package
code --install-extension knot-*.vsix --force

echo "Done. Restart VS Code to activate the extension."
