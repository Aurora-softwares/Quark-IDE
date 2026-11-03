#!/usr/bin/env bash
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
hy="${HY:-hy}"
mkdir -p build
"$hy" build Quark.IDE.hyproj -o build/QuarkIDE
"$hy" build Quark.hyproj -o build/Quark
echo
echo "Built both native Hydrogen windows."
echo "Run from this directory: ./build/Quark"
