# Quark IDE — Native Frontend Implementation and Release Checklist

**Updated:** 2026-11-02  
**Branch:** Development  
**Release status:** NOT READY — must verify native builds, GUI behaviour and the Hydrogen debugger.

This is the source of truth for the current implementation after the native Settings, panel, Hydrogen, terminal, and visual-design work. A checked-in implementation is **not** a successful runtime test. Do not treat this file as a claim that all features are finished.

## Implemented in code

| Capability | Current state | Verification needed |
| --- | --- | --- |
| Native AppKit, WinUI 3, GTK4 frontend (naui 0.8) | Existing native binary and shell | Build + launch on three OSes |
| Appearance, contrast and density refinements | Theme-aware selected document/bottom tabs, light/dark adjustments, compact Solution Explorer and dock headers | Light/dark screenshots, accessibility contrast |
| Source and designer UI | Hydrogen source editor, portable .hyui designer, XML source, property selection | Verify interaction and saved state |
| Solution Explorer | Nested directory tree, file/folder icons, list view, A–Z/Z–A and search | Large-project performance, selection/expand persistence |
| Shared docking state | 12 tool IDs, zones, order, pin/collapse, JSON persistence, reset | Saved-layout migration and round-trip |
| GTK docking presenter | Drag/drop between hosts, tab rails, auto-hide GTK popover, floating native windows | Runtime drag/focus/close test; current implementation retains some legacy fixed splits |
| Toolbox / Symbols / Solution | Pinned defaults and migrated saved layouts | Manual layout reset and focus |
| Output / Errors / Tests / Debug display | Dark ANSI-aware Output, compact Error List filtering/folding, Test pass/fail colouring, Debug console styling | Long-output streaming and font/scrolling tests |
| Hydrogen compiler | Manually configured executable takes priority over environment and PATH, compiler validation and diagnostics | Build, run, check, fmt, tests, publish with an actual Hydrogen build |
| Linux Settings | Shared versioned preferences; GTK Editor and Build switches persist; source wrapping applied when editor opens; Debugger remains disabled | CI and interactive preference refresh, tab width and line-number rendering |
| macOS/Windows Settings | Portable naui settings window supports compiler/theme plus persistent Editor and Build toggles using the shared schema | AppKit/WinUI validation and connecting all editor rendering options |
| Terminal backend | Persistent portable-pty session shared across Linux/macOS/Windows; VT100 parsing | Interactive session and reliable shutdown |
| Linux terminal input | GTK native key forwarding, special keys, viewport resize | Real TUI app in PTY |
| macOS/Windows terminal input | Live typing forwarded to PTY plus explicit Esc/Tab/arrows/Ctrl/resize controls | Full platform-native key event support and VT grid display |

## Not yet complete: release-blocking work

1. **Source-level Hydrogen debugger — [#91](https://github.com/Aurora-softwares/Quark-IDE/issues/91).** Debug pane is present; Start Debugging intentionally remains disabled. Hydrogen must expose stable debug metadata/transport before Quark can claim breakpoints, stepping, watches or stack frames.
2. **Truly interchangeable docking on all OSes — [#88](https://github.com/Aurora-softwares/Quark-IDE/issues/88).** Current live-widget reparenting and overlays are GTK-specific. AppKit and WinUI presenters need equivalent native behaviour. All dock interactions need manual and automated verification.
3. **Full macOS/Windows PTY interaction — [#92](https://github.com/Aurora-softwares/Quark-IDE/issues/92).** A live PTY exists, but a text input + special-key buttons do not replace direct native keyboard/IME, cursor presentation, alternate-screen and per-cell colour rendering.
4. **Remaining Editor/Debugger Settings parity — [#93](https://github.com/Aurora-softwares/Quark-IDE/issues/93).** GTK editor, build and debugger preferences include disabled placeholders. The portable settings UI covers compiler and appearance only.
5. **Native GUI smoke tests and release gate — [#94](https://github.com/Aurora-softwares/Quark-IDE/issues/94).** A green compile/test is necessary, not sufficient; startup, file-open, panel and terminal lifecycle need real GUI testing.

## Hydrogen and Australis OS scope

A selected host Hydrogen compiler is used to build an Australis `.hyproj`. The Australis OS output is **not a normal desktop executable**. Debugging its kernel requires a dedicated remote/QEMU transport, separate from a debugger for Linux/macOS/Windows programs. A Hydrogen compilation error that identifies a missing Australis child project is not a Quark process-launch error; Quark must display the compiler's exact message unmodified.

## In-progress implementation (branch agent/native-settings-and-terminal-reliability-20261102)

- Added `src/native_settings.rs`, a versioned preference schema shared between GTK and AppKit/WinUI, with validation, legacy defaults, atomic writes, and rejection of corrupt or newer-version settings.
- Build/Run now consults Save Before Build and Show Output; when enabled, it saves all modified documents using conflict-checked writes. When disabled, unsaved buffers cause an actionable refusal rather than silently building stale sources.
- GTK Editor preferences and portable Editor/Build controls can be saved. GTK word wrapping is applied on editor construction. **Dynamic wrap updates, accurate gutter line-number visibility, tab-width rendering, and full portable UI parity remain unfinished.**
- These commits require CI and manual platform smoke before being described as validated. They do not complete #88, #91, #92, #93 or #94.

## Required checks before any Release merge

Run on each supported OS, using a compatible Rust toolchain:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo clippy --features native-ui --bin quark-native -- -D warnings
cargo test --features native-ui --bin quark-native
cargo build --release --features native-ui --bin quark-native
```

Then launch the native binary and manually verify:
- Light/Dark tabs and Solution Explorer row density/selection.
- Source open/save and `.hyui` Design/XML/Properties.
- Compiler-path selection and Build / Run / Check / Format / Test.
- GTK docking (pin, float, drag, auto-hide), saved layout, Reset.
- Interactive PTY shell: multiple commands share process state, Ctrl+C, resize and full-screen programs.
- Open/close Settings and native file pickers on macOS (guard against Objective-C exceptions).

**CI is tracked in GitHub Actions.** Mark this checklist verified only after successful run IDs and manual evidence are recorded.

## Recommended milestone order

**P0 — Stabilize:** Bring all GitHub Actions quality/native UI jobs to green, fix any runtime crashes and regression tests.

**P1 — Complete platform parity:** Finish native Settings, AppKit/WinUI dock presenters, and native terminal key/rendering integration.

**P2 — Compiler/debugger contract:** Add Hydrogen debug information, a DAP adapter and Quark breakpoint/step/stack/variables UI, keeping Australis remote debugging separate.

**P3 — Release validation:** Automated GUI startup smoke tests, themed screenshots, workspace migration, large-project performance, clean shutdown, and a release candidate.

Work is complete only when both the implementation and its acceptance tests succeed.
