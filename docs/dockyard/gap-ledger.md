# Dockyard gap ledger

Profile `dockyard.linux.http/0.0.1` is the only admitted compiler input. A row stays open until a later profile names it and a test records the result.

| Gap | Current gate | What remains |
| --- | --- | --- |
| Scene lowering | `@cell`, `@button`, `@input`, `@ontology`, `@interaction`, and `@gesture` return `DK-E0100`. | No scene, D1, or D2 program is a Dockyard input. |
| Frampton, Tov, VEE | The crate links none of those crates and does not spawn their tools. | A version-locked adapter would be a separate admission. |
| musl and other architectures | Any target other than `x86_64-unknown-linux-gnu` returns `DK-E0201`. | Each triple needs its own installed standard library and a separate run of the executable. |
| Container start | `dockyard start` on this host exited 3. It printed `image-start: pass`, `runtime: runc`, `rootfs: read-only`, `capabilities: dropped`, `network: host-namespace`, `runtime-user: 0 mapped to 1000`, `user-applied: BLOCKED`, and `runtime-qualification: BLOCKED`. `verify` still exits 3 and prints `image-start: BLOCKED`. | Applying image user 65532, and a network namespace, remain open. |
| Read-only root, capabilities, resource limits | The runc start path recorded a read-only root, empty `CapEff`, `CapPrm`, and `CapBnd`, and `RLIMIT_NOFILE` 1024. | CPU and memory limits are unset. The image config user was not the process uid. |
| Secrets | A `secret` statement returns `DK-E0101`. The layer is checked for the absence of source statements. | Runtime injection is not representable in this profile. |
| Kubernetes, SBOM, provenance | No manifest, SBOM, or attestation is written. | Each one is a later artifact with its own checker. |
| Release | `--release` returns `DK-E0202`. | Release admission needs a signed publication policy. |
