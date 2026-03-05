# Adding a Chunk Option

Chunk options are the `#| key: value` lines at the top of a code chunk. They
control evaluation, display, figure dimensions, and more. This page shows how
to add a new one end-to-end.

We will add a hypothetical `center-output: true` option as a worked example.

---

## Step 1 — Declare the option in `ChunkOptions`

`crates/knot-core/src/parser/options.rs` contains the `ChunkOptions` struct
(the raw parsed options) and `ResolvedChunkOptions` (after merging defaults).

Add your field to both:

```rust
// In ChunkOptions (raw, all Option<T>)
pub center_output: Option<bool>,

// In ResolvedChunkOptions (resolved, concrete type with a default)
pub center_output: bool,
```

Then update `ResolvedChunkOptions::resolve()` to merge from the option chain
(global defaults → language defaults → per-chunk):

```rust
center_output: per_chunk.center_output
    .or(lang_default.center_output)
    .or(global_default.center_output)
    .unwrap_or(false),
```

---

## Step 2 — Parse the YAML key

The parser in `parser/winnow_parser.rs` reads `#|` lines as YAML strings.
Option names use kebab-case in the source and are mapped to the `ChunkOptions`
fields in the `apply_option` function:

```rust
"center-output" => {
    options.center_output = Some(
        value.parse::<bool>().map_err(|_| ParseError::InvalidOptionValue {
            key: "center-output",
            value: value.to_string(),
        })?
    );
}
```

---

## Step 3 — Register metadata (drives completion + hover)

`OptionMetadata` in `crates/knot-core/src/defaults.rs` drives both the LSP
completion list and hover documentation. Add an entry:

```rust
OptionMetadata {
    name: "center-output",
    kind: OptionKind::Bool,
    default: "false",
    description: "Center the output block horizontally in the document.",
},
```

---

## Step 4 — Use the option in the backend

`backend.rs` renders each chunk node to Typst. In `format_chunk()`, pass the
new option as a parameter to the `#code-chunk(...)` call:

```rust
if options.center_output {
    lines.push("  center-output: true,".to_string());
}
```

---

## Step 5 — Handle it in `lib/knot.typ`

If the option affects rendering, add the corresponding logic to the
`code-chunk` function in `lib/knot.typ`. For `center-output`:

```typst
#let code-chunk(
  // … existing params …
  center-output: false,
  body,
) = {
  // …
  if center-output {
    align(center, output-block)
  } else {
    output-block
  }
}
```

---

## Step 6 — Add a snapshot test

Add a test case in `crates/knot-core/src/backend.rs`:

```rust
#[test]
fn test_format_chunk_center_output() {
    let backend = Backend::new(BackendOptions::default());
    let result = backend.format_chunk(&Node::CodeChunk {
        // … with center_output: true …
    });
    assert_snapshot!(result);
}
```

Run `INSTA_UPDATE=always cargo test -p knot-core` to generate the snapshot.

---

## Step 7 — Expose in `knot.toml`

If the option should be configurable globally or per-language, add it to
`config.rs` in the `ChunkDefaults` struct and handle it in `load_config()`.
Document it in `docs/book/src/chunk-options.md` and
`docs/book/src/configuration.md`.
