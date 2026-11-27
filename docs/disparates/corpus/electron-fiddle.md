# Corpus nomination: `electron/fiddle` (phase 2)

**Nomination (2026-11-19): `electron/fiddle`, whole-repository app slice at tag
`v0.4.0`, MIT, pinned at commit `82fb8290c386aac5654ce5fdb4eb4a6c7faa1654`**
(corpus location `/home/sal/corpus/fiddle`; extraction evidence in
`evidence/disparates/electron-fiddle-phase2/`).

Nominated as the second, more complex reference application: 19 declared IPC
event channels (Reactotron: 5), a main-process IPC manager wrapper, template
fiddle content shipped as static data, and roughly twice the app-slice source
of Reactotron — while remaining bounded and single-window.

## Criteria evidence

| # | Criterion | Evidence at pinned commit |
|---|-----------|---------------------------|
| 1 | Single window; no native modules | 1 `new BrowserWindow(` site in `src/main/windows.ts` (two further sites in `src/content/*.ts` are fiddle template text, not app windows); no `.node` requires anywhere in `src/`; dependency list contains no native addons (`fs-extra`, `chokidar` are pure JS) |
| 2 | Bounded IPC; dynamic requires expected to surface | 19 channels declared in `src/ipc-events.ts` (`IpcEvents` enum); one `ipcMainManager` wrapper in `src/main/ipc.ts`; 29 computed `require()`s surfaced as `undetermined`/`DYNAMIC_REQUIRE_UNRESOLVED`, as anticipated by the criteria |
| 3 | Permissive license | `LICENSE.md`: MIT (GitHub Inc.) |
| 4 | Reproducible from pinned commit | Pinned to tag `v0.4.0` commit `82fb8290…`; extraction reads source bytes only, never executes |

App-slice size: ~7,000 lines of TypeScript across `src/` (254 source files
hashed in the FAIR source manifest, including `static/templates/` — see
honesty notes in `run.json`).

## Candidates passed over

| Candidate | Rejection reason |
|---|---|
| `electron/fiddle` at current HEAD (`c1fb6f50…`, monorepo) | App slice ~75,000 lines (`packages/app/src` alone: main 32.5k, renderer 16.1k); exceeds the bounded-size criterion. IPC is mediated by a code-generated framework (`@marshallofsound/ipc` runtime under `src/ipc/generated/`), which the line-oriented extractor cannot attribute honestly. The `v0.4.0` tag retains the same application character at a bounded size, so the historical pin was chosen instead. |
| `marktext/marktext` | Contains native addon dependencies (native build toolchain in dependencies); fails criterion 1. Not inspected further per nomination guidance. |
| Boost Note-style apps (BoostIO) | Repository is a large multi-package monorepo with cloud-service coupling; no bounded single-window slice identifiable without arbitrary exclusion. |

## Scan-scope finding recorded for the programme

`static/templates/**` (and `src/content/*.ts`) are Electron API example fiddles
shipped as application *data*: they are displayed and run inside the fiddle
sandbox, never executed as Electron Fiddle's own process code. The extractor
has no exclusion mechanism, so template constructs are extracted alongside app
constructs (notably `window_lifecycle` and `ipc_channel` elements from
templates). This is preserved rather than silently dropped, per the
FAIR-preserve-ignorance rule, and declared in the run's honesty notes. A future
extractor revision would need an explicit, recorded scan-scope mechanism
(e.g. manifest-declared data directories) rather than ad-hoc path exclusions.
