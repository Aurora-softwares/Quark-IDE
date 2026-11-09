#!/usr/bin/env bash
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
compiler="${HY:-hy}"
work="$(mktemp -d)"
trap 'rm -rf -- "$work"' EXIT
"$compiler" build Quark.WorkspaceTests.hyproj -o "$work/quark-workspace-tests"
"$work/quark-workspace-tests"
mkdir -p build
"$compiler" build Quark.SessionTests.hyproj -o "$work/quark-session-tests"
"$work/quark-session-tests"
"$compiler" build Quark.BuildTargetTests.hyproj -o "$work/quark-target-tests"
"$work/quark-target-tests" "$work"
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
  timeout 12s "$work/QuarkIDE" --event TargetChanged:buildTarget --state-output "$work/target.json"

  # A working editor window is not enough: real user selections trigger
  # LoadSession on the second window, which was previously untested.
  # The IDE must read the handoff, open the project and consume the request.
  mkdir -p build
  printf 'P\n%s\n' "$root/Quark.IDE.hyproj" > build/.quark-open-request
  timeout 12s "$work/QuarkIDE" --state-output "$work/project-loaded.json" \
    > "$work/project-load.stdout" 2> "$work/project-load.stderr" || {
      cat "$work/project-load.stderr" >&2
      echo "Quark IDE failed to load the project handoff" >&2
      exit 1
    }
  test ! -s build/.quark-open-request || {
    echo "Quark IDE did not consume its launch request" >&2
    echo "---- GTK stderr ----" >&2
    cat "$work/project-load.stderr" >&2
    echo "---- backend stdout ----" >&2
    cat "$work/project-load.stdout" >&2
    echo "---- UI state (selected fields) ----" >&2
    python3 - "$work/project-loaded.json" <<'PYDIAG'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
if path.exists():
    state = json.loads(path.read_text())
    for key in ('projectTitle.text', 'status.text', 'codeName.text', 'projectState.text'):
        print(f"{key}: {state.get(key, '<missing>')}", file=sys.stderr)
else:
    print("No HYUI state was saved", file=sys.stderr)
PYDIAG
    exit 1
  }
  printf 'S\n%s\n' "$root/src/Quark/Workspace.hy" > build/.quark-open-request
  timeout 12s "$work/QuarkIDE" --state-output "$work/source-loaded.json" \
    > "$work/source-load.stdout" 2> "$work/source-load.stderr" || {
      cat "$work/source-load.stderr" >&2
      echo "Quark IDE failed to load the source-file handoff" >&2
      exit 1
    }
  test ! -s build/.quark-open-request

  python3 - "$work" <<'PY'
import json
from pathlib import Path
import sys
work = Path(sys.argv[1])
states = {name: json.loads((work / (name + '.json')).read_text())
          for name in ('start', 'ui', 'code', 'output', 'target')}
assert 'recentList.text' in states['start']
assert states['ui']['editorTabs.value'] == 1, states['ui']
assert states['code']['editorTabs.value'] == 0, states['code']
assert states['output']['bottomTabs.value'] == 0, states['output']
assert states['target']['targetSummary.text'] == 'Target: host CPU', states['target']
for name, state in states.items():
    assert state['__hyui.visible_popovers'] == '0', name
project = json.loads((work / 'project-loaded.json').read_text())
source = json.loads((work / 'source-loaded.json').read_text())
assert project['projectTitle.text'] == 'QuarkIDE', project
assert project['codeName.text'] == 'Program.hy', project
assert source['codeName.text'] == 'Workspace.hy', source
assert 'public class Workspace' in source['codeText.text'], source
print('Native Quark Start, IDE event dispatch and actual launch handoffs passed.')
PY
else
  echo "Native GTK4 interactions not run; set HYUI_NATIVE_UI_SMOKE=1 under Xvfb."
fi
