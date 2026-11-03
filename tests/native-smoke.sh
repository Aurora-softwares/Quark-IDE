#!/usr/bin/env bash
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
compiler="${HY:-hy}"
work="$(mktemp -d)"
trap 'rm -rf -- "$work"' EXIT
"$compiler" build Quark.WorkspaceTests.hyproj -o "$work/quark-workspace-tests"
"$work/quark-workspace-tests"
"$compiler" build Quark.IDE.hyproj -o "$work/QuarkIDE"
"$compiler" build Quark.hyproj -o "$work/Quark"
test -f "$work/QuarkIDE" || test -d "$work/QuarkIDE.app" || test -f "$work/QuarkIDE.exe"
test -f "$work/Quark" || test -d "$work/Quark.app" || test -f "$work/Quark.exe"
echo "Hydrogen workspace smoke and two separate native UI builds passed."
echo "The on-screen launcher/IDE transition still requires interactive testing."
