"""Regressions for the coverage gate itself; no third-party Python packages."""
import importlib.util
from pathlib import Path
import unittest

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

    def test_empty_and_invalid_registers_fail(self):
        self.assertTrue(gate.validate({}, set()))
        register = self.register()
        register["minimum_tests"] = 1
        self.assertTrue(gate.validate(register, {"test_a", "test_b"}))


if __name__ == "__main__":
    unittest.main()
