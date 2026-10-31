#!/usr/bin/env python3
"""Require two distinct, registered, non-ignored Rust tests per applicable feature.

This checks the reviewed feature register, not line coverage or the quality of
assertions. New/changed UI behaviour must be added to the register in review.
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def test_names(output):
    return {line[:-6] for line in output.splitlines() if line.endswith(": test")}


def validate(register, available, ignored=frozenset(), platform=sys.platform):
    errors = []
    if register.get("schema_version") != 1:
        errors.append("Unsupported feature-register schema")
    minimum = register.get("minimum_tests")
    if not isinstance(minimum, int) or isinstance(minimum, bool) or minimum < 2:
        errors.append("minimum_tests must be at least two")
        minimum = 2
    features = register.get("features", [])
    if not features:
        errors.append("The feature register is empty")
    identifiers = set()
    for feature in features:
        identifier = feature.get("id", "")
        if not identifier or identifier in identifiers:
            errors.append(f"Missing or duplicate feature id: {identifier!r}")
        identifiers.add(identifier)
        if not feature.get("category") or not feature.get("feature"):
            errors.append(f"{identifier}: missing category or feature description")
        if platform not in feature.get("platforms", [platform]):
            continue
        tests = feature.get("tests", [])
        if not isinstance(tests, list) or any(not isinstance(t, str) for t in tests):
            errors.append(f"{identifier}: tests must be an array of registered names")
            continue
        if len(set(tests)) != len(tests):
            errors.append(f"{identifier}: duplicate test references do not count")
        missing = set(tests) - available
        if missing:
            errors.append(f"{identifier}: unregistered tests: {', '.join(sorted(missing))}")
        active = (set(tests) & available) - ignored
        if len(active) < minimum:
            errors.append(f"{identifier}: {len(active)} runnable tests; {minimum} required")
    return errors


def listing(ignored=False):
    command = ["cargo", "test", "--all-targets", "--", "--list"]
    if ignored:
        command.append("--ignored")
    result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, check=True)
    return test_names(result.stdout)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--register", type=Path, default=ROOT / "tests/feature-coverage.json")
    args = parser.parse_args()
    register = json.loads(args.register.read_text(encoding="utf-8"))
    available, ignored = listing(), listing(ignored=True)
    errors = validate(register, available, ignored)
    if errors:
        print("Feature coverage failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    features = register["features"]
    applicable = sum(sys.platform in f.get("platforms", [sys.platform]) for f in features)
    print(f"Feature coverage passed: {applicable}/{len(features)} features apply on {sys.platform}; "
          f"each has at least {register['minimum_tests']} distinct, non-ignored tests. "
          f"{len(available - ignored)} Rust tests registered.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
