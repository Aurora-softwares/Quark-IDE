# Quark IDE — native frontend status and release gates

**Updated:** 2026-11-03
**Branch:** `Development`
**Current development version:** `0.6.0`
**Release status:** 🟡 In Progress — development CI is green; native interaction and release acceptance remain open.

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
| Designer | `.hyui` Design/XML views, typed Properties, independent design-time tab choice, GTK layout fixes for Studio widths and runtime fill defaults | Read-only input and visual parity across all OSes; embed Hydrogen's actual renderer ([#98](https://github.com/Aurora-softwares/Quark-IDE/issues/98), [#99](https://github.com/Aurora-softwares/Quark-IDE/issues/99), [#89](https://github.com/Aurora-softwares/Quark-IDE/issues/89)) |
| Settings | Versioned shared preferences, atomic writes, compiler/theme controls, GTK live wrap/tab-width and Build save/output choices | Line-number gutter, direct portable presenter behavior, reset and picker acceptance ([#93](https://github.com/Aurora-softwares/Quark-IDE/issues/93)) |
| Terminal | Persistent PTY/ConPTY session, VT parsing, GTK key forwarding | Direct AppKit/WinUI keyboard/IME and VT grid, lifecycle and full-screen program tests ([#92](https://github.com/Aurora-softwares/Quark-IDE/issues/92)) |
| Debugging | Debug panel and command placement | Compiler debug metadata/transport, DAP adapter, breakpoints and stepping; Start Debugging remains disabled ([#91](https://github.com/Aurora-softwares/Quark-IDE/issues/91)) |

The Studio designer currently honors explicit `RowPanel.preferredWidth` values of 600px and 340px, propagates container width, follows native horizontal fill defaults and renders GTK menu groups as menu buttons. Its embedded title frame remains a design-time approximation; the separately launched Hydrogen window is the visual authority. The GTK layout changes have unit coverage but still need matched screenshots at 1120, 800 and 360 pixels on all supported platforms.

## Next phase — platform parity and live GUI acceptance

1. 🟡 **Native interaction gate — [#94](https://github.com/Aurora-softwares/Quark-IDE/issues/94):** run and record real Start Window, file open/save, editor, Settings, Solution Explorer, dock and terminal behavior on Ubuntu, macOS and Windows. Automated Xvfb startup proves a mapped GTK window, not full interactive correctness.
2. 🟡 **Native Settings — [#93](https://github.com/Aurora-softwares/Quark-IDE/issues/93):** finish line-number, tab/wrap, immediate theme/density and build preference behavior with restart and invalid-config tests on all hosts.
3. 🟡 **Dock and terminal parity — [#88](https://github.com/Aurora-softwares/Quark-IDE/issues/88), [#92](https://github.com/Aurora-softwares/Quark-IDE/issues/92):** implement direct AppKit/WinUI interaction and validate keyboard, focus, resize and native lifetime.
4. 🟡 **Designer and native architecture — [#98](https://github.com/Aurora-softwares/Quark-IDE/issues/98), [#99](https://github.com/Aurora-softwares/Quark-IDE/issues/99), [#89](https://github.com/Aurora-softwares/Quark-IDE/issues/89), [#100](https://github.com/Aurora-softwares/Quark-IDE/issues/100):** make design mode safe, compare standalone Hydrogen renderers, and replace transitional UI wrappers with direct OS frameworks.
5. 🔴 **Source debugger — [#91](https://github.com/Aurora-softwares/Quark-IDE/issues/91):** agree and verify Hydrogen debug metadata/transport before enabling Quark debugger controls. Australis OS images need separate QEMU/remote debugging; they are not host executables.

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
