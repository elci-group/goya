"""Experimental local build checks, separate from browser and release qualification."""
from html.parser import HTMLParser
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
MAJA = os.environ.get("MAJA_BIN")


class Tags(HTMLParser):
    def __init__(self):
        super().__init__()
        self.tags = []
        self.text = []

    def handle_starttag(self, tag, attrs):
        self.tags.append(tag)

    def handle_data(self, data):
        self.text.append(data)


@unittest.skipUnless(MAJA, "MAJA_BIN not set: compiler-dependent tests not run")
class ExperimentalBuild(unittest.TestCase):
    def build(self, fixture, output, *extra):
        return subprocess.run([MAJA, "build", str(HERE / fixture), "--profile",
                               "maja.web.local.v1", "--out", str(output), *extra],
                              capture_output=True, timeout=30)

    def test_static_build_is_byte_identical(self):
        with tempfile.TemporaryDirectory() as temp:
            a, b = Path(temp) / "a", Path(temp) / "b"
            for path in (a, b):
                result = self.build("static.gscene", path)
                self.assertEqual(result.returncode, 0, result.stderr)
            files_a = sorted(p.relative_to(a) for p in a.rglob("*") if p.is_file())
            files_b = sorted(p.relative_to(b) for p in b.rglob("*") if p.is_file())
            self.assertTrue(files_a)
            self.assertEqual(files_a, files_b)
            for name in files_a:
                self.assertEqual((a / name).read_bytes(), (b / name).read_bytes(), str(name))

    def test_authored_markup_is_text(self):
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp) / "out"
            result = self.build("escape.gscene", out)
            self.assertEqual(result.returncode, 0, result.stderr)
            parser = Tags()
            parser.feed((out / "index.html").read_text())
            self.assertNotIn("script", parser.tags)
            self.assertIn("<script>alert('x')</script> & hi", parser.text)

    def test_release_fails_and_existing_output_is_preserved(self):
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp) / "out"
            result = self.build("static.gscene", out, "--release")
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(out.exists())
            out.mkdir()
            marker = out / "existing"
            marker.write_bytes(b"preserve")
            result = self.build("static.gscene", out)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(marker.read_bytes(), b"preserve")
            self.assertEqual([p.name for p in out.iterdir()], ["existing"])


if __name__ == "__main__":
    unittest.main()
