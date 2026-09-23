#!/usr/bin/env bash
#
# Install the VS Code extension from source (for contributors).
# Run from anywhere inside the knot repository.
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VSCODE_DIR="$SCRIPT_DIR/../editors/vscode"

cd "$VSCODE_DIR"
VSIX_FILE="$(node -p 'const p = require("./package.json"); `${p.name}-${p.version}.vsix`')"
npm install
npm run package -- --out "$VSIX_FILE"
code --install-extension "$VSIX_FILE" --force

echo "Done. Restart VS Code to activate the extension."
