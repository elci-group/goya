# Dockyard

Dockyard compiles an admitted HTTP service to a static Linux executable and an OCI image layout.

The profile is `dockyard.linux.http/0.0.1`. It admits one `GET` route and rejects scenes, secrets, and every target except `x86_64-unknown-linux-gnu`. The executable is linked with `crt-static`. The image user in the layout is `65532:65532`. Building the layout does not require a container daemon. `verify` runs the executable, serves the admitted GET for HTTP/1.0 and HTTP/1.1, rejects every other request line it sends, and still reports container start as `BLOCKED`. `start` runs that layout with `runc` when `runc` is already installed. On this unprivileged host the route runs in a loopback-only network namespace, the host port stays closed, the root is read-only, capabilities are empty, CPU and memory limits are unset, and image user `65532:65532` is not applied. Runtime qualification stays `BLOCKED`.

`build` also writes `evidence/sbom.spdx.json` and `evidence/provenance.json` beside the report. The SBOM is SPDX 2.3 for the executable. The predicate leaves `signature` null. Neither file is inside the image. A document that does not match the layout makes `verify` exit 4. Any other file, including a manifest planted beside the evidence, also makes `verify` exit 4.

The admission table, reject list, and output contract are in `docs/dockyard/profile.md`. Open work is in `docs/dockyard/gap-ledger.md`.

```text
dockyard inspect service.gservice
dockyard check service.gservice --target x86_64-unknown-linux-gnu --profile dockyard.linux.http/0.0.1
dockyard build service.gservice --target x86_64-unknown-linux-gnu --profile dockyard.linux.http/0.0.1 --out NEW_DIR
dockyard verify NEW_DIR
dockyard start NEW_DIR
```

`verify` exits 3 when the layout and the process check pass. `start` exits 3 when `runc` is missing, and when a recorded run still lacks runtime qualification. Both exits are the qualification gate, not usage errors. A rejected layout or a run that misses the admitted route exits 4.
