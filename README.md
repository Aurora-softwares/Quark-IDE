# Quark IDE — Rust native edition (v0.3.0)

A **native desktop** Hydrogen code editor written in **Rust**, using `egui/eframe`'s native OpenGL renderer. **No Python, JavaScript, Electron, Chromium, Tauri or webview**.

## Build

Install Rust using [rustup.rs](https://rustup.rs) and a platform-native compiler toolchain.

```sh
cargo run --release
cargo test
cargo build --release
```

Binary output: `target/release/quark-ide` on Linux/macOS, `target\\release\\quark-ide.exe` on Windows.

- Linux may require system X11/Wayland/OpenGL development dependencies. On Ubuntu/Debian, start with `sudo apt install build-essential pkg-config libxkbcommon-dev libwayland-dev libx11-dev libxcursor-dev libxi-dev libxrandr-dev libgl1-mesa-dev`.
- macOS: install Xcode Command Line Tools (`xcode-select --install`).
- Windows: use `rustup` with the MSVC toolchain and install Visual Studio C++ Build Tools.
- Build **on each target OS** for the corresponding executable.

## Features

- `.hy` and `.hyproj` edit, create, save, multiple tabs; keyboard shortcuts (`Ctrl/Cmd+N/O/S/F/P/W`, `F5`, `Ctrl+Shift+B`).
- Multiple workspace folders and tree-based file explorer.
- TOML `.hyproj` recursive project graphs, relative paths, missing-source warnings and cycle detection. Works with `project_references` across folders, as used by Australis.
- Git via installed `git` CLI: status, diff, fetch, pull, push and clone.
- Command output pane and shell-command input (individual commands, not a persistent PTY shell).
- Configurable compiler path. `hy` defaults to SDK. Commands are `hy build/check/run <path>`; `compile` command is meant for the self-hosted compiler. Pick the appropriate compiler executable in settings.
- Workspace and compiler settings persisted to a user-specific config folder.
- Native file/folder dialogs.

## Known limits — deliberate and documented

This is an **early native editor, not VS Code parity**. It does not yet include LSP, autocomplete, integrated debugger, actual PTY terminal, syntax-token highlighting, regex search, unsaved-tab close confirmations, or Git staging/commit UI. The command pane captures output after a command completes; it is not an interactive shell. `Ctrl+Shift+B` uses the configured compiler's `build` subcommand (not every compiler supports it). Opening a `.hyproj` shows referenced projects, but root selection for build is currently based on the active tab; open the root manifest tab to build it.

Do not confuse `hydrogen-stage1 compile <file>` with `hy build <project>` — the tools have different subcommands. Git integration uses your system Git credentials and SSH agents.

## Architecture

- `src/main.rs` — native editor UI, workspace handling, command runner and Git integration.
- `src/project.rs` — standalone, testable Hydrogen project graph resolver.

The project module is intentionally independent of the UI, suitable for future indexing, language server integration and dependency-aware build logic.

## Verification

`cargo`/`rustc` are not installed in the artifact-authoring environment. This Rust source was written but **has not been compiled or tested here**. Run `cargo test` and `cargo build --release` on a Rust-equipped machine; the first build will fetch crates.io dependencies.

## v0.3.0 compiler compatibility fix

Corrected 12 `Ui::close` usages to `Ui::close_menu` and updated the editor layouter closure and `LayoutJob::simple` parameters to match egui/eframe 0.31. This addresses the 14 errors reported from the 0.2.0 build. The complete GUI has not been compiled in this packaging environment (no Rust toolchain available).


## v0.3.0 changes and verification

- **View → Word wrap** toggle (`Alt+Z`), persisted in settings. Both-axis editor scroll area and unbounded layout width when wrapping is disabled. Very large files may still be slow with egui's full-buffer text layout.
- Basic Hydrogen lexical highlighting in a pure, tested Rust module (`src/editor.rs`): keywords, comments, strings, numbers and UTF-8 safety.
- Initial **local** completion popup (`Ctrl/Cmd+Space`), not LSP or cursor-aware completion. It appends the chosen completion to the document; this is a placeholder for real semantic completion.
- Shell / build command output is streamed line by line to the command pane. **This is not a PTY terminal**, and interactive stdin/full-screen terminal applications are not supported yet.
- Fixed the quick-open borrow-check failure seen in v0.2.1.
- `src/project.rs` includes tests for recursive references, cycles, external references, missing files and malformed manifests. `src/editor.rs` includes lexer/completion tests; `src/command.rs` includes spawn/output tests.
- CI checks Linux/macOS/Windows: formatting, clippy, tests, release build; manual acceptance checklist is in `scripts/manual-acceptance.md`.

### Release gate

```sh
./scripts/release-check.sh
```

On Windows, run `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets`, `cargo build --release` in PowerShell.

**Verification limitation:** no Rust toolchain is available in the package creation environment. This version's code and tests have *not* been compiled here. CI must pass on all three systems and manual acceptance tests must be completed before publishing a binary release. The test suite does not yet include automated UI driving, Git network tests, full Hydrogen compiler integration, or PTY/LSP tests. Do not claim complete feature verification based solely on unit tests.
