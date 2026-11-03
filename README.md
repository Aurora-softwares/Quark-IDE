# Quark IDE — Hydrogen Edition

The **Development** branch is a new implementation of Quark in **100% Hydrogen (.hy)** and **Hydrogen UI (.hyui)**. It does not use Rust, egui, GTK bindings written by Quark, JavaScript, Electron, or web UI. HYUI's platform-native runtime is provided by the Hydrogen compiler itself. The original Rust design and its history remain on [Development-RUST](https://github.com/Aurora-softwares/Quark-IDE/tree/Development-RUST).

## Current development version: 0.1.0 (experimental)

A Visual Studio-inspired shell: native menus and command toolbar, lightweight start screen, manifest-driven Solution Explorer (numbered file selector), two source tabs, Properties/Inspector, output panel and status bar. This first step deliberately stays close to the existing Quark layout. The new branch has its own codebase rather than Rust migration glue.

**Implemented with real Hydrogen event handlers:** open a .hyproj manifest, load and navigate its declared .hy sources and optional .hyui document, open standalone files, edit both buffers, Save / Save as / Save all, protect unsaved buffers when switching files or building, new source buffer, create a console or native UI starter project in an existing folder without overwriting existing starter files, find the first matching line in the current buffer, inspect the manifest, and run `hy check`, `hy build`, compiled console executables, or `hy ui` native preview. The command output is captured to `<project>.quark-command.log` and displayed in Quark. Build artifacts are written beside the project using the `<project>.quark-build` prefix.

**Current Hydrogen UI limitations:** Native collection widgets do not yet refresh their rows when the Hydrogen backend changes the `items` property. The Solution Explorer therefore displays a numbered manifest file list with an explicit Open action instead of a misleading dynamic ListBox. HYUI also caps the total event value snapshot at around **24,000 characters** across live inputs, both editor buffers and output. Larger files need a dedicated document/editing protocol before this is suitable for everyday development. Compiler process execution is synchronous; long-running builds and UI previews may block editor interaction until they exit.

**Not yet implemented:** multiple dynamically created tabs, project-wide file enumeration, full solution/project trees, integrated terminal, language server, lexically coloured text, line-number gutter, semantic diagnostics, completion, debugger, breakpoints, refactoring, drag-and-drop UI designer, docking, recent-project persistence, autosave/recovery, build cancellation, and true asynchronous process execution. UI preview launches the *real compiled application*, not an embedded design-time drawing. Host process launching is synchronous, so the IDE may be temporarily unresponsive while a long build or interactive preview runs; this needs an async process API.

### Build and run

The compiler must be bootstrapped from [Hylang-Compiler / Development](https://github.com/Aurora-softwares/Hylang-Compiler/tree/Development), with its native HYUI SDK and runtime installed. From this checkout:

```sh
hy build Quark.hyproj -o build/Quark
./build/Quark
```

On Windows the compiled executable has an `.exe` suffix; macOS packages native UI applications as `.app` bundles. Use the target SDK for each host as described in the Hydrogen compiler's `docs/hydrogen-ui-builds.md`. Quark's `Quark.hyproj` uses the installed compiler's current-host default target.

Quark includes the upstream `Hydrogen.UI.Application` bridge as plain Hydrogen source (`src/Hydrogen/UI/Application.hy`) to avoid a hard-coded machine-local `project_references` path. Keep this copy in sync with the compiler's `libraries/Hydrogen.UI/Application.hy`. New native UI starters include their own minimal bridge, so they are self-contained.

On the Start tab, choose a project or file. For a new app, choose a valid project name and an **existing directory**. Tick Hydrogen UI to include `Main.hyui`, `Program.hy`, and the starter event bridge. Files are not silently overwritten.

Set the compiler executable (default: `hy`) in the Properties pane, then Save, Check, Build, Run or Preview. On macOS/Windows, use the compiler and native UI SDK produced for that OS. Check, Build and Preview invoke the CLI and report real exit status; no build succeeds just because a button was clicked.

### Tests

```sh
python3 -m unittest discover -s tests -p 'test_*.py'
python3 scripts/check-feature-coverage.py
bash tests/native-smoke.sh
```

The Python tests are **structural and contract tests**. The native smoke script (requiring a bootstrapped `hy` compiler and HYUI SDK) builds and executes a real Hydrogen backend test before packaging Quark itself; it does not launch an interactive desktop window. Passing any of these tests does not prove native UI behaviour. Native gates still require the Hydrogen compiler, GTK4/WinUI/AppKit runtime, interactive file picking and text editing, a real build, and a starter UI button event on each host. The compiler's own HYUI preflight test should also be run.

### Follow-up / release gates

🟢 Rebuild native shell in Hydrogen/HYUI; use a real host-specific window and event bridge.

🟢 File opening/saving, starter projects, search, compiler check/build, output capture, and native UI preview are wired to backend code.

🟡 Verify compiler and runtime integration on Linux, macOS, and Windows; ensure shell execution never freezes the editor. Accept only native interaction smoke tests as evidence.

🔴 Rich editor API (syntax decorations, line numbers, completion, multiple documents), asynchronous process/log streaming, navigation, diagnostics and real UI designer.

🔴 Safe save/recovery, external modification conflict handling, preferences, Git, proper debug adapter and tests before claiming parity with Rust Quark.

Development work belongs to the [Hydrogen — Development project](https://github.com/orgs/Aurora-softwares/projects/4). Version 0.1.0 is an experimental starting point, not a production release.
