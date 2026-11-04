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

# Optional native GTK4 integration exercises real desktop windows under Xvfb.
# This is not a substitute for manual visual and keyboard acceptance.
if [[ "${HYUI_NATIVE_UI_SMOKE:-0}" == "1" ]]; then
  [[ "$(uname -s)" == "Linux" ]] || { echo "GTK4 smoke requires Linux" >&2; exit 1; }
  command -v timeout >/dev/null
  command -v python3 >/dev/null
  export GDK_BACKEND=x11 GTK_A11Y=none G_DEBUG=fatal-warnings
  for binary in "$work/Quark" "$work/QuarkIDE"; do
    test -x "$binary"
    file "$binary" | grep -q 'ELF 64-bit'
    ldd "$binary" | grep -q 'libgtk-4'
    if ldd "$binary" | grep -q Qt6; then
      echo "Quark unexpectedly links Qt: $binary" >&2
      exit 1
    fi
    timeout 12s "$binary" --self-test-close >"$work/close.stdout" 2>"$work/close.stderr" || {
      echo "Native GTK4 close failed: $binary" >&2
      tail -n 60 "$work/close.stderr" >&2 || true
      exit 1
    }
  done

  # Events dispatch into the compiled Hydrogen backend and snapshot native
  # GTK widgets; contract-only checks cannot detect event/host mismatches.
  timeout 12s "$work/Quark" --event StartReady:quark --state-output "$work/start.json"
  timeout 12s "$work/QuarkIDE" --event ShowUI:toolbarUI --state-output "$work/ui.json"
  timeout 12s "$work/QuarkIDE" --event ShowCode:toolbarCode --state-output "$work/code.json"
  timeout 12s "$work/QuarkIDE" --event ShowOutput:bottomTabs --state-output "$work/output.json"

  python3 - "$work" <<'PY'
import json
from pathlib import Path
import sys
work = Path(sys.argv[1])
states = {name: json.loads((work / (name + '.json')).read_text())
          for name in ('start', 'ui', 'code', 'output')}
assert 'recentList.text' in states['start']
assert states['ui']['editorTabs.value'] == 1, states['ui']
assert states['code']['editorTabs.value'] == 0, states['code']
assert states['output']['bottomTabs.value'] == 0, states['output']
for name, state in states.items():
    assert state['__hyui.visible_popovers'] == '0', name
print('Native Quark Start and IDE lifecycle and event integration passed.')
PY
else
  echo "Native GTK4 interactions not run; set HYUI_NATIVE_UI_SMOKE=1 under Xvfb."
fi
