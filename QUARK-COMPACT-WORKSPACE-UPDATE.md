# Quark IDE — Compact Workspace + Editor QOL update

Based on the **plugin/explorer/Hydrogen-build source** package, version 0.3.0.
The original plugin manager, `.hyproj` project selection, and Build / Shift+Build contract remain in place.

## Layout

- **Ctrl+B**: toggle the complete left Explorer.
- **F1**: toggle the lower Terminal.
- Both visibility flags are saved to `settings.json` and older settings default to both panels visible.
- Small Explorer/Terminal icon buttons are in the top menu; each panel also has an `×` close button.
- Explorer title, collapse-all, expand-all and refresh use compact controls.
- Sidebars and terminals remain resizable; tab close buttons and a small new-file action remain in the tab strip.
- Reduced global widget padding and spacing; status displays caret line/column, UTF-8 and concise command status.
- Selected file is highlighted in the folder tree; `.quark` build artefacts are omitted from navigation.

## Editor shortcuts and QOL

| Shortcut | Behaviour |
|---|---|
| Ctrl+B | Hide/show Explorer |
| F1 | Hide/show Terminal |
| Ctrl+F | Inline Find, with focus on search field |
| F3 / Shift+F3 | Next / previous search match, wrapping at file boundaries |
| Ctrl+H | Show Find and Replace |
| Ctrl+G | Go to line |
| Ctrl+Tab / Ctrl+Shift+Tab | Next / previous open document |
| Ctrl+Shift+D | Duplicate current source line beneath it when editor has focus |
| Ctrl+Shift+S | Save all dirty tabs |
| Alt+Z | Toggle word wrap (unchanged) |
| Ctrl+Shift+B | Build selected Hydrogen project (unchanged) |

- Find/Replace is inline, not a floating search window. **One** replaces the selected or next exact match; **All** replaces all case-sensitive exact matches.
- Separate Quick Open query prevents its search text from overwriting the editor's Find query.
- Go to Line, selection and duplicate-line operations use character positions, not raw byte indices (works with UTF-8), and duplication preserves CRLF.
- Existing Hydrogen syntax colouring and completion remain in the editor.

## Local verification

The source ZIP and baseline patch were generated without running Rust tooling. **Cargo, rustc and rustfmt are not available in the build environment.** Run these on Chimera:

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
cargo run --release
```

The manual GUI acceptance checklist is extended in `scripts/manual-acceptance.md`. If formatting changes files, include that formatting in your commit.

### Notes / limits

- Quark still uses a command-output panel rather than a true PTY terminal.
- Find/Replace is case-sensitive; there is no project-wide index or regex search yet.
- Programmatic text actions still use direct string edits, so their interaction with egui's undo stack needs interactive validation before a public release.
- The update has not been pushed to GitHub; it only modifies the delivered source ZIP / patch.
