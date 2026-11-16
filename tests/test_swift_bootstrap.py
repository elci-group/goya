import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("swift_bootstrap", ROOT / "scripts/bootstrap-swift.py")
bootstrap = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(bootstrap)


def archive(path, files):
    with tarfile.open(path, "w:gz") as tar:
        for name, content in files.items():
            info = tarfile.TarInfo(name)
            info.mode = 0o755
            info.size = len(content)
            tar.addfile(info, io.BytesIO(content))


class BootstrapTests(unittest.TestCase):
    def test_download_cache_offline_and_existing_destination(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            upstream = root / "upstream"
            upstream.mkdir()
            names = list(bootstrap.ARTIFACTS)
            archive(upstream / names[0], {"swiftly": b"#!/bin/sh\necho 1.2.0\n"})
            archive(upstream / names[1], {bootstrap.TOOLCHAIN_ROOT + "/usr/bin/swift": b"#!/bin/sh\necho Swift version 6.1.2\n"})
            pins = {name: ((upstream / name).as_uri(), bootstrap.digest(upstream / name)) for name in names}
            cache = root / "cache"
            with patch.object(bootstrap, "ARTIFACTS", pins):
                first = bootstrap.install(cache, root / "first", False)
                self.assertTrue((first / "swift").is_file())
                with patch.object(bootstrap.urllib.request, "urlopen", side_effect=AssertionError("network accessed")):
                    second = bootstrap.install(cache, root / "second", True)
                self.assertTrue((second / "swift").is_file())
                with self.assertRaisesRegex(RuntimeError, "already exists"):
                    bootstrap.install(cache, root / "second", True)
                (cache / names[0]).write_bytes(b"corrupt")
                with self.assertRaisesRegex(RuntimeError, "failed SHA-256"):
                    bootstrap.install(cache, root / "third", True)
                self.assertFalse((root / "third").exists())

    def test_offline_missing_and_download_hash_mismatch_fail_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            with self.assertRaisesRegex(RuntimeError, "offline dependency missing"):
                bootstrap.acquire(root, "missing", "https://invalid.example", "0" * 64, True)
            source = root / "source"
            source.write_bytes(b"untrusted")
            with self.assertRaisesRegex(RuntimeError, "failed SHA-256"):
                bootstrap.acquire(root, "cached", source.as_uri(), "0" * 64, False)
            self.assertFalse((root / "cached").exists())

    def test_archive_traversal_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            payload = root / "payload.tar.gz"
            archive(payload, {"../escape": b"bad"})
            destination = root / "destination"
            destination.mkdir()
            with self.assertRaises(tarfile.TarError):
                bootstrap.unpack(payload, destination)
            self.assertFalse((root / "escape").exists())


if __name__ == "__main__":
    unittest.main()
