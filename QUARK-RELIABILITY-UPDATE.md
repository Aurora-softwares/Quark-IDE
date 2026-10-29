# Quark IDE 0.4.0 development update

Date: 2026-10-29. Current development tracking: [Hydrogen — Development](https://github.com/orgs/Aurora-softwares/projects/4). This is an implementation and qualification update, not a binary release declaration.

## Fifteen-point review follow-up

| # | Reviewed concern | Implementation |
| --- | --- | --- |
| 1 | Unsaved work on application exit | Window-close interception; Save All / Discard / Cancel; two-second session snapshots restore buffers and saved baselines. |
| 2 | External edits and interrupted saves | Atomic sibling-temp replacement; existing-file content checks; explicit conflict resolution and Save As; symlink and permission preservation. |
| 3 | Commands monopolise the execution slot | Structured job events, Stop, configurable timeout, process-tree cancellation, worker cleanup during normal shutdown. |
| 4 | Unexpected command directories | Stable selected-project / first-workspace / explicit command directory; actual running directory stays visible; compiler operations use target directory. |
| 5 | Filesystem work during rendering | Background workspace snapshots and project/symbol loading; cached Quick Open filtering and syntax highlighting. |
| 6 | Unbounded output | Bounded 128-message queue, 64-message per-frame budget, 256 KiB UTF-8-safe scrollback, visible-row rendering. |
| 7 | Missing diagnostics outside selected project | Every open Hydrogen buffer is scanned, including synthetic identities for untitled documents. Only project members contribute to unused-hint analysis. |
| 8 | Silent settings failures | Atomic persistence, persistent notifications, unreadable-file preservation and explicit backup/reset. |
| 9 | Misleading Terminal label | Output wording, fresh-shell / noninteractive explanation, stable directory, Stop and timeout controls. |
| 10 | No discard action for dirty tabs | Save / Discard / Cancel modal. |
| 11 | Compiler subcommand confusion | SDK / conservative single-file profiles, --help probe, editable capabilities, unsupported-action guidance. Changing executables clears capabilities. |
| 12 | Hidden navigation limits | All extensions indexed; directory paging, visible index notices, adjustable caps and generated-folder inclusion, native Open File fallback. |
| 13 | Completion disrupts typing | Automatic-completion preference, comment/string suppression, retained editor focus, explicit manual completion. |
| 14 | Multi-project ambiguity and cleanup | Full-path project choices and tab tooltips; path breadcrumbs; unique untitled labels; context-menu/settings workspace removal; explicit command-folder selection. |
| 15 | Contradictory docs | README and acceptance checklist rewritten for current behavior; historical update files remain historical. |

Shortcut handling now matches modifiers exactly: Save cannot consume Save As, Explorer cannot consume Build, and forward-tab navigation cannot consume reverse-tab navigation. Regression tests cover these conflicts and Go to File Escape handling.

The UI now includes a dark neutral editor surface, blue status/accent, native vector activity icons, a welcome page with Start / Recent / Get comfortable / First build, an accessible Command Palette, keyboard Go to File, scrollable tabs, and an Output panel. F1 now opens the palette; Ctrl+` toggles Output. Save As is Ctrl/Cmd+Shift+S; Save All is Ctrl/Cmd+Alt+S.

Clone also now targets the selected empty destination itself and registers it only on a successful exit. Command completion is a typed event, so a program printing text that resembles an exit-code marker cannot spoof the job state.

## Verification

- All 63 Rust/unit/headless egui tests pass locally. They cover storage conflicts/deletion/atomic replacement/symlinks, bounded Unicode output, partial stdout/stderr, missing executables, cancellation of shell descendants, timeouts, recovery baselines, invalid settings preservation, dirty tab/window close protection, command directories, compiler capabilities, all-extension indexing, generated-folder inclusion, and untitled/outside-project diagnostics.
- Linux GUI smoke checks run on an isolated X11 display with a separate temporary configuration. They include welcome/editor rendering, Command Palette interaction, unsaved-close/exit prompts, deliberate crash/restart recovery and command-output behavior. No personal workspace settings are used for these checks.
- The final ./scripts/release-check.sh passes: formatting check, clippy with warnings denied, all 63 tests and the optimized Linux release build. The CI formatting step checks rather than rewriting the checkout.

## Next target and remaining release gates

1. Run the updated acceptance checklist on native Windows and macOS, including file replacement, dialogs, shortcut differences, DPI and process-tree cancellation.
2. Complete the same checklist in a normal Linux desktop session, including Wayland, native file dialogs and interactive resize/DPI behavior.
3. Verify Build / Compile / Check / Run with the actual supported Hydrogen SDK and self-hosted toolchain versions. Stub/command-runner tests do not establish compiler compatibility.
4. Validate authenticated Git Fetch / Pull / Push / Clone against a disposable remote, including credential failures and conflict recovery.
5. Complete large-workspace and large-file acceptance tests on representative user hardware; background indexing and cached highlighting do not virtualise the entire editor buffer.
6. Confirm the three-OS CI matrix on the final reviewed commit before publishing binaries.

Recovery can lose the last two seconds of edits and depends on writable local storage. External-file checks are optimistic rather than cross-process locks. Output remains noninteractive. Compiler-help detection is advisory. A descendant that deliberately detaches from the command's process group can escape ordinary process-tree cancellation; Windows behavior still needs native validation. These limitations remain documented rather than represented as complete IDE parity.
