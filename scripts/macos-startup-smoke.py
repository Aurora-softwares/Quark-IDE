#!/usr/bin/env python3
"""Run native AppKit startup and real Start Window -> IDE transition checks.

Uses the production Start Window's deferred launch action, not an imitation of
the editor. This detects the macOS crash after clicking "Continue without code"
or opening an existing Hydrogen project. It cannot verify screenshot parity.
"""
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time


def check_startup(binary: Path, mode: str = "start", seconds: float = 10.0) -> None:
    if sys.platform != "darwin":
        raise RuntimeError("AppKit startup validation must run on macOS")
    if not binary.is_file():
        raise RuntimeError(f"Missing native Quark executable: {binary}")
    if mode not in ("start", "empty", "project"):
        raise RuntimeError(f"Unknown native workspace smoke mode: {mode}")
    with tempfile.TemporaryDirectory(prefix="quark-macos-smoke-") as work:
        root = Path(work)
        env = os.environ.copy()
        env["HOME"] = work
        env["XDG_CONFIG_HOME"] = work
        env.pop("QUARK_CI_MACOS_TRANSITION", None)
        if mode == "empty":
            env["QUARK_CI_MACOS_TRANSITION"] = "empty"
        elif mode == "project":
            project_dir = root / "My Hydrogen App"
            project_dir.mkdir()
            (project_dir / "Program.hy").write_text("public class Program {}\n")
            manifest = project_dir / "My Hydrogen App.hyproj"
            manifest.write_text(
                'format = 2\nname = "My Hydrogen App"\ntype = "exe"\nsources = ["Program.hy"]\n'
            )
            env["QUARK_CI_MACOS_TRANSITION"] = f"project:{manifest}"
        error_path = root / "quark-stderr.log"
        with error_path.open("wb") as errors:
            process = subprocess.Popen(
                [str(binary)],
                cwd=work,
                env=env,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=errors,
                start_new_session=True,
            )
            try:
                deadline = time.monotonic() + seconds
                ready = False
                while time.monotonic() < deadline:
                    if process.poll() is not None:
                        raise RuntimeError(
                            f"AppKit Quark crashed during {mode} startup "
                            f"(exit {process.returncode}):\n"
                            f"{error_path.read_text(errors='replace')[-7000:]}"
                        )
                    if mode != "start" and "Quark: AppKit IDE workspace ready" in error_path.read_text(errors="replace"):
                        ready = True
                    time.sleep(0.15)
                if mode != "start" and not ready:
                    raise RuntimeError(
                        f"AppKit Quark remained alive but failed to complete the {mode} "
                        f"workspace transition:\n{error_path.read_text(errors='replace')[-7000:]}"
                    )
                print(f"AppKit Quark {mode} startup passed: stayed alive {seconds:.1f}s.")
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGTERM)
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait(timeout=5)


if __name__ == "__main__":
    if len(sys.argv) not in (2, 3):
        raise SystemExit("Usage: macos-startup-smoke.py /path/to/quark-native [start|empty|project]")
    try:
        check_startup(Path(sys.argv[1]).resolve(), sys.argv[2] if len(sys.argv) == 3 else "start")
    except (OSError, RuntimeError) as error:
        print(f"FAILED: {error}", file=sys.stderr)
        raise SystemExit(1) from error
