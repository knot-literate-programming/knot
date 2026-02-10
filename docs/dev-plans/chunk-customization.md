# Chunk Display Customization

**Goal:** Allow users to customize how code chunks are displayed in the rendered document, beyond just controlling execution behavior.

## 🎯 Problem Statement

Currently, Knot supports chunk options that control **execution** (`eval`, `cache`, `echo`) and **metadata** (`caption`, `label`), but the **visual presentation** of chunks is hardcoded in the `#code-chunk` Typst function.

Users cannot:
- Change the layout (horizontal grid vs vertical stack)
- Customize colors, backgrounds, or styling
- Control line numbers, borders, or padding
- Define project-wide presentation defaults

## 📋 Current Architecture Gap

### What exists:
1. **Execution options**: `eval`, `cache`, `echo`, `output`
2. **Graphics options**: `fig-width`, `fig-height`, `dpi`, `fig-format`
3. **Metadata options**: `label`, `caption`, `fig-alt`

### What's missing:
- **Presentation options** that control how `#code-chunk` renders
- **Propagation** of custom options from Rust → Typst
- **Flexible template** in `lib/knot.typ` that respects these options

## 🎨 Proposed Presentation Options

### Display Layout
```yaml
layout: "horizontal" | "vertical" | "output-only" | "code-only"
```
- `horizontal` (default): Side-by-side grid (input | output)
- `vertical`: Stacked layout (input above output)
- `output-only`: Hide code, show only results
- `code-only`: Show only code, suppress output

### Styling Options
```yaml
code-background: color     # Background for code block
output-background: color   # Background for output block
border: bool               # Whether to draw borders
border-color: color        # Color of borders
border-radius: length      # Corner radius
```

### Code Display
```yaml
show-line-numbers: bool    # Display line numbers
line-number-start: int     # Starting line number (default: 1)
highlight-lines: [int]     # Lines to highlight (e.g., [2, 5, 8])
```

### Spacing & Sizing
```yaml
gutter: length             # Space between input and output (horizontal layout)
padding: length            # Internal padding for blocks
width-ratio: (float, float) # Column width ratio for horizontal layout (e.g., (1, 1))
```

## 📐 Order of Precedence

Options are resolved in this order (highest to lowest priority):

1. **Chunk-level options** (in the chunk header)
   ```knot
   ```{r, layout="vertical", border=false}
   ```

2. **knot.toml defaults** (project configuration)
   ```toml
   [defaults]
   layout = "horizontal"
   border = true
   code-background = "#f5f5f5"
   ```

3. **Hardcoded defaults** (in Rust code)
   ```rust
   layout: "horizontal"
   border: true
   code-background: "#ffffff"
   ```

### Resolution Example
```toml
# knot.toml
[defaults]
layout = "horizontal"
border = true
gutter = "1em"
```

```knot
```{r, layout="vertical"}  # Overrides knot.toml's "horizontal"
x <- 1:10
plot(x)
```
```

**Result:** `layout="vertical"`, `border=true` (from knot.toml), `gutter="1em"` (from knot.toml)

## 🏗️ Implementation Plan

**Status:** ✅ **Completed** (commit 200e3f1, February 10, 2026)

### Phase 1: Extend ChunkOptions (Rust) ✅

**File:** `crates/knot-core/src/parser/ast.rs`

Add presentation options to the `define_options!` macro:
```rust
define_options! {
    // ... existing options ...

    /// Layout mode for chunk display
    [val] layout: String, "horizontal".to_string(),
    /// Background color for code block
    #[serde(rename = "code-background")]
    [opt] code_background: String, None,
    /// Background color for output block
    #[serde(rename = "output-background")]
    [opt] output_background: String, None,
    /// Whether to show borders
    [val] border: bool, false,
    /// Border corner radius
    #[serde(rename = "border-radius")]
    [opt] border_radius: String, None,
    /// Whether to show line numbers
    #[serde(rename = "show-line-numbers")]
    [val] show_line_numbers: bool, false,
    /// Gutter size between input and output
    [opt] gutter: String, None,
}
```

### Phase 2: Update knot.toml Support ✅

**File:** `crates/knot-core/src/config.rs`

Extend `ChunkDefaults` to include presentation options:
```rust
#[derive(Debug, Default, Deserialize)]
pub struct ChunkDefaults {
    // ... existing fields ...
    pub layout: Option<String>,
    pub code_background: Option<String>,
    pub output_background: Option<String>,
    pub border: Option<bool>,
    pub border_radius: Option<String>,
    pub show_line_numbers: Option<bool>,
    pub gutter: Option<String>,
}
```

Update `apply_config_defaults()` in `ast.rs`:
```rust
if self.layout.is_none() { self.layout = defaults.layout.clone(); }
if self.code_background.is_none() { self.code_background = defaults.code_background.clone(); }
// ... etc
```

### Phase 3: Propagate Options to Typst ✅

**File:** `crates/knot-core/src/backend.rs`

Modify `format_chunk()` to pass all presentation options to `#code-chunk`:
```rust
let mut chunk_args = vec![
    format!("input: {}", input_content),
    format!("output: {}", output_content),
];

// Add presentation options
chunk_args.push(format!("layout: \"{}\"", resolved.layout));
if let Some(bg) = &resolved.code_background {
    chunk_args.push(format!("code-background: rgb(\"{}\")", bg));
}
if let Some(bg) = &resolved.output_background {
    chunk_args.push(format!("output-background: rgb(\"{}\")", bg));
}
chunk_args.push(format!("border: {}", resolved.border));
if let Some(gutter) = &resolved.gutter {
    chunk_args.push(format!("gutter: {}", gutter));
}

let chunk_call = format!("#code-chunk({})", chunk_args.join(", "));
```

### Phase 4: Flexible Typst Template ✅

**File:** `lib/knot.typ` (in project templates)

Rewrite `#code-chunk` to handle all presentation options:
```typst
#let code-chunk(
  input: none,
  output: none,
  layout: "horizontal",
  code-background: rgb("#ffffff"),
  output-background: rgb("#f4f4f4"),
  border: false,
  border-radius: 4pt,
  show-line-numbers: false,
  gutter: 1em,
  ..rest
) = {
  let code-block = if input != none {
    block(
      fill: code-background,
      radius: border-radius,
      inset: 8pt,
      width: 100%,
      stroke: if border { 1pt + gray } else { none }
    )[#input]
  } else { [] }

  let output-block = if output != none {
    block(
      fill: output-background,
      radius: border-radius,
      inset: 8pt,
      width: 100%,
      stroke: if border { 1pt + gray } else { none }
    )[#output]
  } else { [] }

  if layout == "vertical" {
    stack(dir: ttb, spacing: gutter, code-block, output-block)
  } else if layout == "output-only" {
    output-block
  } else if layout == "code-only" {
    code-block
  } else {
    // horizontal (default)
    grid(
      columns: (1fr, 1fr),
      gutter: gutter,
      code-block,
      output-block
    )
  }
}
```

## 📚 Example Use Cases

### 1. Vertical Layout for Long Code
```knot
```{r, layout="vertical"}
# Long data processing pipeline
data <- read.csv("large_dataset.csv")
processed <- data |>
  filter(value > 100) |>
  mutate(log_value = log(value)) |>
  summarize(mean = mean(log_value))
print(processed)
```
```

### 2. Output-Only for Reports
```knot
```{python, layout="output-only", caption="Summary Statistics"}
import pandas as pd
df = pd.DataFrame({"x": [1, 2, 3], "y": [4, 5, 6]})
print(df.describe())
```
```

### 3. Custom Styling
```knot
```{r, code-background="#fef3cd", output-background="#d1ecf1", border=true}
x <- runif(100)
hist(x, main="Random Distribution")
```
```

### 4. Project-Wide Defaults
```toml
# knot.toml
[defaults]
layout = "vertical"
border = true
border-radius = "6pt"
code-background = "#f8f9fa"
output-background = "#e9ecef"
gutter = "1.5em"
show-line-numbers = true
```

## ✅ Success Criteria

- [x] Users can set presentation options in chunk headers
- [x] Users can define project-wide defaults in `knot.toml`
- [x] Options follow the correct precedence order
- [x] `#code-chunk` template is flexible and customizable
- [ ] Documentation includes examples of all presentation options (TODO: update user docs)
- [x] No breaking changes to existing `.knot` files (defaults preserve current behavior)

### Bonus Features Implemented

Beyond the original plan, the implementation includes:

- **Auto-generated OptionMetadata**: The `define_options!` macro now automatically generates an `option_metadata()` method that returns all option information (name, type, default, documentation).

- **Dynamic knot.toml generation**: `knot init` now generates the `[defaults]` section dynamically using `ChunkOptions::option_metadata()`, ensuring the template is always synchronized with available options.

- **Clear separation of concerns**:
  - `lib/knot.typ` → utility functions only
  - `main.knot` → document configuration (Codly setup, figure numbering)
  - This makes the architecture cleaner and more maintainable.

- **Example tool**: Added `crates/knot-core/examples/generate_defaults.rs` for development/debugging of option documentation generation.

## 🔗 Related Work

- **formatters.md**: Code formatting (Air/Ruff) is orthogonal to presentation
- **master-plan.md**: Add this as a roadmap item under "Knot Core"

## 📝 Notes

- Color values should support both hex (`#rrggbb`) and Typst's `rgb()` format
- Length values (gutter, padding, radius) should use Typst units (`em`, `pt`, `mm`)
- Consider adding presets in the future (e.g., `style: "minimal"` vs `style: "verbose"`)
- Line numbering and syntax highlighting are handled by `codly`, not `#code-chunk`
