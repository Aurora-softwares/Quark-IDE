# Quark IDE 0.5.0 — sidebar, OS terminal, themes and window controls

Development update dated 2026-10-29. Current progress, next target and release gates are recorded in [Hydrogen — Development](https://github.com/orgs/Aurora-softwares/projects/4). This source update is not a published/qualified binary release.

## Completed

- Search and Plugins are independent sidebar views. Search offers active-file Find/Replace plus a bounded background workspace search with line navigation. Plugins expose install-directory information, reload, enable/disable and action buttons directly in their sidebar.
- Sidebar categories moved from a large vertical activity column into a compact icon row inside the left section. Explorer, Search, Source Control, Plugins and Themes occupy that row, with Settings alongside.
- Menus and command discovery now share one top row. Two right-aligned vector buttons independently toggle the sidebar and bottom panel.
- Integrated minimise, maximise/restore and close controls occupy the far right. QUARK is a draggable caption with double-click maximise/restore; eight edge/corner targets hand resizing to the OS. Settings can restore the system title bar. Close shares the existing unsaved-work/session guard. The header remains one row at the 800×500 minimum window size.
- Opening and saving manifests no longer implicitly adds them as workspace roots or changes the build target. Main/referenced manifest ID edits preserve workspace membership; explicit Open Project still adds projects once. Existing unwanted roots can be removed in Settings.
- Dock ordering gives the sidebar the full left edge down to the status bar; the bottom panel occupies only the remaining editor column. Both panels remain resizable and their visibility is persisted.
- Terminal is a persistent real OS shell: Unix PTY or Windows ConPTY through portable-pty. The shell gets actual stdin/stdout/stderr terminal handles, size changes, control characters, application key sequences and paste. A VT screen renders ANSI/256/truecolour, cursor updates, alternate screens and bounded scrollback. Mouse reports are sent when an application requests them; Shift permits local selection.
- Output is the second bottom tab. It contains read-only stdout/stderr and exit results for tool actions, with Clear, Copy log and Stop job. Build/check/compile/run/Git/plugin launches automatically select it, as do tool validation failures. Tool jobs and the shell are independent and can run concurrently.
- Four complete version-1 JSON themes ship in assets/themes: Graphite, Aurora, Dawn and High Contrast. Themes cover interface surfaces/text/status/selection/errors, vector icon colour/style/scale, syntax colour groups and the terminal palette. Users can customise, export and import themes. Syntax-cache keys include theme colours, and changing themes recolours an already-open terminal immediately.
- Existing atomic saves, conflict decisions, dirty-close protection, document snapshots and recovery remain in place. Closing an active terminal warns before ending its session; shell state is intentionally not part of document recovery.

## Regression fixes

- egui-winit translates Ctrl+C into a clipboard Copy event before exposing the key. Terminal now maps Copy with no selection back to the interrupt control character on Linux/Windows. Ctrl+C and Cmd+C are handled separately on macOS. A real-PTY/headless-render regression exercises this bridge while a child sleeps.
- Enter in the Search sidebar now consumes its key event before transferring focus to a selected editor match. It cannot leak into TextEdit and replace that match with a newline. Both a headless regression and the real GUI/recovery snapshot confirm the document remains unchanged.
- The reported manifest-ID workflow exposed automatic project registration in both the file-open and save paths. Those paths now only edit files; tests preserve root membership and the selected build target across repeated root/dependency saves and folder-only manifest edits.

## Verification

- 82 tests pass locally, including genuine PTY detection, persistent directory/environment, interactive read, terminal resize, interrupt handling, ANSI/alternate-screen state, mouse encoding, exact shortcut handling, read-only Output routing, panel geometry, search focus safety, theme preference round-trips, live syntax-cache invalidation, native window command dispatch/dirty-close protection, edge resize targets and manifest ID editing without workspace registration.
- Linux GUI smoke checks use an isolated X11 display and a disposable configuration. Verified one-row header/toggles, all four themes, live terminal recolouring, a Python curses application with working arrow keys, shell input and Ctrl+C, sidebar plugin actions routing to Output while the interactive application remains alive, and dedicated Search/Plugins/Themes views. An isolated Openbox window manager confirms actual maximise/restore geometry, iconic/minimised state, dragging, double-click maximise and edge resizing. The integrated header also fits at 800×500. A final optimized-release GUI check opened a referenced manifest through Go to File, changed its ID and saved; the disk content changed while workspace roots and the selected target stayed unchanged.
- Formatting and clippy with warnings denied pass. The final ./scripts/release-check.sh passes: formatting check, clippy with warnings denied, all 82 tests and the optimized Linux release build.

## Next target and remaining release gates

- Qualify real terminals on native Windows (ConPTY/cmd/PowerShell workflows) and macOS (default shell, shortcuts, copy/paste, process termination and resize).
- Complete normal Linux X11/Wayland desktop acceptance and native window-control checks on Windows/macOS, native dialogs, high DPI, clipboard/IME and mouse-reporting tests. The isolated GUI smoke checks do not establish all platform integration behavior.
- Verify actual supported Hydrogen compiler versions and authenticated Git remotes; tool-stub/unit checks do not establish compiler or network compatibility.
- Confirm final-commit CI on Linux/macOS/Windows and representative large-file/workspace performance before publishing binaries.

The terminal is an embedded VT implementation rather than exhaustive xterm parity. Its grid is capped at 120×400, scrollback at 2,000 rows and background input/output queues are bounded. Resize does not fully reflow old lines. Font coverage depends on the bundled egui fonts. Ordinary foreground jobs are terminated on session close; deliberately detached processes can outlive their parent. Workspace search is case-sensitive and reports its file-size/result limits. These limits are documented in README and the updated acceptance checklist.
