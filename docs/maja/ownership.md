# Wave 0 ownership and integration

Integration authority owns `Cargo.toml`, `Cargo.lock`, root CI, canonical Goya crates, cross-track contracts and merge sequencing. No shared Goya modification is approved by this document. Existing checkout has unrelated changes and must not be reset, overwritten or incorporated into an incidental patch.

The path ownership in [work packages](work-packages.md) is reserved, not a claim that eight isolated worktrees or active workers already exist. Before implementation, each worker receives a distinct branch/worktree and package ID, verifies its base revision and clean status, acquires a bounded lease for mutation, and records contract dependencies. Overlapping paths and shared changes require an integration-owned change record and approval before editing. Independent qualification owns its evidence and does not certify its own implementation. The current read-only reconnaissance workers did not modify shared contracts.

Wave 0 integration decision: freeze the conservative fail-closed policy and source-handoff boundary for planning and independently testable skeletons. The full M0 exit gate remains open while the concrete supported semantic profile, Frampton interface and independent evidence are unavailable. No M1 browser or application claim follows from this planning freeze.
