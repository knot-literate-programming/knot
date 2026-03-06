# LSP Navigation: Definition & Hover

**Goal:** Implement standard IDE navigation features for Typst content inside Knot files.

## 💡 The "Virtual URI" Strategy
We already successfully used the `.knot.typ` virtual URI trick for diagnostics. We apply the same pattern for navigation requests.

### 1. Go to Definition (`textDocument/definition`) — ⏳ TODO (issue #5)
1. **Request**: Intercept the request from VS Code.
2. **Map In**: Convert Knot position (`main.knot`) -> Typst position (`main.knot.typ`).
3. **Forward**: Send the request to Tinymist using the virtual URI.
4. **Response**: Capture the `Location` or `LocationLink`.
5. **Map Out**:
   - Convert the target URI from `.knot.typ` back to `.knot`.
   - Convert the target range back to Knot coordinates.
6. **Reply**: Return the mapped location to VS Code.

### 2. Hover (`textDocument/hover`) — ✅ Done
Implemented in `knot-lsp/src/handlers/hover.rs`:
- R/Python chunks: forwards to the language executor (`get_r_help`, `get_python_help`)
- Inline expressions: same, via `inline_exprs`
- Typst content (section 3 of `handle_hover`): forwarded to Tinymist via virtual URI, range mapped back to `.knot` coordinates

## 📋 Remaining work
- [ ] Implement `handle_definition` in `knot-lsp/src/handlers/` (see issue #5)

## 🎯 Success Criteria
- [x] Hovering over a Typst variable shows its type/documentation.
- [ ] Ctrl+Click on a Typst function name opens the definition.
