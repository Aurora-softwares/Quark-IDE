# Quark IDE — v0.3.0 UI and editing update

This patch is based on the attached **v0.3.0 Rust source ZIP**, rather than a newer local branch. It does **not** modify your GitHub repository or GitHub Projects.

## Changed

1. **Restrained theme** — graphite surfaces, teal highlights and selected widgets, warm warning status. Hydrogen syntax categories use separate subdued colours, not an all-over bright UI.
2. **Logical Hydrogen outline** — the physical workspace folder tree remains. In *Hydrogen Project Outline*, `.hyproj` projects group their source into namespace segments, nested types and class/struct/interface/enum names. Select a leaf to open its source file. Class-free files appear directly under their declared namespace. Click **Refresh** after changing files externally; saving in Quark automatically invalidates the cache.
3. **Editor scrolling** — each tab now has its own stable edit/scroll ID. Text starts at the left of the central editor viewport; long unwrapped lines use a real horizontal scroll area, without the old fixed 960px minimum or an infinite requested width. Alt+Z still enables wrapping.
4. **Hydrogen lexical colours** — expanded keywords and modifiers based on Hydrogen syntax examples; string escapes, single-line and block comments, hex/binary/decimal numeric tokens, primitive types, punctuation, operators, and contextual declaration/method names. This is **lexer-level** support, not a compiler-validated semantic highlighter.
5. **Inline completion** — two typed identifier characters trigger an inline list at the caret (e.g. `st` -> `static`, `string`, `struct`). Arrow Up/Down changes selection, Enter or Tab inserts, Escape dismisses, and mouse clicks insert directly at the caret instead of appending at EOF. Ctrl/Cmd+Space requests suggestions manually. This is local keyword/identifier completion; semantic symbols across files still require a Hydrogen language server.
6. **Tab correction** — closing a background tab no longer incorrectly selects a different document.

## Running it

From the extracted source root:

```sh
cargo fmt --all
cargo test
cargo clippy --all-targets -- -D warnings
cargo run
```

The provided environment did not include `cargo` or `rustc` and had no access to the Rust distribution servers. Consequently compilation and UI behaviour have **not** been validated on a native Rust toolchain here. Please run these commands and the updated `scripts/manual-acceptance.md` checklist on your computer before considering this release-ready.

## Limitations and follow-up

- The outline uses lightweight lexical scanning rather than a complete Hydrogen grammar; multi-namespace files and rare declaration forms may need additional handling.
- Autocomplete suggests keywords and identifiers from the current file, not imported projects. Compiler-backed semantic completion and go-to-definition need a language server.
- The wide-line scrolling fix needs visual validation with your problematic long line on Linux, macOS and Windows.
- Safe file writes, session recovery and large-file indexing are intentionally left for the dedicated reliability milestone.
