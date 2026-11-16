# Dockyard

![Dockyard mark. An engraving of a dock superintendent among cargo, papers, and barrels.](../../assets/Dockyard-logo.jpeg)

This engraving is the Dockyard mark, stored at `assets/Dockyard-logo.jpeg`. It identifies the Dockyard documents. `build` does not copy it into the executable or the image layer.

Dockyard compiles an admitted HTTP service to a static Linux executable and an OCI image layout.

The profile is `dockyard.linux.http/0.0.1`. It admits one `GET` route and rejects scenes, secrets, and every target except `x86_64-unknown-linux-gnu`. The executable is linked with `crt-static` and listens on `127.0.0.1`. The image user in the layout is `65532:65532`. Building the layout does not require a container daemon. `verify` runs the executable, serves the admitted GET for HTTP/1.0 and HTTP/1.1, requires that exact three-header block, rejects every other request line it sends, records a `LISTEN` socket on `127.0.0.1` and no IPv6 listener for that port, writes the admitted GET twice on one connection and requires one response, serves the next connection, rejects an unterminated header and a line-feed-only header with no response bytes, accepts a header split across two writes, rejects a lowercase method, a case-flipped path, and an empty query, accepts one response to `Connection: keep-alive`, rejects `HTTP/1.10` and a doubled space, does not answer `Expect: 100-continue` with `100 Continue`, rejects a lowercase version and a dot segment, accepts one response to a 2048-byte header, rejects a tab after `GET` and a request line with no version, produces no response when the header terminator begins after 2048 bytes, rejects a trailing space and `HTTP/2.0`, returns `404` for a request that is only the header terminator, and still reports container start as `BLOCKED`. The layer reader rejects a non-regular tar typeflag, a non-`ustar` magic, a link name, a prefix, a non-zero mtime, a device number, a name that is not the profile shape, an owner or mode other than `65532` and `0555`, a version other than `00`, a user or group name other than `nonroot`, a name field that is not NUL-terminated and zero-padded, a numeric field that is not zero-padded octal, a checksum field whose trailing byte is not a space, and a non-zero byte in the header pad. `start` records that same loopback listener in the container network namespace, requires the only interface name there to be `lo`, rejects a foreign route, a gateway, or a default mask, rejects an IPv6 address that is not on `lo`, rejects an ARP neighbor, rejects a TCP peer that is not loopback, and rejects a UDP, raw, UDP6, packet, UDP-Lite, ICMP, or ICMPv6 socket. `start` runs that layout with `runc` when `runc` is already installed. On this unprivileged host the route runs in a loopback-only network namespace, the host port stays closed, the root is read-only, capabilities are empty, CPU and memory limits are unset, and image user `65532:65532` is not applied. Runtime qualification stays `BLOCKED`.

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
