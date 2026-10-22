# Quark IDE release acceptance checklist

Automated unit tests cannot prove an interactive GUI and OS shell work end-to-end. Run this checklist **on each target platform** before tagging a release.

- [ ] App starts, renders and closes; settings persist after restart
- [ ] New file, save, reopen, edit, save all, multiple tabs, unsaved-close protection
- [ ] Open a .hyproj, recursive sibling and parent project references, warnings for missing files
- [ ] Open multiple folders; explorer navigates deeply; quick-open opens chosen file
- [ ] Long .hy line: word wrap OFF, horizontal wheel/shift-scroll or scrollbar works
- [ ] Long .hy line: word wrap ON, horizontal overflow eliminated and vertical scrolling works
- [ ] Big source file (10,000+ lines): scroll up/down, edit and save without data loss
- [ ] Hydrogen highlighting: keywords, UTF-8, strings, comments and .hyproj text
- [ ] Local completion popup lists matching tokens and appends selection (not cursor-aware)
- [ ] Terminal command prints stdout/stderr, exits, handles missing executable
- [ ] Build/check with installed Hydrogen SDK; compile with self-hosted compiler
- [ ] Git status/diff/fetch/pull/push against disposable test repository; clone to new folder
- [ ] Shortcuts Ctrl/Cmd+N/O/S/F/P/W, Ctrl/Cmd+Space, Alt+Z, F5, Ctrl+Shift+B
- [ ] Verify fonts, DPI scaling, dragging/resizing, native file dialogs

Release blocker: any checklist item that fails or any CI matrix failure.
