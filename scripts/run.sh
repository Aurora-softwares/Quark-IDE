#!/usr/bin/env bash
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
if [[ -x ./build/Quark.app/Contents/MacOS/hyui-launcher ]]; then
  exec ./build/Quark.app/Contents/MacOS/hyui-launcher
fi
exec ./build/Quark
