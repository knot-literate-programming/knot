# LSP Diagnostics — Runtime Warnings & Errors

**Status:** ✅ Implemented
**Date:** 2026-02-13

## Goal

Surface R/Python runtime warnings and errors as LSP diagnostics (squiggles) in the editor,
integrated into the workflow after execution (build or watch).

## Implementation (2026-02-13)

### 1. Unified Diagnostic Flow
Knot now combines three sources of diagnostics:
- **Structure**: Parsing errors and invalid syntax (captured by `Document::parse`).
- **Options**: Validation of YAML chunk options against known fields.
- **Runtime**: Warnings and Errors persisted in the `.knot_cache/metadata.json` after execution.

### 2. Precise Positioning
While initially thought to be chunk-level only, we achieved **line-level precision**:
- **Python**: Captures `lineno` for warnings and parses tracebacks for errors to find the exact line relative to the chunk start.
- **R**: Highlights the specific line for errors when available in the message. For warnings, since R's `withCallingHandlers` provides the call site but not always a reliable line number, we fall back to highlighting the closing triple backticks (```) to minimize visual noise. (`RuntimeWarning.line` remains reserved for future R introspection improvements).
- **UTF-16 Awareness**: The `PositionMapper` ensures that coordinates are correctly translated between Rust (UTF-8 bytes) and LSP (UTF-16 code units).

### 3. Graceful Degradation & Feedback
- If a fatal error occurs, the language is marked as "broken".
- Subsequent chunks of the same language are rendered as **inert** (grayed out) in the PDF and editor.
- The editor surfaces the full traceback/call info on hover, while the PDF keeps a concise one-line summary for professional rendering.

---

## ⚠️ Future Work

### 1. Diagnostic pour les options de chunk inconnues (PRIORITÉ HAUTE)
Actuellement, une option inconnue dans un chunk est **silencieusement ignorée** par serde.
Exemple : `warinings-visibility: none` (faute de frappe) n'a aucun effet.

- `get_diagnostics()` devrait utiliser `ChunkOptions::option_metadata()` pour valider les clés.
- Toute clé non reconnue → `Diagnostic` avec `severity: Warning` sur la ligne `#|`.

### 2. Live syntax diagnostics (no execution)
Les exécuteurs en arrière-plan pourraient valider la syntaxe en temps réel sur `did_change` sans toucher à l'environnement, offrant un feedback immédiat avant la sauvegarde ou le build.

```r
# R example
tryCatch(parse(text = CODE), error = function(e) e$message)
```

## 🛠️ Constraints & Technical Notes

### 1:1 position mapping invariant
Le mapping des positions `.knot` ↔ `.typ` est basé sur l'identité : le contenu non-Typst est remplacé par des espaces/newlines (même nombre de lignes, même largeur UTF-16). 
**Tout changement dans la génération du `.typ` virtuel doit préserver cet invariant.**

### What Quarto does (for reference)
- **Static diagnostics**: via otter.nvim, which creates hidden language-specific buffers with blank lines for non-code content.
- **Runtime errors**: shown in the terminal only. Not surfaced as LSP diagnostics.
- **Knot's advantage**: Surfacing runtime errors/warnings from cache directly in the editor is a significant improvement over the Quarto experience.
