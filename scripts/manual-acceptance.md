# Quark IDE release acceptance checklist

Automated unit tests cannot prove an interactive GUI and OS shell work end-to-end. Run this checklist **on each target platform** before tagging a release.

- [ ] App starts, renders and closes; settings persist after restart
- [ ] New file, save, reopen, edit, save all, multiple tabs, unsaved-close protection
- [ ] Open a .hyproj: hierarchy renders Project > Namespace segments > Class (filename.hy); namespace files without classes appear as filename.hy, references remain expandable, missing files show warnings
- [ ] Open multiple folders; explorer navigates deeply; quick-open opens chosen file
- [ ] Long .hy line: word wrap OFF, scroll to start/end; no text disappears under Explorer; horizontal scrollbar ends at actual text edge
- [ ] Long .hy line: word wrap ON, horizontal overflow eliminated and vertical scrolling works
- [ ] Big source file (10,000+ lines): scroll up/down, edit and save without data loss
- [ ] Hydrogen highlighting: declarations, modifiers, built-in types, numbers (including hex), literals, UTF-8, strings, escaped quotes, comments; .hyproj text stays plain
- [ ] Typing `st` in a `.hy` file shows static/string/struct by the caret; Tab/Enter and clicking a suggestion replace the typed prefix at the caret (not the end of the file)
- [ ] Terminal command prints stdout/stderr, exits, handles missing executable
- [ ] Build/check with installed Hydrogen SDK; compile with self-hosted compiler
- [ ] Git status/diff/fetch/pull/push against disposable test repository; clone to new folder
- [ ] Shortcuts Ctrl/Cmd+N/O/S/F/P/W, Ctrl/Cmd+Space, Alt+Z, F5, Ctrl+Shift+B
- [ ] Completion popup flips above caret near bottom of screen, never runs behind sidebar, Arrow Up/Down navigate and Esc dismisses
- [ ] Verify graphite/teal theme, selected tabs, fonts, DPI scaling, dragging/resizing, native file dialogs

Release blocker: any checklist item that fails or any CI matrix failure.

## Project-build, explorer and plugins (next UI update)

- 🔴 Open `Australis-OS/src/australlis.hyproj`, then a source `.hy` file. Click **Build**: verify command begins with `build` and the `.hyproj`, includes `-o` and writes to `.quark/build/australlis` directory.
- 🔴 Hold Shift and click **Build** with a `.hy` active: verify command begins with `compile` and the open `.hy`, includes `-o`, and does not build the entire project.
- 🔴 With `.hyproj` selected, Shift+Build reports that a `.hy` file must be open.
- 🔴 Add two different `.hyproj` projects and change active project from the Explorer selector; Build should follow the selected project, not the active editor tab.
- 🔴 Expand, collapse and re-expand nested workspace folders; check the Files and Project Outline tabs independently.
- 🔴 Run Expand All, then Collapse All; verify both views' nested sections respond.
- 🔴 Copy the example plugin to the Plugins directory, click Reload, run its 'Recent commits' command and inspect streamed output. Disable it, verify commands disappear; re-enable and retest.
- 🔴 Make the compiler return exit 1 and verify the terminal shows `[Exit code: 1]` and the status bar reports a failure.

## Compact workspace / editor quality-of-life validation

- 🔴 Ctrl+B toggles the entire Explorer without changing the editor cursor; closing and reopening Quark remembers the visibility state.
- 🔴 F1 toggles the Terminal; terminal close and View menu stay in sync and reopening Quark remembers its visibility state.
- 🔴 Click Build and Shift+Click Build: respectively build the selected `.hyproj` and compile only the active `.hy`. Both still include `-o`.
- 🔴 On a very long line with wrap OFF, hide both panels and scroll to both horizontal edges; editor remains left-aligned.
- 🔴 Sidebar Files/Symbols remains independently collapsible, selected file highlighted, compact expand/collapse icons work.
- 🔴 Ctrl+F opens inline Find and focuses the text field. F3/Shift+F3 navigate matching text with wraparound.
- 🔴 Ctrl+H opens inline Replace. Replace One changes the selection, Replace All replaces every occurrence. Test with Unicode text.
- 🔴 Ctrl+G focuses Go to Line. Verify line 1, middle, last, and invalid lines.
- 🔴 Ctrl+Tab and Ctrl+Shift+Tab cycle tabs, including wraparound.
- 🔴 Ctrl+Shift+D duplicates the line below with cursor position preserved; test CRLF and Unicode.
- 🔴 Status bar displays UTF-8 and the caret's 1-based line/column.
- 🔴 Verify default and existing settings files preserve sidebar/terminal visibility correctly.
- 🔴 Keyboard completion, plugin commands, active-project builds and window resizing still function after panel changes.
