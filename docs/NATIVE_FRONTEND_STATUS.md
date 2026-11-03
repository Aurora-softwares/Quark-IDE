# Quark IDE — native frontend status and release gates

**Updated:** 2026-11-03
**Branch:** `Development`
**Current development version:** `0.6.0`
**Release status:** 🟡 In Progress — the HYUI structure editor and native interactive starter are committed; updated CI and native visual/interaction acceptance remain open.

This is the current development record for the Quark frontend. The [Hydrogen — Development project](https://github.com/orgs/Aurora-softwares/projects/4) tracks issue status. A Cargo version or historical tag does not establish a qualified release.

## Verified checkpoint

- 🟢 The default Cargo frontend is `quark-native`; the previous egui frontend requires `--no-default-features --features legacy-ui`. Shared Hydrogen lexical logic is independent of either UI.
- 🟢 [CI run 41605444994](https://github.com/Aurora-softwares/Quark-IDE/actions/runs/41605444994) passed all six quality/native jobs on Ubuntu, macOS and Windows at `635ccc7`. These jobs cover formatting, Clippy, unit tests, feature-register validation and release builds. Ubuntu native CI also runs GTK startup and selected ownership regressions under Xvfb.
- 🟢 Local Linux checks at `be38e4d`: 450 legacy tests passed, 189 native tests passed with 3 ignored, 11 Python feature-gate tests passed, native and legacy Clippy passed with warnings denied, 217 applicable registered features had two tests each, and the optimized native binary built.
- 🟡 `Start Without Debugging` builds the selected Hydrogen project and launches a host console or native UI artifact. The local bootstrap `hy` built and ran a console sample. Its UI sample could not launch because that compiler binary lacked the optional HYUI host. Packaged UI launch still needs a matching SDK on each target OS.

## Current native implementation

| Area | Implemented in Development | Remaining acceptance |
| --- | --- | --- |
| Shell | Start Window, document tabs, native menus/controls through transitional `naui`, OS-specific GTK/libadwaita, AppKit and WinUI presenters | Remove `naui` and legacy egui for the final production architecture; real UI interaction on all hosts ([#100](https://github.com/Aurora-softwares/Quark-IDE/issues/100)) |
| Editor and project | Hydrogen source editor, Solution Explorer, safe selected-row file actions on Linux, project Build/Run and streamed Output | Native editor option parity, large-workspace behavior, cross-platform file actions and packaged UI execution ([#93](https://github.com/Aurora-softwares/Quark-IDE/issues/93)) |
| Docking | Shared zone/stack/pin model and GTK drag, overlay, auto-hide and floating work | Direct AppKit/WinUI presenters, resizing, persistence and manual drag/focus acceptance ([#88](https://github.com/Aurora-softwares/Quark-IDE/issues/88)) |
| Designer | `.hyui` Design/XML views, responsive layout containers, GTK toolbox drag/drop, safe nested insertion/reordering/reparenting/deletion, collapsible Inspector with native supported properties, separate interactive OS Window and layout-only mode, runnable New UI App scaffolding | CI verification, WinUI 3/AppKit gesture parity, direct visual manipulation, runtime anchors/per-state styling in Hydrogen ([#98](https://github.com/Aurora-softwares/Quark-IDE/issues/98), [Hydrogen #185](https://github.com/Aurora-softwares/Hylang-Compiler/issues/185)) and actual renderer parity ([#89](https://github.com/Aurora-softwares/Quark-IDE/issues/89)) |
| Settings | Versioned shared preferences, atomic writes, compiler/theme controls, GTK live wrap/tab-width and Build save/output choices | Line-number gutter, direct portable presenter behavior, reset and picker acceptance ([#93](https://github.com/Aurora-softwares/Quark-IDE/issues/93)) |
| Terminal | Persistent PTY/ConPTY session, VT parsing, GTK key forwarding | Direct AppKit/WinUI keyboard/IME and VT grid, lifecycle and full-screen program tests ([#92](https://github.com/Aurora-softwares/Quark-IDE/issues/92)) |
| Debugging | Debug panel and command placement | Compiler debug metadata/transport, DAP adapter, breakpoints and stepping; Start Debugging remains disabled ([#91](https://github.com/Aurora-softwares/Quark-IDE/issues/91)) |

## Studio HYUI side-by-side screenshot review — 2026-11-03

The user compared Quark's embedded Studio designer to the **same built Hydrogen GTK application**. The reported differences were at the container level: Studio/Help appeared as two full-width, vertically stacked dropdowns instead of a compact horizontal menu; card headings and checkboxes were centred; settings and summary cards had the wrong spacing and width; card borders/padding and primary button colour differed; and labels were styled with Quark's palette rather than Hydrogen's GTK role classes.

Changes committed on `Development` (acceptance still pending):
- 🟡 **Menus:** a dedicated horizontal GTK box containing non-expanding `GtkMenuButton` siblings, selected in design mode without dispatching Hydrogen actions.
- 🟡 **Cards:** real native `GtkFrame` with the `card` style class and in-frame margins matching Hydrogen's `Panel.padding`; panel titles and checkboxes/switches are left-aligned.
- 🟡 **Proportions:** preferred widths are minimums rather than exact fixed sizes. The Studio settings panel requests 600px, the workspace panel at least 340px and grows into remaining space; when both cannot fit, RowPanel stacks even if the configured breakpoint has not yet been reached.
- 🟡 **Theme fidelity:** the Linux designer applies Hydrogen's `title-1`, `title-3`, `caption`, `dim-label`, `accent`, `heading` and `suggested-action` GTK classes directly rather than painting or guessing equivalent text/button styles.
- 🟢 **Regression coverage added:** pure unit tests for preferred widths, overflow/wrap, menu grouping, style class mappings and primary button semantics. Linux CI now includes display-backed Xvfb tests that inspect actual GTK menu orientation and card/child padding. Those jobs must pass on the *current* commit before any new green-dot verification claims.

**Still outstanding:** visually compare both screenshots again after these changes at 1120px, 800px and 360px; verify native title frame, content layout, keyboard/read-only handling and responsive reflow. Windows/macOS still require their own native AppKit/WinUI presenter acceptance, and the transitional `naui` abstraction must be removed for the final approved architecture (#100). Embedded decoration is an approximation: a separately launched Hydrogen OS window is authoritative.



## New responsive designer and Inspector — development checkpoint

The nested HYUI structure editor now supports adding actual Label, Button, Input, Select, CheckBox, Slider and other controls to `Window`, `Panel`, `ColumnPanel`, `RowPanel`, `GridPanel`, `ScrollView` and `Tab`. It uses HYUI element-child paths, preserving existing element IDs, event handlers and source text. Dropping onto a container inserts inside it; a selected leaf inserts beside it. The Linux GTK toolbox supports native dragging, and the Inspector exposes Move Earlier/Move Later/Delete and drag-to-reparent. Mutations are deferred until native widget handlers return, preventing widget unparenting within drag/click callbacks. Native New UI App scaffolding produces a Hydrogen `.hyproj`, `Main.hyui`, interactive `Program.hy` and a bundled source library. The Inspector has expandable/collapsible sections that retain state and typed supported Hydrogen style selectors.

The Design toolbar now has **Document / Phone 360 / Tablet 800 / Desktop 1120** responsive viewport presets that reflow the native preview without changing the persisted `Window.width`. A stacked responsive panel shrinks to the preview allocation rather than overflowing merely because its preferredWidth is larger. The model's `apply_designer_source` method validates generated Inspector and drag/drop edits atomically: rejected edits leave the previous valid document untouched while manual XML typing can still hold incomplete source.

New regression tests cover nested insertion, order, parent changing, event-handler retention, invalid drop targets, safe deletion, responsive rows/grids and scroll containers, drag payload isolation, Inspector fold-state independence, no-overwrite starter generation, viewport width overrides and atomic source mutation safety. They are registered in `tests/feature-coverage.json`. These tests do not substitute for real drag/selection/resize acceptance on all three desktops.

**Layout/styling compatibility:** Hydrogen HYUI v1 does **not** support arbitrary x/y/anchor-point positioning, dynamic responsive min/max geometry, or per-state (hover/focus/active/selected/disabled) colours and fonts. Quark must not generate unknown attributes that break compilation. [Hydrogen #185](https://github.com/Aurora-softwares/Hylang-Compiler/issues/185) is the language/parser/native-renderer dependency for those features. Until then, responsive design is expressed using RowPanel/ColumnPanel/GridPanel/ScrollView, `preferredWidth`, `fill`, `breakpoint`, `gap` and other supported properties.

**Current acceptance:** structure editor implementation 🟡 (CI and manual testing outstanding); GTK drag/drop 🟡 (interactive desktop test outstanding); native Inspector 🟡 (platform acceptance outstanding); WinUI 3/AppKit drag/drop 🔴; anchor / full per-state style and font support 🔴 (Hydrogen dependency); no fourth UI framework permitted (Quark #100).

## Next phase — platform parity and live GUI acceptance

1. 🟡 **Native interaction gate — [#94](https://github.com/Aurora-softwares/Quark-IDE/issues/94):** run and record real Start Window, file open/save, editor, Settings, Solution Explorer, dock and terminal behavior on Ubuntu, macOS and Windows. Automated Xvfb startup proves a mapped GTK window, not full interactive correctness.
2. 🟡 **Native Settings — [#93](https://github.com/Aurora-softwares/Quark-IDE/issues/93):** finish line-number, tab/wrap, immediate theme/density and build preference behavior with restart and invalid-config tests on all hosts.
3. 🟡 **Dock and terminal parity — [#88](https://github.com/Aurora-softwares/Quark-IDE/issues/88), [#92](https://github.com/Aurora-softwares/Quark-IDE/issues/92):** implement direct AppKit/WinUI interaction and validate keyboard, focus, resize and native lifetime.
4. 🟡 **Designer and native architecture — [#98](https://github.com/Aurora-softwares/Quark-IDE/issues/98), [#99](https://github.com/Aurora-softwares/Quark-IDE/issues/99), [#89](https://github.com/Aurora-softwares/Quark-IDE/issues/89), [#100](https://github.com/Aurora-softwares/Quark-IDE/issues/100):** make design mode safe, compare standalone Hydrogen renderers, and replace transitional UI wrappers with direct OS frameworks.
5. 🔴 **Source debugger — [#91](https://github.com/Aurora-softwares/Quark-IDE/issues/91):** agree and verify Hydrogen debug metadata/transport before enabling Quark debugger controls. Australis OS images need separate QEMU/remote debugging; they are not host executables.

## macOS launch reliability — current Development

- 🟢 **AppKit Start Window → IDE crash resolved in CI:** A foreign Objective-C exception (`Rust cannot catch foreign exceptions`) was reproduced during the transition into an empty or existing Hydrogen workspace. The root cause was clearing the native Properties pane and reparenting its already-attached AppKit header while constructing the IDE. The header now retains its original NSView parent; Quark mounts the editor content separately. The Start Window is kept alive while the IDE is constructed in a distinct AppKit NSWindow, then closed via a deferred UI task. [CI run 41651996180](https://github.com/Aurora-softwares/Quark-IDE/actions/runs/41651996180) passed **all six Windows/Linux/macOS quality and native UI jobs**, including the real AppKit Start Window lifetime, blank-workspace and existing-project launch tests, bundled app packaging and signature validation. This resolves the automated macOS launch gate; **hands-on Finder/visual/keyboard acceptance remains outstanding**.
- 🟡 **Distributable macOS package:** `scripts/package-macos-app.sh` creates `dist/Quark IDE.app`, adds a native `Info.plist`, verifies an ad-hoc code signature, and archives it in `Quark-IDE-macOS.zip` using `ditto`. This retains Unix executable permissions, unlike a bare GitHub Actions binary download. The macOS CI job extracts and checks the ZIP before upload.
- 🟡 **Hydrogen UI application launch:** Existing Quark Run commands launch a fresh native `.app` bundle through macOS LaunchServices, with tests guarding missing bundles and unsafe symlinks. Full AppKit application-content, exit/crash and Hydrogen event acceptance still requires hands-on desktop validation.
- 🔴 **Signing and installation:** Ad-hoc signing is for local/CI validation only. Developer ID signing, notarization and a release installer are not yet done; do not present this as a Gatekeeper-cleared public release.
- 🔴 **Actual macOS GUI acceptance:** Confirm Finder open, launcher navigation, project creation, native `.hyui` design editing, packaged Hydrogen UI events, restart and clean quit on both Apple Silicon and Intel (or Rosetta as appropriate). GitHub CI smoke is an additional guard, not full visual acceptance.

## Remaining release gates

- 🔴 Record interactive GUI acceptance, accessibility, focus, high-DPI, theme, resizing, file pickers and native window behavior on each OS. Use the [manual acceptance matrix](../scripts/manual-acceptance.md); mark only observed cases complete.
- 🔴 Build and launch console and packaged HYUI applications with a matching Hydrogen SDK on all three hosts; confirm errors and process exit behavior.
- 🔴 Complete #88, #89, #91, #92, #93, #98, #99 and #100 to their issue acceptance criteria. CI compilation alone does not close them.
- 🔴 Run representative large-workspace, long-output, terminal and clean-shutdown tests, then qualify a release candidate with source revision and artifacts.

## Required checks before Release

```sh
cargo fmt --all -- --check
cargo clippy --no-default-features --features legacy-ui --all-targets -- -D warnings
cargo test --no-default-features --features legacy-ui --all-targets
cargo clippy --no-default-features --features native-ui --bin quark-native -- -D warnings
cargo test --no-default-features --features native-ui --bin quark-native
python3 -m unittest discover -s scripts -p test_feature_coverage.py
python3 scripts/check-feature-coverage.py
cargo build --release --no-default-features --features native-ui --bin quark-native
```

Run these in CI on Ubuntu, macOS and Windows, then complete the native interaction matrix. Keep each issue In Progress until its implementation and platform-specific acceptance both succeed.
