# Pinned Swift bootstrap

`scripts/bootstrap-swift.py` installs **Swiftly 1.2.0** and **Swift 6.1.2**
for Linux x86-64 on Ubuntu 24.04-compatible systems. It installs directly into
an isolated directory; it does not modify shell profiles or the system Swift
installation. Swiftly is supplied as a pinned executable; the bootstrap
unpacks the pinned Swift toolchain directly so installation never asks
Swiftly to resolve versions or fetch a different toolchain.

| Artifact | Authoritative source | SHA-256 |
| --- | --- | --- |
| Swiftly 1.2.0 Linux x86-64 | [swiftly-1.2.0-x86_64.tar.gz](https://download.swift.org/swiftly/linux/swiftly-1.2.0-x86_64.tar.gz) | `0b26b568811404374c7d7f07276b6aad4145db5d1893a45f307de6a94ca8d9c3` |
| Swift 6.1.2 Ubuntu 24.04 x86-64 | [Swift 6.1.2 toolchain](https://download.swift.org/swift-6.1.2-release/ubuntu2404/swift-6.1.2-RELEASE/swift-6.1.2-RELEASE-ubuntu24.04.tar.gz) | `d749d5fe2d6709ee988e96b16f02bca7b53304d09925e31063fd5ec56019de9f` |

The hashes were computed from the official downloads. At pinning time their
detached signatures verified against the Swift.org published keys: Swiftly
automatic signing key #4 (`E813C892820A6FA13755B268F167DF1ACF9CE069`) and the
Swift 6.x release key (`52BB7E3DE28A71BE22EC05FFEF80A866B47A981F`). The
bootstrap verifies the pinned SHA-256, not a live GPG trust chain. Review both
the upstream signature and the new hashes when updating these pins.

Run `python3 scripts/bootstrap-swift.py` online. By default it caches archives
under `${XDG_CACHE_HOME:-~/.cache}/goya/swift` and installs to
`${XDG_DATA_HOME:-~/.local/share}/goya/swift-6.1.2`. The command prints the
directory containing `swift` and `swiftc`; add it to `PATH` for the current
process if needed. Python 3.12+, system libraries required by the Swift
toolchain, and free space for both compressed and extracted artifacts are
required. The install destination must be new.

For a disconnected machine, transfer both previously verified cache files,
then run `python3 scripts/bootstrap-swift.py --offline`. Use `--cache PATH` and
`--install-dir PATH` to choose locations. Every cache use is rehashed; a
missing or corrupt artifact fails with an explicit error, with no system
Swift fallback and no network in offline mode. A failed installation is not
published. Python's tar data filter rejects unsafe archive paths and links.

CI runs fixture tests and both online and offline installs on Ubuntu 24.04.
These are Linux compiler checks; they do not supply Xcode, the iOS SDK, signing,
or iOS device qualification.
