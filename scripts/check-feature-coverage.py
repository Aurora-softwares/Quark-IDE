"""Fail when user-facing features are missing independent coverage entries."""
import json
import pathlib
import re
import sys

root = pathlib.Path(__file__).resolve().parents[1]
catalog = json.loads((root / "tests/feature-coverage.json").read_text())
code = (root / "tests/test_quark_contract.py").read_text()
errors = []
for feature, tests in catalog["features"].items():
    if len(set(tests)) < 2:
        errors.append("%s: fewer than two distinct tests" % feature)
    for test in tests:
        if not re.search(r"^    def %s\(self\):" % re.escape(test), code, re.M):
            errors.append("%s: unknown test %s" % (feature, test))
if errors:
    print("\n".join(errors), file=sys.stderr)
    sys.exit(1)
print("Feature coverage register ok: %s feature areas." % len(catalog["features"]))
