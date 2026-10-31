# Quark IDE 0.6.0 — coverage, welcome preferences and catalogues

Development update dated 2026-10-29. Current version, completed work, next target and release gates are tracked in [Hydrogen — Development](https://github.com/orgs/Aurora-softwares/projects/4). This is a development source update, not a platform-qualified binary release.

## Completed

- Added a reviewed register of 185 features, each with at least two distinct, non-ignored tests. Local release checks and all three CI jobs enforce the minimum and verify that test names actually exist. Eight Python tests verify the gate. The native/headless/filesystem/PTY Rust suite now contains 349 tests.
- Added per-test persistence paths and shared file-operation handlers so tests exercise actual writes, failures, recovery, Save As, theme import/export and backups without changing the user's configuration or opening native choosers.
- Added independent recent history with remove buttons. Removing a recent item preserves workspace membership, selected build target, editor buffers and files. Old preferences migrate once; explicit empty history stays empty, and reopening restores an entry once at the front.
- Added dismiss buttons for the shortcut and first-build welcome cards, persistent visibility, a Restore hidden sections button and Settings checkboxes.
- Refreshed the sidebar's installed theme/extension lists, search fields, metadata rows and item options. Explorer and Source Control retain their current functions, with compact underline Explorer tabs and grouped Source Control actions. The extensions icon uses native vector blocks; options use font-safe ASCII dots.
- Theme and extension menus offer Enable, Enable (Workspace), Disable, Disable (Workspace), Auto update, Settings, Open Containing Folder, Download ZIP and Remove. Workspace identity uses the first folder (falling back to the first project directory), so changing build targets or terminal directories does not change scope.
- Theme lists are read-only. Editing opens from item Settings, applies a local override and preserves bundled assets. Reset, versioned JSON import/export, custom-theme removal and legacy custom-theme migration are supported. Bundled themes can be disabled; their Remove option is unavailable.
- Extension Settings exposes actions and manifest editing, followed by Reload. Availability is respected by the sidebar, top menu and runner. Removal requires confirmation and moves files into recoverable extension-backups; active jobs prevent removal.
- Added local ZIP export with bounded contents, symlink rejection and failure-safe destination handling. Theme folder opening materialises an inspectable JSON snapshot in the configuration area.
- Add/discovery and auto-update transports are explicit placeholders until repositories exist. Auto update stores a preference without making network requests. Local installation/import and management remain functional.
- Fixed manual completion disappearing on the next frame when automatic suggestions were off. Regression coverage now exercises manual persistence, Escape and acceptance. Resetting an inactive theme also no longer recreates the removed customisation entry.

## Verification

The complete local release check passes: formatting, clippy with warnings denied, all 349 Rust tests, all eight validator tests, the 185-feature coverage floor and the optimized Linux build.

An isolated X11/Openbox session running the optimized build verified theme search/options, opening Theme Settings, live name edits and persistence/reset; recent removal retaining the workspace; independent dismissal of both welcome cards and Restore hidden sections; extension options/Settings and a real action writing Git version output into the read-only Output tab. Explorer/Source Control presentation was inspected with existing actions preserved. Native dialogs, file-manager launching and other host-specific effects remain in the manual release gates.

Actual application screenshots: [themes and Welcome](assets/quark-catalogues.png), [theme options](assets/quark-theme-options.png), [extensions and action output](assets/quark-extensions.png).

## Next target and remaining release gates

- Complete native Windows ConPTY/macOS and Linux Wayland desktop acceptance, including dialogs, clipboard/IME, DPI, file managers and window controls.
- Run final reviewed-commit CI on Linux/macOS/Windows; local headless tests do not establish all native effects.
- Qualify actual supported Hydrogen compiler versions, artifacts and authenticated Git workflows. No remote Git operations are part of the automated tests.
- Check representative large workspaces/files and terminal retention under normal desktop use.
- Wire theme/extension repository discovery and automatic updates once the repositories and package/update contracts are supplied.

See [TESTING.md](TESTING.md), [the machine-checked feature register](tests/feature-coverage.json) and [manual acceptance](scripts/manual-acceptance.md). Earlier terminal/window work is documented in [the 0.5.0 report](QUARK-WORKSPACE-UPDATE.md).
