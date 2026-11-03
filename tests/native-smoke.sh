#!/usr/bin/env bash
set -euo pipefail
compiler="${HY:-hy}"
work="$(mktemp -d)"
trap 'rm -rf -- "$work"' EXIT
"$compiler" build Quark.WorkspaceTests.hyproj -o "$work/quark-workspace-tests"
"$work/quark-workspace-tests"
"$compiler" build Quark.hyproj -o "$work/Quark"
echo "Quark Hydrogen backend smoke and native UI packaging completed."
echo "Manual launch and interaction still required on the current host."
