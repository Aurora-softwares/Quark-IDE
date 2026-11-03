"""Regressions for the coverage gate itself; no third-party Python packages."""
import importlib.util
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch
from types import SimpleNamespace

spec = importlib.util.spec_from_file_location("feature_gate", Path(__file__).with_name("check-feature-coverage.py"))
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)


class CoverageGateTests(unittest.TestCase):
    def register(self, tests=None):
        return {"schema_version": 1, "minimum_tests": 2, "features": [
            {"id": "example", "category": "Example", "feature": "Example behaviour",
             "tests": tests if tests is not None else ["test_a", "test_b"]}]}

    def test_two_registered_distinct_tests_pass(self):
        self.assertEqual(gate.validate(self.register(), {"test_a", "test_b"}), [])

    def test_one_test_fails(self):
        self.assertTrue(gate.validate(self.register(["test_a"]), {"test_a"}))

    def test_duplicate_test_cannot_satisfy_minimum(self):
        self.assertTrue(gate.validate(self.register(["test_a", "test_a"]), {"test_a"}))

    def test_missing_and_ignored_tests_do_not_count(self):
        self.assertTrue(gate.validate(self.register(), {"test_a"}))
        self.assertTrue(gate.validate(self.register(), {"test_a", "test_b"}, {"test_b"}))

    def test_higher_minimum_and_duplicate_ids_are_enforced(self):
        register = self.register()
        register["minimum_tests"] = 3
        self.assertTrue(gate.validate(register, {"test_a", "test_b"}))
        register["minimum_tests"] = 2
        register["features"].append(register["features"][0])
        self.assertTrue(gate.validate(register, {"test_a", "test_b"}))

    def test_platform_specific_features_only_require_tests_on_their_platform(self):
        register = self.register()
        register["features"][0]["platforms"] = ["linux", "darwin"]
        self.assertEqual(gate.validate(register, set(), platform="win32"), [])
        self.assertTrue(gate.validate(register, set(), platform="linux"))

    def test_list_parser_counts_tests_not_benchmarks_or_summaries(self):
        self.assertEqual(gate.test_names("a::test: test\nb::bench: benchmark\n1 test, 1 benchmark\n"), {"a::test"})

    def test_native_and_legacy_test_lists_are_collected_separately(self):
        outputs = [
            SimpleNamespace(stdout="legacy::test: test\n"),
            SimpleNamespace(stdout="native::test: test\n"),
        ]
        with patch.object(gate.subprocess, "run", side_effect=outputs) as run:
            self.assertEqual(gate.listing(), {"legacy::test", "native::test"})
        self.assertEqual(run.call_count, 2)
        first = run.call_args_list[0].args[0]
        second = run.call_args_list[1].args[0]
        self.assertIn("legacy-ui", first)
        self.assertIn("native-ui", second)
        self.assertIn("--no-default-features", first)
        self.assertIn("--no-default-features", second)

    def test_ignored_native_and_legacy_test_lists_are_both_checked(self):
        with patch.object(
            gate.subprocess, "run",
            side_effect=[
                SimpleNamespace(stdout="legacy::ignored: test\n"),
                SimpleNamespace(stdout="native::ignored: test\n"),
            ],
        ) as run:
            self.assertEqual(
                gate.listing(ignored=True),
                {"legacy::ignored", "native::ignored"},
            )
        for call in run.call_args_list:
            self.assertIn("--ignored", call.args[0])

    def test_test_listing_failure_reports_cargo_diagnostics(self):
        failure = subprocess.CalledProcessError(
            101, ["cargo", "test"], stderr="GTK4 development files missing"
        )
        with patch.object(gate.subprocess, "run", side_effect=failure):
            with self.assertRaisesRegex(SystemExit, "GTK4 development files missing"):
                gate.listing()

    def test_empty_and_invalid_registers_fail(self):
        self.assertTrue(gate.validate({}, set()))
        register = self.register()
        register["minimum_tests"] = 1
        self.assertTrue(gate.validate(register, {"test_a", "test_b"}))


if __name__ == "__main__":
    unittest.main()
