# Quark IDE 0.4.0 acceptance checklist

Run on each target platform before publishing binaries. Unit tests and a successful build do not replace GUI, compiler and authenticated Git checks. Use temporary files and disposable repositories. Record OS, toolchain, source revision and outcomes.

## Welcome and familiar navigation

- [ ] Fresh start: welcoming Start/Recent/Get comfortable/First build layout; Explorer visible, Output hidden. Check narrow and wide windows and DPI scaling.
- [ ] Vector activity icons render without missing glyphs; Explorer, Find, Source Control, Plugins and Settings each open their intended controls.
- [ ] F1 and Ctrl/Cmd+Shift+P open the Command Palette; typing filters, Up/Down moves, Enter runs and Escape closes. Mouse scrolling works independently.
- [ ] Ctrl/Cmd+P focuses Go to File; Up/Down/Enter work across more results than fit on screen; Open File fallback works with unknown extensions.
- [ ] Ctrl/Cmd+B toggles Explorer, Ctrl/Cmd+Shift+E shows it, Ctrl+` toggles Output; persisted visibility survives restart. F1 no longer toggles Output.
- [ ] Tabs scroll horizontally, paths are available on hover and under tabs, identically named files can be distinguished, untitled tabs have distinct names. New/opened tabs accept typing immediately.
- [ ] Help > Welcome preserves open tabs and allows returning to them.

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
- [ ] Editing stays responsive while a large project/index scan runs. Refresh after external additions/deletions updates Files and Symbols.
- [ ] Expand/Collapse All works in both Files and Symbols. Referenced projects, namespaces and types render; malformed/missing files show warnings.
- [ ] Long line with wrap off: both horizontal edges accessible, text and gutter stay aligned. Wrap on: logical line numbers align with wrapped rows. Hide/show panels and repeat.
- [ ] 10,000+ line source: scroll, edit, completion, find, save and restart recovery without data loss; record latency. Very large buffers remain a release qualification concern.

## Editor and diagnostics

- [ ] Hydrogen highlighting covers keywords/types/modifiers/literals/numbers, UTF-8, strings, escapes and comments; .hyproj stays plain.
- [ ] Suggestions replace only the prefix at the caret; mouse/Up/Down/Tab/Enter work; Escape dismisses and typing can continue. Test Unicode, middle-of-file and popup near viewport edges.
- [ ] No code suggestions inside comments/strings; automatic completion can be disabled and manual Ctrl/Cmd+Space still works in code.
- [ ] Ctrl/Cmd+F focuses Find; F3/Shift+F3 wrap. Replace One/All, Ctrl/Cmd+G, Ctrl/Cmd+Tab and Ctrl/Cmd+Shift+D work with Unicode and CRLF. Verify undo/redo after edits.
- [ ] Gutter tooltips/underlines identify lexical errors in selected-project, unrelated and untitled .hy buffers. Incomplete project graphs suppress unused hints.
- [ ] Quick-check status identifies scanning/partial coverage; compiler errors remain in Output; possible-unused hints are not presented as a full compiler/type check.

## Commands, compiler and Git

- [ ] Output streams stdout/stderr, including a prompt without a newline. Missing executable, nonzero exit, timeout and Stop yield a clear result and release the command slot.
- [ ] Run a noisy command exceeding 256 KiB: UI stays responsive and scrollback remains bounded. A printed '[Exit code: 0]' line cannot change the real job result.
- [ ] Stop a shell that spawned a sleeping child; verify the child exits. Repeat on native Windows. Run again immediately afterwards.
- [ ] Switch tabs during a running command: its displayed directory stays fixed. Shell/Git/plugin directory follows explicit folder or selected project/first workspace, never the active source tab.
- [ ] `cd` in one command does not affect the next; pane explains fresh-shell/noninteractive behavior. Interactive input uses an external terminal.
- [ ] Compiler Settings: detect valid executable and review capabilities; handle invalid executable/help failure; changing executable clears old capabilities; unavailable operations guide to Settings.
- [ ] With actual Hydrogen tools: Build targets selected .hyproj, Shift+Build compiles active .hy, both pass -o; check/run use target directory. Verify successful artifacts and failures. Single-file action rejects a manifest.
- [ ] Git Status/Diff/Fetch/Pull/Push with a disposable remote; validate authentication failure without a hidden prompt and explicit directory selection.
- [ ] Clone into chosen empty destination: successful repo root added; invalid URL/nonempty destination/missing git does not add a failed workspace. Confirm later Git actions target the cloned repository.
- [ ] Example plugin loads/reloads, enable/disable works, required context errors are clear, stdout streams and Stop works.

Release gates: all applicable checks above, formatting, clippy with warnings denied, tests and release compilation on Linux/macOS/Windows. Record unavailable tests as outstanding, not passed.
