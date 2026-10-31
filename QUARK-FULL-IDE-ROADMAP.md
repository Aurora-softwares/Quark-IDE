# Quark — full Hydrogen IDE roadmap

Status: design plan for Quark Development, October 2026. This is a roadmap,
not a claim that the features below have shipped.

## Product direction

Quark should become a native, cross-platform, solution-oriented Hydrogen
development environment closer to Visual Studio than a general-purpose editor.
Keep the existing Rust/egui application and reuse its safe documents,
terminal, workspace indexing, and Hydrogen compiler command runner.

## Increment 1 — reliable IDE diagnostics (in this PR)

- Separate compiler Problems from read-only Output and heuristic editor hints.
- Recognise explicit file/line error/warning output from the active tool job.
- Allow opening the source location from the bottom panel.
- Keep unrecognised output visible without pretending it has a location.
- Add parser regression tests, including warnings, positions and Windows paths.

Release gate: Rust formatting, Clippy, unit tests, and native release build
on Linux, macOS and Windows. Validate against actual Hydrogen output before
declaring the parser complete.

## Increment 2 — build and launch as first-class concepts

- Introduce BuildService, LaunchService and DebugService boundaries, separate
  from the editor widget state.
- Respect a typed target/platform and executable artifact description exposed
  by the Hydrogen compiler. Do not guess an output extension from the host OS.
- Track task state and results, with separate Build, Run, Test and Debug actions.
- Add editable launch profiles: project, target, artifact, working directory,
  argv and environment, persisted safely per workspace.
- Make Build, Rebuild, Clean, Run Without Debugging, and Stop explicit.
- Do not launch the program if building failed; use the artifact path the
  compiler actually generated. Preserve a readable log of every step.

Release gate: actual Linux and Windows executable fixtures; test macOS and
Australis formats against their available runners or emulators.

## Increment 3 — solutions and project authoring

- Create a solution model grouping several .hyproj projects without changing
  their existing compiler format.
- Startup project, project dependencies, build order and per-configuration
  target/platform selection.
- Project templates, add/remove/rename/reference actions, manifest validation,
  project property editor and deliberate generated-output cleanup.
- Avoid implicit workspace changes when opening a manifest for text editing.

Release gate: dependency cycles, cross-platform path handling, missing files,
and multi-project incremental build tests.

## Increment 4 — real debugging

- First design a Hydrogen-to-debugger contract: address/line mappings,
  symbol names, function boundaries, stack frames, source paths and variable
  location information. Validate generated binaries before promising locals.
- On Linux, integrate a debugger backend through a well-defined adapter.
  Evaluate GDB/MI or a supported debug-adapter protocol; use LLDB as appropriate
  on macOS. Windows needs its own verified native debugger path.
- Implement session lifecycle, pause/resume, stop, step in/over/out, breakpoints,
  call stack, threads, scopes, watches and variable inspection.
- Store breakpoints by source path/line, distinguish pending vs verified, and
  handle source-map mismatches and stale artifacts.
- Never describe Run as Debug until a debugger has actually attached and
  breakpoints can be honoured.

Release gate: real native debug sessions, including controlled crashes and
optimized-vs-debug artifact handling; platform-specific CI or manual acceptance.

## Increment 5 — quality-of-life Visual Studio features

- Test Explorer with discovery/execution/result history.
- Clickable error locations and structured diagnostic IDs, quick fixes and
  semantic language-server features if/when Hydrogen exposes them.
- Git staging/committing, launch configurations, solution search and profiling.
- Optional Hydrogen.UI designer, property inspector and live preview only
  after a stable UI metadata/serialization and runtime preview contract.

## Important Hydrogen Compiler dependencies

Quark's current build command planner still contains legacy `native` and
`exe` names. Audit and align it with the current compiler's explicit
Linux, Windows, macOS and Australis targets before making build/run promises.
The current CLI Run verb is not equivalent to a debugger, and source-level
debugging requires usable mapping/symbol data from Hydrogen.
