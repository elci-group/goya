# Local package installation

Build a Linux package with `scripts/package-local.sh [output-directory]`.
The output is a versioned `.tar.gz` file, a neighboring `.sha256` sidecar,
and a versioned standalone installer script. Copy all three files together to
the offline machine. The installer does not download anything; it requires
`tar`, `sha256sum`, `awk`, `mktemp`, and the usual POSIX file utilities to be
available there.

Install the two commands into `~/.local/bin`:

```sh
sh goya-VERSION-linux-ARCH-install.sh goya-VERSION-linux-ARCH.tar.gz
```

Choose another executable directory with `--dest`:

```sh
sh goya-VERSION-linux-ARCH-install.sh --dest "$HOME/bin" goya-VERSION-linux-ARCH.tar.gz
```

The installer requires the archive's matching `.sha256` file and checks the
digest before extraction. It rejects absolute paths, parent traversal, link
entries, and member names outside the package's single top-level directory.
It extracts into a private temporary directory, then stages each executable
inside the destination before installing it. Existing files are preserved by
default. Pass `--force` to replace existing regular `goya-studio` or
`goya-scene` files; symlinks and other non-regular targets are refused even
with that option.

To inspect a package's checksum and archive paths without changing an install
directory, run:

```sh
sh goya-VERSION-linux-ARCH-install.sh --verify-only goya-VERSION-linux-ARCH.tar.gz
```

The SHA-256 sidecar detects accidental corruption and mismatched files. By
itself it does not establish who produced the package.

## Optional Ed25519 publisher signature

The builder can create a detached signature over the exact archive bytes.
Pass the publisher's private key explicitly:

```sh
scripts/package-local.sh --out-dir dist --signing-key /secure/keys/goya-release-private.pem
```

This writes `ARCHIVE.tar.gz.sig` alongside the archive and checksum. The
private key is read in place and is not copied into the package or output
directory. Distribute the `.tar.gz`, `.sha256`, `.sig`, and installer together.
The `.sha256` file is still only a corruption check; the detached signature
authenticates the archive only relative to a public key the installer trusts.

The installer verifies a signature only when the operator supplies a trusted
public key separately:

```sh
sh goya-VERSION-linux-ARCH-install.sh --verify-only \
  --trusted-key /secure/keys/goya-release-public.pem goya-VERSION-linux-ARCH.tar.gz
```

`--trusted-key` requires the matching `.tar.gz.sig` file and verifies Ed25519
before inspecting or installing the archive. Without that option, the
installer checks only SHA-256 integrity and explicitly reports that publisher
authenticity was not checked. Key generation, public-key distribution,
publisher identity, and release publication remain release-operator duties.

## Rollback and removal

The installer places only `goya-studio` and `goya-scene` in the selected
directory. To remove this installation, delete those two paths from that
directory, for example:

```sh
rm "$HOME/.local/bin/goya-studio" "$HOME/.local/bin/goya-scene"
```

For a non-default destination, use the same directory passed to `--dest`.
If `--force` replaced earlier versions, restore those from your own backup;
the installer does not keep copies of overwritten files. Package data and
documentation remain in the original archive and are not copied into the
destination.
