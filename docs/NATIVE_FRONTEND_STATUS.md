# Quark IDE — Native Frontend Implementation and Release Checklist

**Updated:** 2026-11-03
**Branch:** Development  
**Current development version:** 0.6.0 (Hydrogen — Development project).
**Release status:** NOT READY — must verify native builds, GUI behaviour and the Hydrogen debugger.

The latest Linux CI repair removes an unused GTK import and makes the designer
layout tests Clippy-clean. Local Linux verification: native Clippy with warnings
denied, 189 native tests passed (3 ignored), and the feature register reports
217 applicable features with at least two tests each. The next target is a green
three-platform CI run followed by live GUI acceptance; the release gates below
remain open until those checks succeed.

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

## GTK editor preference refinements (branch agent/native-editor-preferences-live-20261102)

- GTK now sets actual Pango tab stops from Editor tab-width preferences (bounded 1–16) rather than persisting the value without applying it.
- An editor-owned weak-widget timer checks the shared preferences for changes and reapplies wrapping and tab stops without requiring an editor reload; it stops when the view is destroyed. Syntax highlighting remains theme-aware.
- Accurate scroll-synced line number gutters, direct AppKit/WinUI editor options and complete Settings parity still require implementation and visual review. This work does **not** close #93.

## Designer interaction progress (Development, 2026-11-02)

- The Linux native preview container is explicitly top-left aligned and the
  embedded window no longer adds an 18px outer inset. XML width and height
  still determine the preview's fixed window dimensions.
- GTK design-mode pointer presses are claimed before native controls receive
  their default click actions, and route to Properties selection. This needs
  live GTK verification, including nested controls, keyboard activation and
  focus. Do **not** describe all preview input as read-only yet.
- Properties contains an independent **Design page** chooser for a selected
  `Tabs` element. It uses the real `Tab.title` labels and a session-local
  selection; `Tabs.value` remains the application's explicitly editable
  startup tab. Added unit tests and feature-coverage entries.
- **Still required for #98:** prevent keyboard/IME-driven control edits and
  prove selection-only pointer/keyboard behaviour on AppKit and WinUI,
  confirm actual value inputs remain persistent across save/reopen.
- **Still required for #99 / #89:** replace the hard-coded simulated titlebar
  and fixed visual appearance with a theme-adaptive, reusable Hydrogen
  runtime renderer and compare real GTK/AppKit/WinUI screenshots against
  built apps. Current native widgets are not sufficient evidence of exact
  Hydrogen theme or tabs parity.
- All task-status changes await current CI and native manual acceptance.
  This is partial implementation, not closure of #98, #99 or #89.

## Approved UI technology policy — issue #100 (Development, 2026-11-02)

**The final production Quark IDE must use only GTK4/libadwaita (Linux),
WinUI 3 (Windows), and AppKit (macOS).** Do not introduce or retain a
cross-platform widget toolkit or independent drawing framework as a fourth
production UI technology. Native framework APIs own the window, widgets,
theme, menu, focus, keyboard/IME, docking and control layout on their OS.

Migration done in Development:
- `quark-native` is now the only default Cargo frontend (`native-ui`).
  The older `quark-ide` (egui/eframe) binary requires explicit `legacy-ui`
  and is retained only to migrate existing workflows and preserve tests.
- CI tests the two frontends with mutually exclusive features; native jobs
  never request the legacy UI feature.
- Hydrogen lexer and completion logic are in UI-independent
  `src/hydrogen_lexer.rs`, shared by the GTK and legacy editors, so native
  syntax no longer imports the egui renderer or theme module.
- Designer's embedded Linux tabs now use GTK4 StackSwitcher+Stack, the same
  native widget pair as Hydrogen's Linux runtime. Its Linux header uses the
  libadwaita HeaderBar and follows host styling; the embedded header is not
  an OS-managed window frame.
- Native Properties offers direct true/false and theme selectors and
  choice-index selectors; its XML validator rejects non-finite numbers,
  impossible ranges, invalid flags, and out-of-range choices.

**Migration not done:** Native shell/layout/control rendering still relies
on `naui` as a platform wrapper. This dependency is prohibited in the final
architecture and requires rewriting the remaining presentations directly in
GTK4, WinUI 3 and AppKit. The previous egui binary and dependencies must be
deleted after feature migration; defaulting to native does not satisfy the
framework-only requirement by itself. Desktop window decorations can only be
exact in real OS-managed windows; the standalone Hydrogen runtime window
remains the native visual authority. Native AppKit/WinUI widgets, read-only
keyboard handling, Hydrogen renderer sharing and screenshot comparisons
remain unverified or incomplete. Track the strict migration in
[#100](https://github.com/Aurora-softwares/Quark-IDE/issues/100).

## Hydrogen Studio layout parity — screenshot review, 2026-11-02

The latest user screenshot highlighted **container-level** differences even when
individual controls appeared OS-native. It is a layout regression, not a reason
to replace the approved GTK4/libadwaita, WinUI 3 or AppKit controls.

Development changes:
- Horizontal `RowPanel` width hints now preserve each child's explicit
  `preferredWidth`: the Studio Application settings and Your workspace
  panels are **600px** and **340px**, not two equal cells.
- `Panel` and `GridPanel` contents stretch to their allotted native width,
  and nested rows, columns and tab pages propagate that width consistently.
  Text input, sliders, selections and progress bars now follow Hydrogen's
  native `fill` defaults, rather than the designer's hard-coded 190px input.
- GTK labels use the runtime's left alignment rather than centred content.
- GTK `MenuBar` groups are shown as native `GtkMenuButton` controls (`Studio`
  and `Help` in this fixture); nested actions are not presented as a row of
  ordinary buttons. No Hydrogen events are invoked by the designer.
- The outer designer viewport has a separate 16px inset and theme-sensitive
  GTK shadow, without modifying the saved Hydrogen `Window.padding`.
- Added native designer unit tests for fixed preferred widths, flexible
  allocations, hidden children, runtime horizontal-fill defaults, menu
  semantics and the theme-aware inset.

**Acceptance remains open:** compare the same Studio.hyui file between
Quark's Design pane and Hydrogen's launched GTK application at 1120, 800
and 360 width. GTK `FlowBox` reflow as the window is resized and precise
AppKit/WinUI layout parity still require screenshot/interaction verification.
Do not treat these layout source changes alone as visual acceptance or full
cross-platform UI framework migration (#89 and #100).

## Required checks before any Release merge

Run on each supported OS, using a compatible Rust toolchain:

```bash
cargo fmt --all --check
cargo clippy --no-default-features --features legacy-ui --all-targets -- -D warnings
cargo test --no-default-features --features legacy-ui --all-targets
cargo clippy --no-default-features --features native-ui --bin quark-native -- -D warnings
cargo test --no-default-features --features native-ui --bin quark-native
cargo build --release --no-default-features --features native-ui --bin quark-native
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
