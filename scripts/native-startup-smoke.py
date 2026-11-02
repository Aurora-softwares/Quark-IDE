#!/usr/bin/env python3
"""Confirm the native GTK window keeps its process alive under a virtual display.

This is a startup-lifetime smoke only: successful execution is NOT evidence
of interactive docking, terminal keyboard parity or rendered pixel correctness.
"""
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time


def check_startup(binary: Path, seconds: float = 5.0) -> None:
    if not binary.is_file():
        raise RuntimeError(f"Native executable does not exist: {binary}")
    with tempfile.TemporaryDirectory(prefix="quark-native-smoke-") as home:
        env = os.environ.copy()
        env["XDG_CONFIG_HOME"] = home
        env["GDK_BACKEND"] = "x11"
        env["GSETTINGS_BACKEND"] = "memory"
        env["LIBGL_ALWAYS_SOFTWARE"] = "1"
        # Treat GTK parenting warnings (including "Tried to remove non-child")
        # as a startup failure instead of silently printing thousands of lines.
        env["G_DEBUG"] = "fatal-warnings"
        process = subprocess.Popen(
            [str(binary)],
            cwd=home,
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=True,
        )
        try:
            deadline = time.monotonic() + seconds
            mapped_window = False
            while time.monotonic() < deadline:
                exit_code = process.poll()
                if exit_code is not None:
                    stdout, stderr = process.communicate(timeout=2)
                    raise RuntimeError(
                        f"Native GUI exited before startup smoke completed (exit={exit_code}):\n"
                        f"stdout: {stdout.decode(errors='replace')[-3000:]}\n"
                        f"stderr: {stderr.decode(errors='replace')[-6000:]}"
                    )
                if not mapped_window:
                    windows = subprocess.run(
                        ["xdotool", "search", "--onlyvisible", "--name", "Quark"],
                        capture_output=True,
                        check=False,
                        text=True,
                    )
                    mapped_window = windows.returncode == 0 and bool(windows.stdout.strip())
                time.sleep(0.1)
            if not mapped_window:
                raise RuntimeError("Quark process stayed alive but no mapped GUI window was visible.")
            print(f"Quark native GTK startup smoke passed: mapped window visible for {seconds:.1f} seconds.")
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.communicate(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.communicate(timeout=5)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("Usage: native-startup-smoke.py /absolute/path/to/quark-native")
    try:
        check_startup(Path(sys.argv[1]).resolve())
    except (RuntimeError, OSError) as exc:
        print(f"FAILED: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
