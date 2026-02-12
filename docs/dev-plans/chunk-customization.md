# Chunk Display Customization

**Goal:** Allow users to customize how code chunks are displayed in the rendered document, beyond just controlling execution behavior.

## 🎯 Problem Statement

Currently, Knot supports chunk options that control **execution** (`eval`, `cache`) and **visibility** (`show`), but the **visual presentation** of chunks is hardcoded in the `#code-chunk` Typst function.

Users cannot:
- Change the layout (horizontal grid vs vertical stack)
- Customize colors, backgrounds, or styling
- Control line numbers, borders, or padding
- Define project-wide presentation defaults

## 📋 Current Architecture Gap

### What exists:
1. **Execution options**: `eval`, `cache`
2. **Visibility options**: `show` (`code`, `output`, `both`, `none`)
3. **Graphics options**: `fig-width`, `fig-height`, `dpi`, `fig-format`
4. **Metadata options**: `label`, `caption`, `depends`

### What's missing:
- **Presentation options** that control how `#code-chunk` renders
- **Propagation** of custom options from Rust → Typst
- **Flexible template** in `lib/knot.typ` that respects these options

## 🎨 Proposed Presentation Options

### Display Mode
```yaml
show: "both" | "code" | "output" | "none"
```
- `both` (default): Display both code and output
- `code`: Display only the source code
- `output`: Display only the execution results
- `none`: Execute the chunk but display nothing (useful for setup/imports)

### Display Layout (when show: "both")
```yaml
layout: "horizontal" | "vertical"
```
- `horizontal` (default): Side-by-side grid (code | output)
- `vertical`: Stacked layout (code above output)

### Styling Options
```yaml
code-background: color     # Background for code block
code-stroke: stroke        # Border for code block (alias: code-border)
code-inset: length         # Padding for code block (alias: code-padding)
code-radius: length        # Corner radius for code block

output-background: color   # Background for output block
output-stroke: stroke      # Border for output block (alias: output-border)
output-inset: length       # Padding for output block (alias: output-padding)
output-radius: length      # Corner radius for output block
```

### Spacing & Sizing
```yaml
gutter: length             # Space between code and output (horizontal layout)
width-ratio: string        # Column width ratio (e.g., "1:1", "2:1")
align: string              # Content alignment (Typst alignment)
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

The `define_options!` macro system uses markers to control behavior and configurability:
- `[val]` : Required value with default, configurable in `knot.toml`.
- `[opt]` : Optional value without default, configurable in `knot.toml`.
- `[meta]`: Chunk-specific metadata, NOT configurable in `knot.toml`.
- `[col]` : Collection (Vec), NOT configurable in `knot.toml`.

```rust
define_options! {
    /// Whether to evaluate the chunk
    [val] eval: bool, true,
    /// What to display in the output (both, code, output, or none)
    [val] show: Show, Show::Both,
    /// Whether to cache the results
    [val] cache: bool, true,

    // ... Presentation Options ...

    /// How to layout code and output (horizontal or vertical)
    [val] layout: Layout, Layout::Horizontal,
    /// Space between code and output blocks
    [opt] gutter: String, None,
    /// Background color for code container
    #[serde(rename = "code-background")]
    [opt] code_background: String, None,
    /// Border stroke for code container (alias: code-border)
    #[serde(rename = "code-stroke", alias = "code-border")]
    [opt] code_stroke: String, None,
    /// Corner radius for code container
    #[serde(rename = "code-radius")]
    [opt] code_radius: String, None,
    /// Internal padding for code container (alias: code-padding)
    #[serde(rename = "code-inset", alias = "code-padding")]
    [opt] code_inset: String, None,
    
    // ... Output options follow same pattern ...
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

The `TypstBackend` translates `ResolvedChunkOptions` into a Typst `#code-chunk()` call. It intelligently handles visibility based on the `show` option:

```rust
// Generate code based on show option
let should_show_code = matches!(resolved_options.show, Show::Both | Show::Code);
if should_show_code {
    args.push(format!("code: {}", code_str));
} else {
    args.push("code: none".to_string());
}

// Generate output based on show option
let should_show_output = matches!(resolved_options.show, Show::Both | Show::Output);
if should_show_output {
    args.push(format!("output: {}", output_str));
} else {
    args.push("output: none".to_string());
}
```

### Phase 4: Flexible Typst Template ✅

**File:** `lib/knot.typ` (in project templates)

The `#code-chunk` function in Typst uses standard blocks and can be fully customized:

```typst
#let code-chunk(
  code: none,
  output: none,
  layout: none,
  gutter: 0.5em,
  code-background: none,
  code-stroke: none,
  code-inset: 0pt,
  ..
) = {
  // ... ratio parsing ...

  let code-block = if code != none {
    block(fill: code-background, stroke: code-stroke, inset: code-inset)[#code]
  } else { none }

  // ... output block ...

  if code == none and output != none {
    output-block
  } else if output == none and code != none {
    code-block
  } else if code != none and output != none {
    // Both: use horizontal/vertical layout
  }
}
```

## 📚 Example Use Cases

### 1. Vertical Layout for Long Code
```knot
```{r}
#| layout: vertical
# Long data processing pipeline
data <- read.csv("large_dataset.csv")
print(data)
```
```

### 2. Output-Only for Reports
```knot
```{python}
#| show: output
#| caption: Summary Statistics
import pandas as pd
df = pd.DataFrame({"x": [1, 2, 3]})
print(df.describe())
```
```

### 3. Custom Styling (using aliases)
```knot
```{r}
#| code-background: "#fef3cd"
#| output-background: "#d1ecf1"
#| code-border: 1pt + orange
#| code-padding: 10pt
hist(runif(100))
```
```

### 4. Project-Wide Defaults
```toml
# knot.toml
[chunk-defaults]
layout = "vertical"
code-radius = "6pt"
code-background = "#f8f9fa"
output-background = "#e9ecef"
gutter = "1.5em"
```

## ✅ Success Criteria

- [x] Users can set presentation options in chunk headers
- [x] Users can define project-wide defaults in `knot.toml`
- [x] Options follow the correct precedence order
- [x] `#code-chunk` template is flexible and customizable
- [x] `none` mode allows execution without display
- [x] Aliases (`padding`, `border`) improve UX for CSS/Quarto users
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
