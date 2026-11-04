# Local package installation

Build a Linux package with `scripts/package-local.sh [output-directory]`.
The output is a versioned `.tar.gz` file and a neighboring `.sha256` sidecar.
Copy both files together to the offline machine. The installer does not
download anything; it requires `tar`, `sha256sum`, `awk`, `mktemp`, and the
usual POSIX file utilities to be available there.

Install the two commands into `~/.local/bin`:

```sh
sh scripts/install-local.sh goya-VERSION-linux-ARCH.tar.gz
```

Choose another executable directory with `--dest`:

```sh
sh scripts/install-local.sh --dest "$HOME/bin" goya-VERSION-linux-ARCH.tar.gz
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
sh scripts/install-local.sh --verify-only goya-VERSION-linux-ARCH.tar.gz
```

The SHA-256 sidecar detects accidental corruption and mismatched files. It is
not a publisher signature, so it does not establish who produced the package.

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
