# Knot Editor Support

This directory contains IDE/editor integrations for Knot.

## VS Code Extension

Location: `vscode/`

### Features Implemented

#### ✅ Phase 1: Syntax Highlighting & Formatting (Complete)

**Syntax Highlighting:**
- Typst markup (headings, bold, italic, code, math)
- R code chunks with full R syntax highlighting
- Inline R expressions `#r[...]`
- Chunk options `#| key: value`
- YAML frontmatter
- Comments and links

**Code Formatting:**
- Format entire document (`Shift+Alt+F`)
- Format on type (auto-format R chunks on newline)
- Uses Air formatter (Posit's official R formatter)
- Graceful degradation if Air not installed

**Basic LSP Features:**
- Document symbols (chunk navigation)
- Diagnostics (parsing errors)
- Syntax validation

### Quick Start

See: `vscode/QUICKSTART.md` for immediate testing (no dependencies)

Full guide: `vscode/TESTING.md`

### Installation

**For Development:**
```bash
cd vscode
code .
# Press F5 to launch Extension Development Host
```

**For Users (future):**
```bash
# Will be published to VS Code Marketplace
code --install-extension knot
```

## Architecture Overview

```
editors/
└── vscode/
    ├── package.json              # Extension manifest
    ├── language-configuration.json  # Brackets, folding, etc.
    ├── syntaxes/
    │   └── knot.tmLanguage.json  # TextMate grammar (syntax highlighting)
    ├── README.md                 # User-facing documentation
    ├── QUICKSTART.md             # Quick testing guide
    └── TESTING.md                # Comprehensive testing guide
```

## Language Server (knot-lsp)

Location: `../crates/knot-lsp/`

### Implemented Features

**Phase 1: Basic Support ✅**
- Document parsing and validation
- Diagnostics for malformed chunks
- Document symbols (chunk outline)
- R code formatting via Air

**Current Capabilities:**
- `textDocument/didOpen` - Track opened files
- `textDocument/didChange` - Track edits
- `textDocument/diagnostic` - Syntax errors
- `textDocument/documentSymbol` - Chunk list
- `textDocument/formatting` - Format all R chunks
- `textDocument/onTypeFormatting` - Format current chunk on newline

### Planned Features

**Phase 2: Enhanced LSP (with tinymist proxy) 🚧**
- Proxy Typst requests to tinymist
- Combined diagnostics (Typst + R + Knot)
- Hover information on chunks
- Completion for chunk options
- Live preview integration

**Phase 3: R Language Support (with Air/Ark) 🔮**
- R diagnostics (via Air LSP when available)
- R hover and completion
- R semantic highlighting
- Jump to definition (R variables)

## How It Works

### Syntax Highlighting

**TextMate Grammar** (no runtime dependencies):
1. VS Code loads `knot.tmLanguage.json`
2. Grammar defines patterns for Knot syntax
3. Embedded grammars delegate to R and Typst
4. Works offline, instant, no LSP needed

Example pattern (R chunk):
```json
{
  "begin": "^(```)(\\{)(r)...",
  "patterns": [
    { "include": "source.r" }  // Delegate to R grammar
  ]
}
```

### Code Formatting

**Air Integration** (optional dependency):
1. User presses Enter in R chunk
2. VS Code sends `textDocument/onTypeFormatting` to knot-lsp
3. knot-lsp parses document, finds current chunk
4. Spawns `air format --stdin` subprocess
5. Streams R code to Air, receives formatted code
6. Returns TextEdit to VS Code
7. VS Code applies the edit

**Graceful Degradation:**
- If Air not installed → formatting disabled, everything else works
- If Air formatting fails → error logged, no crash
- Works with any Air version (calls via CLI, not library)

### Document Symbols

**Chunk Navigation:**
1. User opens Outline panel (`Ctrl+Shift+O`)
2. VS Code requests `textDocument/documentSymbol`
3. knot-lsp parses document with knot-core
4. Returns list of chunks with positions
5. VS Code shows clickable list
6. Clicking jumps to chunk

## Dependencies

### Required (for syntax highlighting)
- None! Just VS Code.

### Optional (for formatting)
- **Air**: R code formatter
  - Install: `brew install posit-dev/tap/air` (macOS)
  - Or: `curl -fsSL https://posit-dev.github.io/air/install.sh | sh`
  - Website: https://posit-dev.github.io/air/

### Optional (for LSP features)
- **knot-lsp**: Language server (built from source)
  - Build: `cargo build --release -p knot-lsp`
  - Auto-detected if in PATH

## Testing

### Test Syntax Highlighting (No dependencies)

```bash
cd editors/vscode
code .
# Press F5
# Open ../../examples/test-syntax.knot
```

You should see colorful R and Typst syntax.

### Test Formatting (Requires Air)

1. Install Air (see above)
2. Launch extension (`F5`)
3. Open a `.knot` file
4. Type messy R code: `x<-1+2`
5. Press Enter
6. Should auto-format to: `x <- 1 + 2`

### Test LSP (Requires knot-lsp)

1. Build: `cargo build --release -p knot-lsp`
2. Add to PATH or configure in VS Code settings
3. Open `.knot` file
4. Check Output panel: "Knot Language Server" should show startup logs
5. Press `Ctrl+Shift+O` to see chunk list

## Future Editors

### Neovim
- Use knot-lsp with `nvim-lspconfig`
- TreeSitter grammar (port from TextMate)
- Telescope integration for chunk navigation

### Emacs
- Use knot-lsp with `lsp-mode` or `eglot`
- Major mode for Knot
- Org-mode-like chunk execution

### Zed
- Language extension with TreeSitter grammar
- Native LSP integration

## Contributing

To add support for a new editor:

1. **Syntax Highlighting:**
   - Port `vscode/syntaxes/knot.tmLanguage.json` to editor's format
   - TreeSitter grammar (for modern editors)
   - TextMate grammar (for classic editors)

2. **LSP Integration:**
   - Configure editor to launch `knot-lsp`
   - Map LSP capabilities to editor features
   - Test with `examples/test-syntax.knot`

3. **Formatting:**
   - Use LSP formatting capabilities (built-in)
   - Or call Air directly from editor plugin

## Resources

- **Air Formatter**: https://posit-dev.github.io/air/
- **LSP Spec**: https://microsoft.github.io/language-server-protocol/
- **TextMate Grammars**: https://macromates.com/manual/en/language_grammars
- **Typst**: https://typst.app/
- **R**: https://www.r-project.org/

## Status

| Feature | Status | Notes |
|---------|--------|-------|
| Syntax highlighting | ✅ Complete | TextMate grammar |
| Code folding | ✅ Complete | Built-in |
| Bracket matching | ✅ Complete | Built-in |
| Document symbols | ✅ Complete | Chunk navigation |
| Diagnostics | ✅ Basic | Parsing errors only |
| R formatting | ✅ Complete | Via Air |
| Format-on-type | ✅ Complete | R chunks only |
| Hover | 🚧 Planned | Phase 2 |
| Completion | 🚧 Planned | Phase 2 |
| Typst LSP proxy | 🚧 Planned | Phase 2 (tinymist) |
| R LSP proxy | 🔮 Future | Phase 3 (Air/Ark) |
| Live preview | 🔮 Future | Phase 2/3 |

✅ Complete | 🚧 In progress | 🔮 Planned
