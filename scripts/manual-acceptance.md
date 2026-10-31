# Quark IDE 0.6.0 acceptance checklist

Run on each target platform before publishing binaries. Unit tests and a successful build do not replace GUI, compiler and authenticated Git checks. Use temporary files and disposable repositories. Record OS, toolchain, source revision and outcomes.

## Welcome and familiar navigation

- [ ] Fresh start: welcome layout, Explorer visible, bottom panel hidden. Menus and command discovery share a single top row. Both right-aligned layout buttons toggle the intended section and remember state.
- [ ] At narrow/wide sizes, the sidebar owns the lower-left corner and the bottom panel begins to its right. Hiding/restoring the sidebar resizes both editor and terminal grid. Check DPI scaling.
- [ ] Integrated minimise, maximise/restore and close work through the OS; close prompts for dirty buffers and active terminal/tool sessions. Drag QUARK, double-click it, and resize all eight edges/corners. Repeat with system title bar enabled in Settings, on X11/Wayland/macOS/Windows.
- [ ] At 800×500, all top-bar controls remain visible and the command button truncates instead of wrapping.
- [ ] Slim sidebar icon row renders without missing glyphs; Explorer, Search, Source Control, Extensions, Themes and Settings each open their intended controls.
- [ ] F1 and Ctrl/Cmd+Shift+P open the Command Palette; typing filters, Up/Down moves, Enter runs and Escape closes. Mouse scrolling works independently.
- [ ] Ctrl/Cmd+P focuses Go to File; Up/Down/Enter work across more results than fit on screen; Open File fallback works with unknown extensions.
- [ ] Ctrl/Cmd+B toggles Explorer, Ctrl/Cmd+Shift+E shows it, Ctrl+` toggles the bottom panel; persisted visibility survives restart. F1 opens the palette outside the focused terminal.
- [ ] Tabs scroll horizontally, paths are available on hover and under tabs, identically named files can be distinguished, untitled tabs have distinct names. New/opened tabs accept typing immediately.
- [ ] Help > Welcome preserves open tabs and allows returning to them.

## Welcome personalisation and catalogues

- [ ] Remove a recent project and a recent folder. Both remain open in the workspace, their files remain intact, and the entries stay removed after restart. Reopening each puts it back in recent history once.
- [ ] Hide each Get comfortable card independently, restart, and restore through Welcome and Settings. Both-hidden state retains a restore control.
- [ ] Theme and extension searches match names case-insensitively, show empty results, and preserve active theme/enabled state. Long names and descriptions do not push the options button outside the sidebar.
- [ ] Theme and extension lists have no inline editing or actions; the item options menu contains the requested nine controls. Theme Settings and extension Settings open from that menu.
- [ ] Enable/disable globally and per workspace. Switch workspaces, selected build targets and terminal-directory overrides. Scope follows the workspace; no-workspace options explain why they are disabled.
- [ ] Add shows that the repository is not configured. Import a valid local theme and reject invalid JSON without changing the current theme. Auto update preferences survive restart and cause no network request.
- [ ] Theme Settings edits colours, icons, syntax and terminal palette live; reset restores originals and leaves bundled assets intact. Export/import JSON round-trips overrides.
- [ ] Download ZIP for a theme and extension; inspect contents. Cancel a save and try a failing destination. Existing files are retained on export failure; symlink/size limits report failures.
- [ ] Open Containing Folder launches the platform file manager for a theme snapshot and installed extension; test paths with spaces and a missing folder.
- [ ] Remove a local theme, try Remove on a bundled theme, remove/cancel an extension removal, and verify recoverable extension backups. Running tool jobs prevent extension removal.
- [ ] Extension Settings exposes actions and manifest editing; saved edits apply after Reload. A disabled extension cannot run from either its settings or the top menu.

## File safety and recovery

- [ ] New, Save, Save As, Save All, reopen and multiple tabs work. Verify Ctrl/Cmd+Shift+S is Save As, Ctrl/Cmd+Alt+S is Save All.
- [ ] Dirty tab close: Cancel preserves text; Discard closes without writing; Save writes and closes; cancelling the save dialog preserves the tab.
- [ ] Dirty window close: Cancel keeps the app open; Save All writes every file before exit; Discard exits without writing changes.
- [ ] While a command runs, closing prompts before stopping it; normal exit terminates the job and its ordinary child processes.
- [ ] Change a saved file externally, then save edits in Quark: conflict dialog appears; Cancel preserves both versions; Save As preserves the external version; explicit overwrite writes edits; Reload discards edits and reads disk. Repeat after external deletion.
- [ ] Attempt saving to a read-only file/directory: useful visible failure; original content remains intact. Check symlink preservation and permissions.
- [ ] Choose a Save As destination already open in another tab: Quark refuses to create two conflicting buffers.
- [ ] Make unsaved edits and wait over two seconds. Force-terminate this test instance, restart, and verify buffers and original disk baselines return. Verify an external edit during downtime still triggers a conflict.
- [ ] In an isolated config, corrupt settings.json/session.json: original preserved, warning shown, persistence paused for that file. Back up/reset creates a backup before resuming. Test unwritable configuration and recovery directories.

## Workspace and large inputs

- [ ] Open multiple folders/projects, switch selected project, remove folders/projects via context menu and Settings; no on-disk files are deleted. Same-name project choices show paths.
- [ ] Explorer directory with 350+ files: Show More reaches every indexed file. Unknown extensions appear in Go to File.
- [ ] Lower index cap to force truncation: notices explain it; increase the cap and rescan to reach remaining files.
- [ ] Generated folders hidden by default; include-generated setting makes them searchable. .git remains excluded. Unreadable folders and depth/repeated-directory limits show notices.
- [ ] Edit and save IDs in the main and referenced `.hyproj` manifests, including opening through Files, Symbols and Go to File. Workspace roots and selected build target stay unchanged; Symbols refresh uses the saved manifest. Open Project still adds explicitly and only once.
- [ ] Editing stays responsive while a large project/index scan runs. Refresh after external additions/deletions updates Files and Symbols.
- [ ] Expand/Collapse All works in both Files and Symbols. Referenced projects, namespaces and types render; malformed/missing files show warnings.
- [ ] Long line with wrap off: both horizontal edges accessible, text and gutter stay aligned. Wrap on: logical line numbers align with wrapped rows. Hide/show panels and repeat.
- [ ] 10,000+ line source: scroll, edit, completion, find, save and restart recovery without data loss; record latency. Very large buffers remain a release qualification concern.

## Real OS terminal

- [ ] Opening Terminal starts the configured OS shell on a PTY/ConPTY. `test -t 0` on Unix succeeds; `cd` and environment changes persist across commands.
- [ ] Interactive `read`, a REPL and password entry receive input correctly. Ctrl+C interrupts a sleeping process and returns to the prompt. Ctrl+D exits a Unix shell; normal Windows shell exit works.
- [ ] Verify history arrows, Home/End, Tab completion, control keys and function keys. Application cursor mode works in a curses-style full-screen program.
- [ ] Resize while a terminal application runs: OS `stty size` / application size matches the rendered grid; alternate-screen exit restores the shell screen.
- [ ] ANSI 16/256/truecolour output, Unicode, wide/combining characters, cursor positioning, clear screen and scrollback render sensibly. Check current font glyph availability.
- [ ] Drag selects and platform copy/paste work; Ctrl+C with no selection interrupts. Test macOS Cmd+C separately. Verify bracketed multiline paste and Shift-selection when a terminal program captures the mouse.
- [ ] Test xterm SGR mouse press/release and wheel events in an application that requests mouse tracking. Shift+PageUp/PageDown and wheel scrollback work when mouse capture is off.
- [ ] Hide Terminal, switch tabs, run a tool job and return: the session remains alive. New session / End session and normal app exit terminate ordinary foreground jobs. Detached jobs and Windows process-tree handling require native verification.
- [ ] A missing/invalid shell or directory reports a useful failure and New session retries. Output remains usable after terminal failure.

## Themes

- [ ] Graphite, Aurora, Dawn and High Contrast each apply to the entire UI, icons, status bar, editor syntax and the live terminal. No remaining hardcoded dark foreground/background makes the light theme unreadable.
- [ ] Changing syntax colours updates an already-open unchanged document immediately; switching themes does not retain cached colours.
- [ ] In theme Settings: Outline/Rounded/Solid icons, icon colour and scale controls change the sidebar and remain legible. Interface, syntax and terminal palette controls apply live.
- [ ] Custom changes, selected preset, sidebar view and bottom tab persist across restart. Opening old 0.4.0 settings defaults the new theme fields correctly.
- [ ] Export a custom theme and re-import it: colours and icon settings round-trip. Invalid/incomplete/unsupported-version imports leave the current theme intact and report the failure.

## Editor and diagnostics

- [ ] Hydrogen highlighting covers keywords/types/modifiers/literals/numbers, UTF-8, strings, escapes and comments; .hyproj stays plain.
- [ ] Suggestions replace only the prefix at the caret; mouse/Up/Down/Tab/Enter work; Escape dismisses and typing can continue. Test Unicode, middle-of-file and popup near viewport edges.
- [ ] No code suggestions inside comments/strings; automatic completion can be disabled and manual Ctrl/Cmd+Space still works in code.
- [ ] Ctrl/Cmd+F focuses the Search sidebar; Enter selects a match without inserting a newline or modifying the document. F3/Shift+F3 wrap. Ctrl/Cmd+Shift+F searches indexed files and opens result lines. Replace One/All, Ctrl/Cmd+G, Ctrl/Cmd+Tab and Ctrl/Cmd+Shift+D work with Unicode and CRLF. Verify undo/redo after edits.
- [ ] Gutter tooltips/underlines identify lexical errors in selected-project, unrelated and untitled .hy buffers. Incomplete project graphs suppress unused hints.
- [ ] Quick-check status identifies scanning/partial coverage; compiler errors remain in Output; possible-unused hints are not presented as a full compiler/type check.

## Tool Output, compiler and Git

- [ ] Output streams stdout/stderr, including a prompt without a newline. Missing executable, nonzero exit, timeout and Stop yield a clear result and release the command slot.
- [ ] Run a noisy command exceeding 256 KiB: UI stays responsive and scrollback remains bounded. A printed '[Exit code: 0]' line cannot change the real job result.
- [ ] Stop a shell that spawned a sleeping child; verify the child exits. Repeat on native Windows. Run again immediately afterwards.
- [ ] Switch tabs during a running command: its displayed directory stays fixed. Shell/Git/plugin directory follows explicit folder or selected project/first workspace, never the active source tab.
- [ ] Output has no command input field. Build/Compile/Check/Run/Git/plugin actions and validation failures select Output. Typing there cannot execute commands or alter the log.
- [ ] Run a tool job while a terminal application is active; switching to Output and back preserves the shell/application.
- [ ] Compiler Settings: detect valid executable and review capabilities; handle invalid executable/help failure; changing executable clears old capabilities; unavailable operations guide to Settings.
- [ ] With actual Hydrogen tools: Build targets selected .hyproj, Shift+Build compiles active .hy, both pass -o; check/run use target directory. Verify successful artifacts and failures. Single-file action rejects a manifest.
- [ ] Git Status/Diff/Fetch/Pull/Push with a disposable remote; validate authentication failure without a hidden prompt and explicit directory selection.
- [ ] Clone into chosen empty destination: successful repo root added; invalid URL/nonempty destination/missing git does not add a failed workspace. Confirm later Git actions target the cloned repository.
- [ ] Example plugin loads/reloads, enable/disable works, required context errors are clear, stdout streams and Stop works.

Release gates: all applicable checks above, formatting, clippy with warnings denied, tests and release compilation on Linux/macOS/Windows. Record unavailable tests as outstanding, not passed.
