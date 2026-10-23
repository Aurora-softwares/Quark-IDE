# Quark IDE — plugin, explorer and Hydrogen build update

This package is based on the **attached v0.3.0 UI update**, with the Rust 1.99/egui 0.31 fixes from the follow-up messages included. This is not a remote GitHub checkout or a merge of subsequent local changes.

## What changed

- **Build project by default:** Click **▶ Build**, choose **Hydrogen → Build active project**, or press `Ctrl+Shift+B`. This runs the configured compiler as `build <selected.hyproj> -o <output>`, using the selected project regardless of the active source tab.
- **Compile just the current file:** **Shift+click ▶ Build** or choose **Hydrogen → Compile current .hy file**. This uses `compile <active.hy> -o <output>`. Standalone compilation may fail if the file depends on other sources; that is expected. If the active tab is a manifest, Quark tells you to open a `.hy` file.
- **Safe output location:** Quark creates `<manifest-directory>/.quark/build/<manifest-stem>`; for projects with `type = "os"`, this is **created as a directory**. Standalone sources use `<source-directory>/.quark/build/<source-stem>`. The self-hosted compiler's mandatory `-o` argument is always supplied.
- **Project picker:** choose the active `.hyproj` above the explorer. Opening a `.hyproj` registers it as a project. The choice persists in Quark settings.
- **Cleaner explorer:** Files / Project Outline tabs, active-project selector, refresh, Expand All and Collapse All buttons. Physical directories and logical namespace branches can be toggled independently. The panel remains resizable.
- **Plugins v1:** local JSON manifests defining named external commands with argument arrays (no shell by default), loaded at startup or by Reload. Enable/disable plugins in Plugins → Manage plugins; run their commands from the Plugins menu. Plugins are only launched by a deliberate menu click. No in-process native library loading or arbitrary embedded scripts.
- **Readable process exit codes:** command output now shows `[Exit code: 0]` or a nonzero code, rather than internal Unix `ExitStatus` debug formatting; failed commands update the status bar.
- **Compiler/Clippy compatibility fixes included:** `Stroke::new(1.0_f32, ...)`, `ccursor.index`, borrowed cursor for `pos_from_cursor`, and `map_while(Result::ok)` for command output streams.

## Compiler setup and Australis

In **Hydrogen → Compiler settings**, set the compiler path, e.g.:

`/home/rsmith/Projects/Aurora-Softwares/Hylang-Compiler/build/self_hosting/hydrogen-stage1`

Open the `.hyproj` using **File → Open .hyproj**, and select the correct project in Explorer. For `Australis-OS/src/australlis.hyproj`, the generated command has this form:

```sh
/path/to/hydrogen-stage1 build /path/to/Australis-OS/src/australlis.hyproj -o /path/to/Australis-OS/src/.quark/build/australlis
```

The final argument is a **directory** for an OS project. This directly addresses the missing `-o` compiler usage error in the report. Other build/semantic errors, if any, need separate troubleshooting.

Because the output is in `.quark/build`, add `.quark/` to your project `.gitignore` if desired. Quark does not edit the project's `.gitignore` for you.

## Native plugin v1 format

Install each trusted plugin into a subdirectory containing `plugin.json`:

- Linux: `${XDG_CONFIG_HOME:-~/.config}/QuarkIDE/plugins/<plugin-id>/plugin.json`
- macOS: `~/Library/Application Support/QuarkIDE/plugins/<plugin-id>/plugin.json`
- Windows: `%APPDATA%\\QuarkIDE\\plugins\\<plugin-id>\\plugin.json`

Use Plugins → Manage plugins to see and copy the actual directory path, create the parent folder, and enable/disable installed plugins. Copy the **example** from `examples/plugins/workspace-tools/` into that plugins directory, then choose Plugins → Reload manifests. The example is not installed or run automatically.

A manifest has this schema:

```json
{
  "api_version": 1,
  "id": "my-tool",
  "name": "My Tool",
  "description": "Command-line tool integration",
  "commands": [
    {
      "title": "Run my tool",
      "program": "./my-tool",
      "args": ["--project", "{project}", "--path", "{workspace}"],
      "needs_project": true
    }
  ]
}
```

Tokens supported in `program` and each argument: `{project}`, `{file}`, `{workspace}`, `{plugin_dir}`. File/project tokens require an applicable file or project. A program starting with `./` resolves relative to the plugin's installed directory; a plain name is searched in `PATH`. Arguments are passed separately to the process without shell parsing, preserving spaces in filenames.

**Security:** commands have ordinary user permissions. Plugins are **not sandboxed**; do not install unknown manifests or executables. There are no plugin-supplied UI panels, editor APIs, lifetime hooks, or hot reloading of an active command in this v1 protocol. Disabling prevents future launches; it cannot terminate a currently running command.

## Validate on your machine

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
cargo run --release
```

Then check `scripts/manual-acceptance.md` for the new UX acceptance cases. **This environment lacks cargo/rustc, so the build and interactive GUI have not been executed here.** The included Rust tests exercise build command planning and plugin arguments, but still need running by you.

## Applying without losing local changes

Back up or commit your local work first. The full ZIP is a reference source snapshot. The `.patch` is made against the **previous UI source with the eight compile fixes and the two command-stream Clippy fixes applied**. If your local files have diverged further, use `git apply --check` before applying the patch and inspect conflicts, rather than replacing your whole checkout.
