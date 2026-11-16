#!/usr/bin/env python3
"""Install the pinned Linux Swift toolchain from SHA-256-verified archives."""

import argparse
import fcntl
import hashlib
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.error
import urllib.request

SWIFTLY_VERSION = "1.2.0"
SWIFT_VERSION = "6.1.2"
TOOLCHAIN_ROOT = "swift-6.1.2-RELEASE-ubuntu24.04"
ARTIFACTS = {
    "swiftly-x86_64-1.2.0.tar.gz": (
        "https://download.swift.org/swiftly/linux/swiftly-1.2.0-x86_64.tar.gz",
        "0b26b568811404374c7d7f07276b6aad4145db5d1893a45f307de6a94ca8d9c3",
    ),
    "swift-6.1.2-RELEASE-ubuntu24.04.tar.gz": (
        "https://download.swift.org/swift-6.1.2-release/ubuntu2404/"
        "swift-6.1.2-RELEASE/swift-6.1.2-RELEASE-ubuntu24.04.tar.gz",
        "d749d5fe2d6709ee988e96b16f02bca7b53304d09925e31063fd5ec56019de9f",
    ),
}


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def acquire(cache, name, url, expected, offline):
    path = cache / name
    if path.is_file():
        if digest(path) == expected:
            return path
        raise RuntimeError(f"cached artifact failed SHA-256 verification: {path}")
    if offline:
        raise RuntimeError(f"offline dependency missing: {path}; run without --offline first")
    fd, temporary = tempfile.mkstemp(prefix=".download-", dir=cache)
    try:
        with os.fdopen(fd, "wb") as output, urllib.request.urlopen(url, timeout=60) as source:
            shutil.copyfileobj(source, output, 1024 * 1024)
        if digest(Path(temporary)) != expected:
            raise RuntimeError(f"downloaded artifact failed SHA-256 verification: {name}")
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)
    return path


def unpack(archive, destination):
    # Python's data filter rejects absolute paths, traversal, unsafe links and
    # special files before writing them; archives must already match the pin.
    with tarfile.open(archive, "r:gz") as source:
        source.extractall(destination, filter="data")


def install(cache, target, offline):
    cache.mkdir(parents=True, exist_ok=True)
    target.parent.mkdir(parents=True, exist_ok=True)
    with (cache / ".bootstrap.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        artifacts = {name: acquire(cache, name, url, sha, offline)
                     for name, (url, sha) in ARTIFACTS.items()}
        if target.exists() or target.is_symlink():
            raise RuntimeError(f"installation path already exists: {target}; choose a new path")
        staging = Path(tempfile.mkdtemp(prefix=".swift-install-", dir=target.parent))
        try:
            unpack(artifacts["swiftly-x86_64-1.2.0.tar.gz"], staging)
            unpack(artifacts["swift-6.1.2-RELEASE-ubuntu24.04.tar.gz"], staging)
            swiftly = staging / "swiftly"
            swift = staging / TOOLCHAIN_ROOT / "usr/bin/swift"
            for command, wanted in ((swiftly, SWIFTLY_VERSION), (swift, f"Swift version {SWIFT_VERSION}")):
                actual = subprocess.check_output([str(command), "--version"], text=True, stderr=subprocess.STDOUT)
                if wanted not in actual:
                    raise RuntimeError(f"unexpected version from {command}: {actual.strip()}")
            os.rename(staging, target)
        finally:
            if staging.exists():
                shutil.rmtree(staging)
    return target / TOOLCHAIN_ROOT / "usr/bin"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--offline", action="store_true", help="use verified cached artifacts only")
    parser.add_argument("--cache", type=Path, default=Path(os.environ.get("XDG_CACHE_HOME", Path.home() / ".cache")) / "goya/swift")
    parser.add_argument("--install-dir", type=Path, default=Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local/share")) / "goya/swift-6.1.2")
    args = parser.parse_args()
    if sys.version_info < (3, 12):
        parser.error("Python 3.12 or newer is required for safe archive extraction")
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        parser.error("only Linux x86_64 with Ubuntu 24.04-compatible libraries is supported")
    try:
        binary_dir = install(args.cache, args.install_dir, args.offline)
    except (OSError, RuntimeError, subprocess.SubprocessError, tarfile.TarError, urllib.error.URLError) as error:
        print(f"swift bootstrap: {error}", file=sys.stderr)
        return 1
    print(f"Installed Swiftly {SWIFTLY_VERSION} and Swift {SWIFT_VERSION} in {args.install_dir}")
    print(f"Swift binaries: {binary_dir}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
