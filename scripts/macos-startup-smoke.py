#!/usr/bin/env python3
"""AppKit startup-lifetime smoke for the actual Quark IDE binary.

This check runs on the macOS GitHub Actions runner; a successful compilation
alone never proves NSApplication can create and retain the Start Window.
It does not claim that UI screenshots, gestures or keyboard input passed.
"""
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time


def check_startup(binary: Path, seconds: float = 6.0) -> None:
    if sys.platform != "darwin":
        raise RuntimeError("AppKit startup validation must run on macOS")
    if not binary.is_file():
        raise RuntimeError(f"Missing native Quark executable: {binary}")
    with tempfile.TemporaryDirectory(prefix="quark-macos-smoke-") as work:
        env = os.environ.copy()
        env["XDG_CONFIG_HOME"] = work
        process = subprocess.Popen(
            [str(binary)],
            cwd=work,
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=True,
        )
        try:
            deadline = time.monotonic() + seconds
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    stdout, stderr = process.communicate(timeout=3)
                    raise RuntimeError(
                        f"AppKit Quark exited during startup (exit {process.returncode})\n"
                        f"stdout: {stdout.decode(errors='replace')[-3000:]}\n"
                        f"stderr: {stderr.decode(errors='replace')[-6000:]}"
                    )
                time.sleep(0.15)
            print(f"AppKit Quark startup lifetime passed: process remained alive {seconds:.1f}s")
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
        raise SystemExit("Usage: macos-startup-smoke.py /path/to/quark-native")
    try:
        check_startup(Path(sys.argv[1]).resolve())
    except (OSError, RuntimeError) as error:
        print(f"FAILED: {error}", file=sys.stderr)
        raise SystemExit(1) from error
