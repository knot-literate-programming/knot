# Testing the Knot VS Code Extension

## Prerequisites

### 1. Install Air (R formatter)

Air is required for R code formatting features.

**macOS/Linux:**
```bash
curl -fsSL https://posit-dev.github.io/air/install.sh | sh
```

**macOS with Homebrew:**
```bash
brew install posit-dev/tap/air
```

**Windows:**
```powershell
winget install Posit.Air
```

Verify installation:
```bash
air --version
```

### 2. Build knot-lsp

From the root of the knot repository:
```bash
cargo build --release -p knot-lsp
```

The binary will be at: `target/release/knot-lsp`

## Installation Methods

### Method 1: Development Mode (Recommended for testing)

1. Open this directory in VS Code:
   ```bash
   cd editors/vscode
   code .
   ```

2. Press `F5` to launch Extension Development Host

3. In the new VS Code window, open a `.knot` file (e.g., `../../examples/test-syntax.knot`)

### Method 2: Install from VSIX

1. Package the extension:
   ```bash
   cd editors/vscode
   npm install
   npm run package
   ```

2. Install the generated `.vsix` file:
   - In VS Code: Extensions → `...` menu → "Install from VSIX"
   - Or: `code --install-extension knot-0.1.0.vsix`

### Method 3: Symlink (for continuous development)

```bash
ln -s $(pwd) ~/.vscode/extensions/knot-vscode
```

Then reload VS Code.

## Testing Features

### 1. Syntax Highlighting

Open `examples/test-syntax.knot` and verify:

- ✅ Typst markup is highlighted (headings with `=`, bold `*`, italic `_`)
- ✅ R code chunks have proper R syntax highlighting
- ✅ Inline R expressions `#r[...]` are highlighted
- ✅ Chunk options `#| eval: false` are highlighted
- ✅ Math `$...$` is highlighted
- ✅ Typst functions `#table`, `#figure` are highlighted
- ✅ Comments `//` are highlighted

### 2. Code Formatting

**Format entire document:**
1. Open a `.knot` file with messy R code
2. Press `Shift+Alt+F` (or `Cmd+Shift+P` → "Format Document")
3. Verify that all R chunks are formatted

**Format on type:**
1. Go inside an R chunk
2. Type some messy R code: `x<-1+2`
3. Press Enter (newline)
4. The chunk should auto-format: `x <- 1 + 2`

**Expected formatting changes:**
- `x<-1` → `x <- 1` (spaces around operators)
- `if(x){y}` → proper indentation and spacing
- Pipe operators `%>%` properly aligned

### 3. Language Server Features

If knot-lsp is in PATH or configured:

1. Open a `.knot` file
2. Check the Output panel (View → Output → select "Knot Language Server")
3. Verify the server started without errors

**Document Symbols:**
- Press `Ctrl+Shift+O` (or `Cmd+Shift+O` on macOS)
- You should see a list of all R chunks
- Click on a chunk name to navigate to it

**Diagnostics:**
- Create a malformed chunk: ` ```{r ` without closing
- You should see a red squiggly line with error message

## Troubleshooting

### "Air formatter not available"

Check that Air is installed and in PATH:
```bash
which air  # Should output a path
air --version
```

If Air is installed in a custom location, configure in VS Code settings:
```json
{
  "knot.formatter.air.path": "/custom/path/to/air"
}
```

### "knot-lsp not found"

Add the binary to your PATH, or configure:
```json
{
  "knot.lsp.path": "/absolute/path/to/target/release/knot-lsp"
}
```

### Syntax highlighting not working

1. Verify the file has `.knot` extension
2. Check the language mode (bottom right of VS Code): should show "Knot"
3. If it shows "Plain Text", click it and select "Knot"

### Formatting not working

1. Check Output panel for formatter errors
2. Verify Air can format the R code manually:
   ```bash
   echo 'x<-1+2' | air format --stdin
   ```
3. Check if the chunk has syntax errors (Air won't format invalid R)

## Configuration

### VS Code Settings

All available settings:

```json
{
  // Enable/disable LSP
  "knot.lsp.enabled": true,

  // Path to knot-lsp executable
  "knot.lsp.path": "knot-lsp",

  // Path to Air executable
  "knot.formatter.air.path": "air",

  // Format R chunks on newline
  "knot.formatter.formatOnType": true,

  // Format R chunks on save
  "knot.formatter.formatOnSave": true
}
```

### Disable specific features

To disable format-on-type:
```json
{
  "knot.formatter.formatOnType": false
}
```

To use only syntax highlighting (no LSP):
```json
{
  "knot.lsp.enabled": false
}
```

## Next Steps

After verifying everything works:

1. Test with your own `.knot` documents
2. Report issues at: https://github.com/YOUR_REPO/knot/issues
3. Contribute improvements to the grammar or LSP features

## Known Limitations

- Full Typst syntax highlighting requires the Typst extension
- LSP features (hover, completion) are still in development
- Air must be installed separately (not bundled)
- Format-on-type only formats the current R chunk (not the whole document)
