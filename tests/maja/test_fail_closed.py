"""Black-box admission checks; this suite does not qualify browser output."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent
CASES = json.loads((ROOT / "golden" / "rejection-cases.json").read_text())
MAJA = os.environ.get("MAJA_BIN")


@unittest.skipUnless(MAJA, "MAJA_BIN not set: compiler-dependent tests not run")
class FailClosed(unittest.TestCase):
    def run_maja(self, *args):
        return subprocess.run([MAJA, *map(str, args)], capture_output=True, timeout=30)

    def test_check_rejects_every_unavailable_profile_case(self):
        for case in CASES:
            with self.subTest(case=case):
                source = ROOT / "fixtures" / case["fixture"]
                result = self.run_maja("check", source, "--target", "web", "--profile", case["profile"])
                self.assertNotEqual(result.returncode, 0)
                self.assertTrue(result.stderr.strip(), "rejection requires a diagnostic")

    def test_build_rejects_without_publishing_files(self):
        for case in CASES:
            with self.subTest(case=case), tempfile.TemporaryDirectory(prefix="maja-negative-") as temp:
                source = ROOT / "fixtures" / case["fixture"]
                output = Path(temp) / "dist"
                result = self.run_maja("build", source, "--profile", case["profile"], "--release", "--out", output)
                self.assertNotEqual(result.returncode, 0)
                self.assertTrue(result.stderr.strip(), "rejection requires a diagnostic")
                self.assertFalse(output.exists() and any(output.rglob("*")), "failed build published files")

    def test_rejection_diagnostic_is_deterministic(self):
        source = ROOT / "fixtures" / "static.gscene"
        args = ("check", source, "--target", "web", "--profile", "maja.web.v1")
        first, second = self.run_maja(*args), self.run_maja(*args)
        self.assertNotEqual(first.returncode, 0)
        self.assertEqual((first.returncode, first.stdout, first.stderr),
                         (second.returncode, second.stdout, second.stderr))


if __name__ == "__main__":
    unittest.main()
