# Blue Moon — kaptaind data-loss incidents and recovery, 18 November 2026

Status: **Closed, recovery verified**, 18 November 2026. Operational record
for the Blue Moon programme; read with the incident note in
`docs/blue-moon/roadmap.md`. Every command, hash and time below was re-run
against this repository while writing this document.

## Summary

On 18 November 2026 the Blue Moon programme lost working-tree state twice to
an automated versioning daemon (`kaptaind`) running concurrently with agent
work in the shared checkout. The daemon commits every dirty file every few
minutes on whatever branch is checked out — 117 commits on `main` alone in
the 18:00–22:00 window that day — with subjects like
`fix: apply code changes (registry.rs, registry.rs.tmp.3699.3d116c68f9b4)`
(note the daemon's own scratch file in the list) and trailers like
`kaptaind: Patch -> v13.3.3 [api-stable; paths=2; api_touches=2; deps=0;
runtime=0; score=0.413; cluster=…]`.

**First sweep (~18:59).** The reflog records a history rewrite immediately
before it: `main` reset to `origin/main` at 18:55:51, seven cherry-picks
re-applied at 18:56:01, reset again at 18:57:24. The sweep then took the
working tree with it. The first Blue Moon increment — new, never-committed
files — had no commits to fall back on and was destroyed outright. It was
rebuilt from session records by 19:16 the same day and reconciled before
phase 2.

**Second sweep (20:57).** While the shared tree sat on `sash/phase0-smoke`
(20:26–20:56), the daemon committed both the Blue Moon phase 4/5/8 work and
a concurrent agent's SSO/accelerator workstream to that branch. The last
daemon commit landed at 20:55:47; at 20:56:05 the tree was reset (stash,
then checkout of `main`), removing from disk every file that existed only on
`sash/phase0-smoke` — the whole phase 4/5 Blue Moon surface, BLUE-MOON-8,
and the other workstream's files. Because everything had been committed
seconds earlier, full recovery from git history was possible; the tree was
reconciled by 21:01 and phase 5 continued.

This record is not an accusation. The same high-frequency commit behaviour
that makes the history noisy is why the second incident was recoverable at
all: every lost file had a commit.

## Timeline

| Time | Event | Lost from the working tree | Survived |
| --- | --- | --- | --- |
| 18:55:51–18:57:24 | `main` reset to `origin/main`, seven cherry-picks, reset again (reflog) | — | History rewritten; reflog retains the record |
| ~18:59 | Working-tree sweep | First Blue Moon increment: new files never committed, so no git trace | Nothing in git. Rebuilt from session records 19:03–19:16 (`62a6a4e` 19:09 `bluetooth_caprichio.rs`, `b69a233` 19:12 `bluetooth_runtime.rs`, `7803b76` 19:15 registry wiring, `9fb1eff` 19:16 `BLUE-MOON-1.md`); first `~/.local` backup taken 19:17:34 |
| 20:26 | Shared tree switched to `sash/phase0-smoke` (base `8b818b1`, 19:58:19) | — | Blue Moon phases 4/5/8 continue on that branch |
| 20:26–20:55 | Daemon commits interleave Blue Moon and the SSO/accelerator workstream on `sash/phase0-smoke` | — | Every file committed within minutes of creation |
| 20:53:36 | Off-repo snapshot `~/.local/backup/blue-moon-20261118-205336` | — | Complete pre-sweep Blue Moon surface (9 sources, 8 docs, roadmap) |
| 20:55:47 | Last daemon commit on `sash/phase0-smoke` (`f8f6d12`) | — | Final pre-sweep state committed |
| 20:56:05 | Reset + stash + checkout `main` | Files tracked only on `sash/phase0-smoke` removed from disk: `bluetooth_access_caprichio.rs`, `bluetooth_profile_caprichio.rs`, `crates/neptune/src/bluetooth.rs`, `BLUE-MOON-6/7/8.md`, modifications to `bluetooth_runtime.rs`, `lib.rs`, `registry.rs`, Neptune `lib.rs`/`service.rs`, roadmap; plus the other workstream's files | All committed and reachable on `sash/phase0-smoke`; independent copy in the 20:53:36 backup |
| 20:57:31 | Daemon commits on `main`: `4b0b498` `feat!: change public API (BLUE-MOON-6.md, BLUE-MOON-7.md, +38 more)`, `kaptaind: Major -> v11.0.0 [paths=45]` — actual diff: `VERSION` only | The sweep, recorded: the commit subject still names 45 files that were no longer on disk | Proof the tree was swept between the daemon's analysis and its commit |
| 21:00:47 | Recovery commit `3cb34e3` (`v12.0.0`): re-adds the six swept Blue Moon files, restores the modifications | — | Programme reconciled |
| 21:01:44 | `a53a0b0` (`v12.0.1`) re-adds `goya-caprichios.workspace = true` to `crates/neptune/Cargo.toml` | — | Phase 5 continued on `main`; `nevada/phase0` branched at 21:48 |

## Recovery procedure

The procedure below is what worked, in order, for the 20:57 incident. Run it
from the repository root, on the branch the programme should continue on.

**(a) Locate each lost file's newest commit.** The commits exist on the
branch the tree was reset away from; `--all` reaches them:

```
git log --all --format="%h %ad %s" --date=format:%H:%M -- <path>
```

For `crates/goya-caprichios/src/registry.rs` this lists the daemon's whole
commit sequence for the day (newest first: `831b99b 21:48`, `3099b64 21:31`,
… `06c9259 20:50` …), which identifies both the pre-change and post-change
commits for step (c).

**(b) Restore solo files wholesale.** For files only one workstream touched,
check out the newest commit that contains them:

```
git checkout <commit> -- <path>
```

Applied to `bluetooth_access_caprichio.rs`, `bluetooth_profile_caprichio.rs`,
`crates/neptune/src/bluetooth.rs` and `docs/blue-moon/BLUE-MOON-{6,7,8}.md`,
taken from `sash/phase0-smoke` (`f8f6d12` and predecessors). The result was
committed as `3cb34e3` (21:00:47); the restored files are byte-identical to
the 20:53:36 `~/.local` snapshot, which cross-checks the restore.

**(c) Merge shared files with a diff and a 3-way apply.** `registry.rs`
carried both the Blue Moon wiring and the other agent's SSO/accelerator
wiring; a wholesale checkout of one side would have deleted the other's
arms. Instead, diff the pre-change commit against the post-change commit and
re-apply onto the current tree:

```
git diff <before> <after> -- <paths> > /tmp/wiring.diff
git apply --3way /tmp/wiring.diff
```

Here `<before>` = `8b818b1` (last `main` commit before the sweep) and
`<after>` = `f8f6d12` (last `sash/phase0-smoke` commit). The 983-line diff
carries both workstreams' added wiring (118 added Bluetooth-related lines,
217 added SSO/accelerator lines); `--3way` merges it onto the current tree
using the git blob identities as the merge base, so both sets of registry
arms coexist — verified in the recovered tree, where `registry.rs` carries
the Blue Moon and accelerator wiring together.

**(d) Strip references to files that were never committed.** Files destroyed
in the ~18:59 sweep before their first commit have no commits and no
snapshot; no amount of history digging recovers them. If the merged wiring
still references such a file, remove the reference rather than inventing the
file: strip the `use`/declaration/match arm together with its balanced
braces and trailing comma (a brace-aware remover, not a line grep), so the
remainder still parses, then let the owning workstream re-wire from its own
session records. A stash is not a substitute: stash commits are not reached
by `git log --all` and are easy to lose.

**(e) Re-add lost dependency lines.** `Cargo.toml` edits are swept with the
tree like any other file. Verified case: the Neptune dependency line was
committed on `sash/phase0-smoke` at 20:44 (`f7393fc`), lost from disk at
20:56:05, and re-added on `main` at 21:01:44 (`a53a0b0`,
`goya-caprichios.workspace = true`). Sweep every manifest the lost work
touched, not just source files.

Finish by re-running the test suite and diffing the restored files against
the newest `~/.local/backup/blue-moon-*` snapshot before committing the
reconciliation.

## Why .local backups

Off-repo snapshot copies under `~/.local/backup/blue-moon-*` are taken at
each phase completion — outside `.git`, where resets, cleans, branch
switches and history rewrites cannot touch them. The day's five snapshots:

| Snapshot | Taken at | Contents |
| --- | --- | --- |
| `blue-moon-20261118-191734` | 19:17:34, immediately after the ~18:59-sweep rebuild | Rebuilt sources (`bluetooth_caprichio/control/runtime/stream`) + `BLUE-MOON-1/2/3.md`, roadmap |
| `blue-moon-20261118-195323` | 19:53:23, phase 3 complete | Guard + Continuity sources, `BLUE-MOON-1..5.md` |
| `blue-moon-20261118-203548` | 20:35:48, phase 4 complete | Access + Profile sources, `BLUE-MOON-1..7.md` |
| `blue-moon-20261118-205336` | 20:53:36, BLUE-MOON-8 complete — 2.5 minutes before the sweep | Full surface: all seven caprichio sources + runtime + Neptune `bluetooth.rs`/`service.rs`, `BLUE-MOON-1..8.md`, roadmap |
| `blue-moon-20261118-214953` | 21:49:53, post-recovery | Includes the merged `registry.rs` and `lib.rs` |

When they were used: the first snapshot exists *because* of the ~18:59
sweep — no backup existed before it, which is why that loss was total. The
20:53:36 snapshot bounded the second incident: it preserved the complete
pre-sweep state off-repo and confirmed, byte for byte, that the git-history
restore of step (b) reproduced exactly what was lost. Git history was the
recovery path; the backup was the proof it was complete.

## Recommendations

- **Keep untracked work short-lived.** Any file that exists longer than one
  daemon commit cycle without a commit is one sweep from oblivion. The
  ~18:59 loss was unrecoverable for exactly this reason.
- **Snapshot after each green suite.** The `~/.local/backup/blue-moon-*`
  habit cost seconds per phase and bounded the second incident's blast
  radius; it is the only mechanism that survives a hostile tree operation.
- **Pause auto-commit daemons during multi-agent sessions.** Two workstreams
  sharing one checkout, plus a daemon committing whatever branch is checked
  out, produced interleaved history on `sash/phase0-smoke` and set up both
  sweeps. Suspend the daemon, or give each workstream its own worktree.
- **Make daemon commits include new files atomically.** A daemon cycle
  should stage and commit new files in the same atomic pass as modifications
  (`git add -A` + commit), so no file waits uncommitted between cycles — and
  no tree operation may run in that window.
- **Never `git reset --hard` in a shared tree.** Both sweeps were
  reset/checkout operations executed in a checkout other agents were using.
  The 20:56:05 reset was harmless to `sash/phase0-smoke` (everything was
  committed) and destructive to the tree on disk. Coordinate resets, or move
  the work to separate worktrees as the programme now does.
