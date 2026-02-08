# LSP Navigation: Definition & Hover

**Goal:** Implement standard IDE navigation features for Typst content inside Knot files.

## 💡 The "Virtual URI" Strategy
We already successfully used the `.knot.typ` virtual URI trick for diagnostics. We will apply the same pattern for navigation requests.

### 1. Go to Definition (`textDocument/definition`)
1. **Request**: Intercept the request from VS Code.
2. **Map In**: Convert Knot position (`main.knot`) -> Typst position (`main.knot.typ`).
3. **Forward**: Send the request to Tinymist using the virtual URI.
4. **Response**: Capture the `Location` or `LocationLink`.
5. **Map Out**: 
   - Convert the target URI from `.knot.typ` back to `.knot`.
   - Convert the target range back to Knot coordinates.
6. **Reply**: Return the mapped location to VS Code.

### 2. Hover (`textDocument/hover`)
Similar to definition, but handle the Markdown content in the response. Since we use a 1:1 line padding, the Markdown content shouldn't need heavy modification unless it contains line-specific links.

## 📋 Implementation Plan
- [ ] Implement `handle_definition` in `knot-lsp/src/handlers/`.
- [ ] Implement `handle_hover` logic for Typst (currently only handles R/Python).
- [ ] Update `PositionMapper` if 1:1 identity is no longer sufficient.

## 🎯 Success Criteria
- [ ] Ctrl+Click on a Typst function name opens the definition.
- [ ] Hovering over a Typst variable shows its type/documentation.
