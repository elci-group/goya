# Goya, in plain language

A local report page. It explains what Goya is, how its habits differ from
ordinary frontend work, and which kinds of software that shape can carry.
It is a reading of the repository, not a release announcement.

This package is its own Cargo workspace. It is not a member of the Goya
workspace, and it has no third-party dependencies.

## Run

From this directory:

```sh
cargo run --offline -- --port 4188
```

Then open <http://127.0.0.1:4188/>.

The process binds `127.0.0.1` only. `--bind` accepts another address when you
need one. `--self-check` confirms the page contains its sections and exits.

Fira Sans and Fira Sans Compressed are included so the page does not fall
back to a default face. They are Mozilla’s fonts, under the SIL Open Font
License.

## Routes

| Path | Body |
| --- | --- |
| `/` | The report |
| `/report.css` | Styles |
| `/favicon.svg` | Mark |
