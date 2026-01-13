# Quick Start: Testing Syntax Highlighting (No Air Required)

You can test syntax highlighting **immediately** without installing Air!

## 1. Open VS Code in this directory

```bash
cd editors/vscode
code .
```

## 2. Launch Extension Development Host

Press `F5` (or Run → Start Debugging)

This will open a new VS Code window with the extension loaded.

## 3. Open a test file

In the new window, open: `File → Open File → ../../examples/test-syntax.knot`

## 4. Verify Syntax Highlighting

You should immediately see:

- ✅ **Headings** (`= Title`) in bold/large font
- ✅ **R code chunks** with proper syntax highlighting:
  - Keywords: `library`, `function`, `if`, `for`
  - Operators: `<-`, `%>%`, `+`, `-`
  - Strings: `"text"` in green/orange
  - Numbers: `123` in distinct color
  - Comments: `# comment` in gray
- ✅ **Inline R** `#r[nrow(df)]` highlighted
- ✅ **Chunk options** `#| eval: true` highlighted
- ✅ **Typst markup**:
  - Bold: `*text*`
  - Italic: `_text_`
  - Code: `` `code` ``
  - Math: `$E = mc^2$`
  - Functions: `#table`, `#align`, `#figure`

## 5. Test Basic Features

**Navigation:**
- Press `Ctrl+Shift+O` (or `Cmd+Shift+O` on macOS)
- You should see a list of all R chunks
- Click to jump to a chunk

**Folding:**
- Click the small arrow next to ` ```{r ` to fold/unfold chunks

**Bracket Matching:**
- Place cursor on `{` → matching `}` is highlighted
- Same for `[`, `]`, `(`, `)`

## What's Working vs. Not Working (Without Air)

### ✅ Working (No dependencies)

- Syntax highlighting for R and Typst
- Code folding
- Bracket matching
- Document symbols (chunk list)
- Auto-closing brackets/quotes
- Basic diagnostics (parsing errors)

### ❌ Not Working (Requires Air)

- R code formatting (`Shift+Alt+F`)
- Format-on-type (auto-format on Enter)

## Next: Install Air for Formatting

If you want to test formatting features:

**macOS:**
```bash
brew install posit-dev/tap/air
```

**Linux:**
```bash
curl -fsSL https://posit-dev.github.io/air/install.sh | sh
```

Then restart the Extension Development Host (press `F5` again).

## Troubleshooting

**Problem:** File shows as "Plain Text" instead of "Knot"

**Solution:** Click the language indicator (bottom right) and select "Knot"

**Problem:** No syntax highlighting at all

**Solution:**
1. Ensure the file has `.knot` extension
2. Reload the window: `Ctrl+Shift+P` → "Reload Window"
3. Check for errors: `Ctrl+Shift+P` → "Toggle Developer Tools"

**Problem:** Some colors look wrong

**Solution:** This is normal - different VS Code themes render colors differently. The semantic highlighting is working, just with different colors.

## Comparing to Plain Text

To appreciate the highlighting:

1. Open `test-syntax.knot`
2. Click language indicator (bottom right)
3. Select "Plain Text"
4. See the difference (no colors!)
5. Switch back to "Knot" to see highlighting

## Success Criteria

If you can see:
- R code chunks with colorful syntax (not all gray)
- Chunk options `#|` in a distinct color
- Typst headings `=` styled differently
- Math `$...$` highlighted

Then **syntax highlighting is working!** 🎉

You can now start using Knot with basic IDE support, and add formatting later by installing Air.
