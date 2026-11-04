# C2PA interoperability

`goya-provenance::C2paTool` delegates manifest creation to the official
`c2patool` executable. Goya passes an asset and a temporary manifest
definition containing the custom assertion `org.goya.provenance.claims`.
The official tool creates the JUMBF manifest store, asset hard binding, claim
signature, and media-format embedding. Goya's legacy `SignedManifest2` is a
separate internal signature format and is **not** a C2PA manifest.

## Requirements

- Install the official `c2patool` executable and provide it on `PATH`, or use
  `C2paTool::with_executable` to select its path.
- Configure a signer executable with `C2paTool::sign_asset`. The signer must
  implement c2patool's signer subprocess protocol. Also provide its PEM
  certificate-chain path and a `C2paSigningAlgorithm`; c2patool v0.9.12
  requires both `sign_cert` and `alg` in the manifest definition when using
  `--signer-path`. Goya deliberately does not select c2patool's development
  test key or put private keys in the manifest definition.
- Use an asset format supported by the installed C2PA tool. The destination
  extension must match the source format. Existing destinations are rejected
  unless overwrite is explicitly enabled.

Example:

```rust,ignore
use goya_provenance::{C2paSigningAlgorithm, C2paTool, ManifestClaims};

let report = C2paTool::new().sign_asset(
    "input.jpg",
    "output.jpg",
    "/usr/local/bin/goya-c2pa-signer",
    "/etc/goya/release-chain.pem",
    C2paSigningAlgorithm::Es256,
    &claims,
    false,
)?;
assert!(report.output_bytes.unwrap_or(0) > 0);
```

`inspect_asset` returns c2patool's raw manifest report. Reading an asset is
not equivalent to trusting its signer: configure and apply the official
C2PA trust anchors/allow-list policy in the c2patool environment or invocation
before treating a signature as trusted. The adapter does not infer trust from
the Goya `key_role` metadata.

The integration follows the C2PA requirement that a standard manifest contain
an assertion store, claim, claim signature, and one asset hard-binding
assertion. See the [C2PA 2.3 specification, §11](https://spec.c2pa.org/specifications/specifications/2.3/specs/C2PA_Specification.html#_types_of_manifests),
the [official c2patool manifest format](https://github.com/contentauth/c2patool/blob/main/docs/manifest.md),
and [signing guidance](https://github.com/contentauth/c2patool/blob/main/docs/signing.md).

The repository's automated adapter tests use a controlled fake executable to
verify argv separation, input/output protections, claim escaping, and error
handling. They do not replace an interoperability check with a real c2patool
build and a production trust chain; those require the external tool and signer.

## Interoperability smoke

On 2026-11-04, the adapter signed a 32×32 PNG using official `c2patool`
v0.9.12, an ephemeral P-256 key/certificate, and an OpenSSL subprocess signer.
The output was 21,893 bytes; c2patool re-read it and reported an ES256 signature
and the `org.goya.provenance.claims` assertion. The temporary self-signed
certificate was not on a production trust list, so this demonstrates real
format/signature interoperability but **does not establish a trusted
production signing chain**. No test key or signed asset is stored in this
repository.
