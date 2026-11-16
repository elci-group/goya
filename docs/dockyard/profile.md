# Dockyard profile `dockyard.linux.http/0.0.1`

Dockyard compiles one admitted HTTP service to a static Linux executable and an OCI image layout. It does not admit a `.gscene` file. `build` and `verify` do not start a container. `start` runs an existing layout when `runc` is already on `PATH`.

The target is `x86_64-unknown-linux-gnu` linked with `crt-static`. That produces a static-pie ELF on a host that already has this standard library. Dockyard does not download a target, a base image, or a toolchain. `x86_64-unknown-linux-musl`, ARM, 32-bit, and Windows triples are rejected.

```text
dockyard inspect service.gservice
dockyard check service.gservice --target x86_64-unknown-linux-gnu --profile dockyard.linux.http/0.0.1
dockyard build service.gservice --target x86_64-unknown-linux-gnu --profile dockyard.linux.http/0.0.1 --out NEW_DIR
dockyard verify NEW_DIR
dockyard start NEW_DIR
```

`check` writes nothing. `build` exits 0 only after the executable and the layout exist. That success still reports image start and runtime qualification as `BLOCKED`. `verify` runs the executable from the image layer, checks the admitted route and one unknown path, and exits 3. It leaves image start `BLOCKED` even when `runc` is installed. `--release` is rejected.

## Admitted source

Exactly four lines, in this order. A final newline is optional. Blank lines, comments, CR, and NUL are rejected.

```text
service ping
listen 39171
route GET /health
respond 200 text/plain "ok"
```

`service` is a lowercase name of at most 31 characters. `listen` is a port from 1024 through 65535, without leading zeros, so the image user can bind it. `route` is `GET` and one absolute path of at most 128 bytes. Segments use ASCII letters, digits, and `.` `_` `~` `-`. `.` and `..` segments are rejected. `respond` is status `200`, media type `text/plain`, and a quoted body of 1 to 512 printable ASCII bytes. The body has no quotes or backslashes.

Presentation (`@cell`, `@button`, `@input`, `@ontology`, `@interaction`, `@gesture`) and operational statements (`secret`, `volume`, `env`, `kubernetes`, and the other words in the admission table) are errors. They are not stripped.

## Output

`NEW_DIR` must be a new directory whose parent already exists. A failed build removes its temporary directory and does not leave `NEW_DIR`.

```text
NEW_DIR/service
NEW_DIR/oci-layout
NEW_DIR/index.json
NEW_DIR/blobs/sha256/...
NEW_DIR/evidence/report.txt
NEW_DIR/evidence/sbom.spdx.json
NEW_DIR/evidence/provenance.json
```

`service` is the static executable. The OCI layout follows image layout 1.0.0. The single uncompressed layer contains `srv/dockyard/service` with mode `0555` and uid/gid `65532`. The image config sets `User` to `65532:65532`, an absolute entrypoint, the listened port, and labels for the admitted route. `created` is `1970-01-01T00:00:00Z`. The same source bytes produce the same executable, layer, config, manifest, report, SBOM, and provenance predicate on this toolchain. Those three evidence files record SHA-256 digests and do not record a host path.

`evidence/sbom.spdx.json` is SPDX 2.3. Its one package checksum is the executable. The layer digest appears only in the document namespace. `evidence/provenance.json` lists the source, executable, layer, config, and manifest digests and sets `signature` to null. Neither file is copied into the layer. `verify` recomputes both. A mismatch exits 4 with `DK-E4001`.

`verify` accepts only that tree. `blobs/sha256` contains the manifest, the config, and the layer named by the index, and nothing else. `evidence` contains `report.txt`, `sbom.spdx.json`, and `provenance.json`, and nothing else. A planted `evidence/kubernetes.yaml` and an extra blob each made `verify` exit 4 with `DK-E4001` and `layout is not the closed profile set`. A symlink made `verify` exit 4 with `DK-E4001` and `layout contains a symlink`. Those failures did not print a host path.

The response body is the service contract, so it is present in the executable. Source statements, secrets, and evidence are not copied into the layer. There is no secret syntax to embed.

`verify` checks layout digests, the non-root layer, and the report. It then starts the layer bytes. An unknown path returns `404` and `not found`. The admitted `GET` returns `200`, `text/plain`, and the body named by the image label, for both `HTTP/1.0` and `HTTP/1.1`. The status line is `HTTP/1.1` and the response includes `connection: close`. On the fixture, `POST /health`, `GET /health?x`, and `GET /healthx` each returned `404` and `not found`. That process check leaves `image-start: BLOCKED` and exits 3.

`start` reads the same layout and does not write into it. A missing or rejected layout exits 4 with `DK-E4001`. When `runc` cannot be executed, `start` exits 3 and prints `image-start: BLOCKED` with the detail `runc is unavailable. Dockyard does not install a runtime.` Dockyard does not start a Docker daemon. When `runc` is present, the command extracts the layer executable into a bundle and runs `/srv/dockyard/service` in a new network namespace, with an empty capability set, a read-only root, and `RLIMIT_NOFILE` 1024. Loopback is the only interface. The unknown path and the admitted path are requested inside that namespace. While the process is running, a connection to the admitted port on the host is refused. On this unprivileged host, `runc` accepts only a mapping that covers container uid 0, and the caller can map one uid. The process therefore runs as container uid 0 mapped to the caller. The recorded result exits 3:

```text
image-start: pass
runtime: runc
rootfs: read-only
capabilities: dropped
cpu: unset
memory: unset
network: namespace
runtime-user: 0 mapped to 1000
user-applied: BLOCKED
runtime-qualification: BLOCKED
detail: image user 65532:65532 was not applied. This caller can map one uid and runc requires container uid 0.
```

The image config still names user `65532:65532`. A caller that is uid 0 and gid 0 uses that image user and no user namespace. This unprivileged host cannot apply that user: `/etc/subuid` and `/etc/subgid` are empty, `newuidmap` is absent, and a second uid mapping makes runc fail with `mapping tool not present: Operation not permitted`.

`cpu` and `memory` are read from the running process cgroup. On this host both are `unset`. `user@1000.service` accepts writes to `memory.max` and `cpu.max`, and moving a process into that cgroup is refused because `user-1000.slice/cgroup.procs` is not writable by this user. `start` does not set `linux.resources`. Doing so makes runc fail with `cgroup.procs: permission denied`.

Runtime qualification still exits 3. A started process that misses the admitted route, the read-only root, the empty capabilities, the open-file limit, or the closed host port exits 4. A host-port check that neither connects nor is refused exits 4 with the connect error.

This profile is not a release admission. CPU and memory limits, Kubernetes manifests, and multi-arch indexes are outside it. The provenance predicate is unsigned.
