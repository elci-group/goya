# Goya architecture review

A loopback page that reads this server's model of the Goya backend and draws it.
The model covers the live frame path, the D0/D1/D2 execution path, crate
dependencies taken from the workspace manifests, and the wave ledger in
`docs/contracts.md`.

This package is its own Cargo workspace. It is not a member of the Goya
workspace and it has no third-party dependencies.

## Run

From this directory:

```sh
cargo run --offline -- --port 4187
```

Then open <http://127.0.0.1:4187/>.

The process binds `127.0.0.1` only. `--bind` accepts another address when you
need one. `--self-check` prints model counts and exits.

## Routes

| Path | Body |
|---|---|
| `/` | Review page |
| `/review.css`, `/review.js` | Page assets |
| `/api/model` | Crates, edges, waves, boundaries, findings |
| `/api/trace?event=pointer-button` | One backend walk |

Trace names: `pointer-button`, `pointer-miss`, `key`, `scroll`, `ime`, `swap`, `d0`, `d1`, `d2`, `classify-refuse`.
