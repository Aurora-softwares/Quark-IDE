# Quark IDE — diagnostic gutter (working source update)

This source update builds on `Quark-IDE-compact-workspace-QOL-source.zip`.
It does **not** modify or push anything to GitHub.

## Editor changes

- Fixed-width gutter at the left edge of the editor, separate from the explorer sidebar.
- Line numbers use the **same egui galley** as the editor, accounting for text wrapping and vertical scrolling.
- Marker icons and tooltips: `!` for lexical errors/warnings, `●` for a possible unused declaration.
- Underlines appear under flagged source tokens. The colour matches the diagnostic severity.
- Status-bar count displays problems and hints.
- Analysis is performed on a worker thread, debounced after edits to avoid blocking typing.
- Unsaved source buffers override saved files when scanning a project's source graph.

## Project-wide unused-code hints: deliberately conservative

Quark loads the selected `.hyproj`, recursively resolves `project_references` and counts identifier references across all resolvable `.hy` source files. It suggests **possibly unused** when a simple private method, private field or typed local-variable declaration has no other identifier occurrences. Comments and string literals do not count as references. The code deliberately suppresses unused suggestions when the project graph has missing sources or unreadable manifests.

This is **not** a semantic language server. It cannot resolve overloads, determine symbol identity across namespaces, detect dynamically generated/reflection calls or prove external users don't call an API. It purposely excludes public, internal, protected, override, extern and entry-point-like members. False negatives are expected, and there may still be false positives for generated code or unusual declaration syntax.

Other initial diagnostics: unterminated quoted literals and unclosed block comments.
Compiler error-to-gutter mapping and complete semantic symbol resolution remain future work.

## Build on your machine

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
cargo run --release
```

The source was reviewed and packaged in an environment without `cargo`/`rustc`, so **these Rust build and GUI checks have not been executed**. The included `src/diagnostics.rs` has unit tests, but they too require running the commands above.

## Manual acceptance checklist

- 🔴 Open a 500+ line `.hy` file: line numbers follow the text while scrolling vertically.
- 🔴 Enable/disable word wrap and scroll a long line horizontally: gutter remains fixed.
- 🔴 Introduce an unclosed block comment: red `!`, hover description, underline.
- 🔴 Introduce an unclosed string: red `!`, hover description, underline.
- 🔴 Add `private void Unused() {}` in a loaded `.hyproj`: amber warning.
- 🔴 Reference `Unused` from a second `.hy` in the project: warning disappears after rescan.
- 🔴 Add a typed local such as `int onlyHere = 3;` inside a method: possible-unused hint.
- 🔴 Public methods should not receive unused hints merely because they have no calls in this project.
- 🔴 Remove a referenced file to make the project graph incomplete: no misleading unused hints.
- 🔴 Verify the existing `Ctrl+B`, `F1`, Build, Shift+Build, autocomplete, plugins and terminal remain functional.

The red markers mean **not locally verified**, not necessarily broken.
