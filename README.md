# Quark IDE — Hydrogen Edition

Quark's **Development** branch is a new IDE written in Hydrogen (`.hy`) with
its interface defined in Hydrogen UI (`.hyui`). The previous Rust implementation
remains available on [Development-RUST](https://github.com/Aurora-softwares/Quark-IDE/tree/Development-RUST).

## Current development version: 0.2.0 — separate Start and IDE windows

Quark now has **two separate native desktop applications**, not two pages inside
one window:

- **Quark Start Window** (`Quark.hyproj` / `Quark.Start.hyui`): a compact,
  lightweight launcher with recent projects, search, Open Project, Open Folder,
  Open Source / HYUI, a native starter-project wizard, GitHub clone, and
  Continue Without Code.
- **Quark IDE** (`Quark.IDE.hyproj` / `Quark.hyui`): a full-size
  Visual Studio-inspired workspace with native menu/toolbar, a left toolbox,
  centre code/HYUI editors, right Solution Explorer and Properties,
  and a bottom Output / Error List / Debug / Tests panel.

Selecting an existing project, new project, folder or file records a *launch
request*, starts a **separate QuarkIDE process**, and then issues
`Hydrogen.UI.Application.Exit()` to close the Start Window. The IDE reads the
request in its `onLoad` handler and loads the correct workspace. **Return to
Start** refuses to close if there are unsaved changes. The handoff contains
only the type and path, never unsaved source code.

The project still uses **100% Hydrogen and Hydrogen UI for Quark application
code**. The host-native GTK4, AppKit or WinUI runtime is supplied by Hydrogen,
not by Rust or Quark-specific C++.

### Linux: build and launch

This version needs the **latest Hylang-Compiler Development** checkout, because
the native HYUI runtime was extended with `Application.Exit()`, separate GTK
application instances, and native GTK `TextArea` text updates. The compiled
Hydrogen Stage 1 must be available on PATH as `hy`, or via `HY`.

```sh
git checkout Development
git pull
HY=/path/to/Hylang-Compiler/build/self_hosting/linux/hydrogen-stage1 bash scripts/build.sh
bash scripts/run.sh
```

The build script compiles **both** `build/QuarkIDE` and `build/Quark`.
Run the launcher from the repository root (`scripts/run.sh` handles this).
The two applications currently exchange the selection via
`build/.quark-open-request`, and persist up to ten recent paths in
`build/.quark-recent`. These are local developer workspace files and should
not be committed.

A direct build of `Quark.hyproj` creates **only the Start Window**. Always run
`scripts/build.sh` before testing the two-window flow. On Linux, the IDE is
detached with `nohup` before the Start Window exits.

### Windows and macOS

Both native UI manifests can be packaged with a host-specific Hydrogen SDK.
The HYUI `Exit()` protocol is now implemented for the legacy native host and
the Windows WinUI runtime too. Quark starts the macOS bundle's native
`hyui-launcher` from the repository root and uses that same entry point for the
second window, retaining the working directory needed by the handoff file.
The [Apple Silicon CI job](https://github.com/Aurora-softwares/Quark-IDE/actions/runs/42610383401/job/129136960593)
built both arm64 AppKit bundles, launched each native window, and exercised
Quark's Build and Run service against real arm64 console and HYUI projects. The full
Start-to-IDE handoff and interactive controls still need macOS validation;
Windows companion launch also remains open.

The IDE's Properties panel has a **Build target** selector. Host CPU remains the
default. On macOS, **macOS arm64 (Apple Silicon)** passes `--target macos-arm64`
to Hydrogen's C++ bootstrap compiler for console and HYUI project builds.
Point the compiler field to the Development source checkout's `build/arm64/hy`;
HYUI projects also need its `hyui` tool and exported `macos-arm64.app` runtime.
On an Intel Mac, Build can cross-compile arm64 output but Run asks you to move
it to Apple Silicon. On Apple Silicon, Run executes a console binary or opens
the packaged `.app`. Preview continues to use the host UI runtime. Quark's
target selection has native Linux regression tests and an Apple Silicon CI
Build/Run test for both console and AppKit projects.

### Runtime file-path compatibility

The early native Hydrogen Linux runtime called `rt_create_parents` by
temporarily editing the caller's string, which could segfault on a read-only
ELF string literal. Quark still constructs session file paths through
`Workspace.Join` for compatibility with older compiler builds. Hydrogen's
Development runtime has also been corrected to copy path strings to writable
stack memory before creating directory components.

The launcher writes native startup logs to `build/.quark-ide.log` and
`build/.quark-start.log`; these are ignored by Git and can be inspected if
window startup fails. Build both executables again after updating Hydrogen.

### Functional workspace backend

The IDE can open `.hyproj`, `.hy` and `.hyui` files, navigate source paths
declared in a Hydrogen manifest, edit and save named documents, protect
unsaved buffers when opening another file or changing workspaces, create
starter console and native UI projects, search and replace within either active editor buffer,
inspect the manifest, and invoke Hydrogen `check`, `build`, run-without-
debugging, and actual native UI preview. Build logs and exit codes are shown
in Output. The project remains strict about not presenting Run as Debug.

The toolbox and bottom Error/Debug/Test panes visually reserve space for
future capabilities; tool selection is informational and **is not a real
drag-and-drop designer**.

**Hydrogen UI limitations:** native collections do not currently repaint
dynamic `items` values, so Solution Explorer and recent projects use
working numbered lists. HYUI's event protocol still caps snapshots at
about 24,000 characters shared across live fields; larger documents need
a dedicated source-editor backend. Processes run synchronously inside
Hydrogen event callbacks, so compiler commands can temporarily block the
IDE and a long-running preview can wait for the preview window to close.
A source-level debugger, rich editor (including cursor jump-to-match), Git staging, persistent docking and
full source recovery are not implemented.

### Tests

```sh
python3 -m unittest discover -s tests -p 'test_*.py' -v
python3 scripts/check-feature-coverage.py
HY=hy bash tests/native-smoke.sh
```

The Python tests validate XML and source contracts; the native smoke script
compiles the standalone Hydrogen workspace test and both desktop frontends.
CI additionally bootstraps the latest Hydrogen Development Stage 1 and runs
both GTK4 applications under Xvfb with `HYUI_NATIVE_UI_SMOKE=1`, verifying
window shutdown, event dispatch and live GTK widget-state transitions.
The Apple Silicon job builds both AppKit bundles with Hydrogen's C backend,
checks their arm64 host/backend binaries, launches both windows, and runs
console and AppKit applications through Quark's target-aware Build/Run service.
On Linux you can run the same integration suite using
`HYUI_NATIVE_UI_SMOKE=1 HY=/path/to/hydrogen-stage1 dbus-run-session -- xvfb-run -a bash tests/native-smoke.sh`.
Full Start-to-IDE handoff, file pickers, macOS AppKit/Windows WinUI, accessibility
and visual acceptance still require manual or platform-native UI automation.

### Development progress / release gates

🟢 Separate native Start and IDE documents, independently compiled Hydrogen
applications and an explicit close-after-launch handoff.

🟢 Visual Studio-style three-column workspace, project/file actions, compiler
integration, project generation, recent paths and source contract CI.

🟡 Verify complete Start-to-IDE handoff on macOS and Windows; complete
portability and accessibility checks.

🔴 Rich editor APIs, asynchronous process execution, robust secure session
storage, project trees, Git integration, live UI designer, debugger, test
explorer and release-quality file recovery.

Development work should be tracked in [Hydrogen — Development](https://github.com/orgs/Aurora-softwares/projects/4).
