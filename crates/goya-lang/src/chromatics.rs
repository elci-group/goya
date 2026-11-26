//! Draft static validation for `@chromatics` declarations (directive
//! GOYA-INT-CHR-001 §12.3, contract GY-CHROMATICS/0.1 draft).
//!
//! Quarantine surface like [`crate::intents`]: the profile it describes is
//! **proposed, not ratified**, every strict admission refuses the block
//! (`GY-E1020`), and nothing here admits a target, lowering or release.
//! Validation is the compile-time half of the §12.3 list; the runtime half
//! is `goya_chromatics::generation::verify`. Validation never mutates the
//! scene and is deterministic on all inputs.
//!
//! Enforced where statically determinable:
//! - a candidate's intent names a declared `@interaction` (the admitted,
//!   registered unit of behaviour — not the experimental `@intent`);
//! - duplicate explicit slot reservations are refused (two candidates
//!   pinned to one abstract slot);
//! - duplicate entities across candidates are refused (the engine would
//!   refuse the second binding at runtime anyway);
//! - sensitive, destructive and privileged candidates must declare
//!   `confirm: true` (§20: a shortcut may enter a confirmation workflow,
//!   never bypass one);
//! - authored `probability` stays in 0..=1000 per mille and `cost` is at
//!   least 1, so the §9 utility terms mean what the model says.
//!
//! Not statically representable in this grammar revision, and therefore not
//! checked here: capability requirements (the target negotiates capacity)
//! and scope inheritance (scenes have no nested view constructs yet).

use crate::parser::{ParsedScene, SourceRange};
use crate::scene::ChromaticsDecl;
use std::collections::BTreeSet;

/// One failed chromatics invariant. Codes sit in the GY-E10xx series:
/// `GY-E1021` covers every chromatics semantic failure, with `name`
/// carrying the specific cause and `range` pointing at the offending
/// declaration span. (`GY-E1020` is strict admission refusing the block at
/// all; see [`crate::parser`].)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChromaticDiagnostic {
    pub code: &'static str,
    pub name: String,
    pub range: SourceRange,
}

impl std::fmt::Display for ChromaticDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: invalid chromatics declaration: {}",
            self.code, self.name
        )
    }
}

impl std::error::Error for ChromaticDiagnostic {}

/// Validate the scene's `@chromatics` declaration against its declared
/// `@interaction`s. Returns all failures in source order; an empty result
/// means the declaration satisfies the draft invariants. Scenes without a
/// declaration validate trivially.
pub fn validate_chromatics(parsed: &ParsedScene) -> Vec<ChromaticDiagnostic> {
    let Some(decl) = &parsed.scene.chromatics else {
        return Vec::new();
    };
    let interactions: BTreeSet<&str> = parsed
        .scene
        .interactions
        .iter()
        .map(|interaction| interaction.name.as_str())
        .collect();
    validate_decl(decl, &interactions)
}

/// Validate one declaration against a set of declared interaction names.
pub fn validate_decl(
    decl: &ChromaticsDecl,
    interactions: &BTreeSet<&str>,
) -> Vec<ChromaticDiagnostic> {
    let mut out = Vec::new();
    let mut pinned_by: BTreeSet<&str> = BTreeSet::new();
    let mut entities: BTreeSet<&str> = BTreeSet::new();
    for candidate in &decl.candidates {
        if let Some(slot) = &candidate.pinned {
            if !pinned_by.insert(slot) {
                out.push(diagnostic(
                    format!(
                        "candidate {} pins slot {} a second time; a slot has at most one reservation",
                        candidate.name, slot
                    ),
                    candidate.range,
                ));
            }
        }
        if let Some(entity) = &candidate.entity {
            if !entities.insert(entity) {
                out.push(diagnostic(
                    format!(
                        "candidate {} names entity {} another candidate already carries",
                        candidate.name, entity
                    ),
                    candidate.range,
                ));
            }
        }
        if !interactions.contains(candidate.intent.as_str()) {
            out.push(diagnostic(
                format!(
                    "candidate {} names interaction {} which the scene does not declare",
                    candidate.name, candidate.intent
                ),
                candidate.range,
            ));
        }
        if candidate.risk.needs_confirmation() && !candidate.confirmation {
            out.push(diagnostic(
                format!(
                    "candidate {} is {} and must declare confirm: true; a shortcut may enter a confirmation workflow, never bypass one",
                    candidate.name,
                    candidate.risk.as_str()
                ),
                candidate.range,
            ));
        }
        if candidate.probability.is_some_and(|p| p > 1000) {
            out.push(diagnostic(
                format!(
                    "candidate {} declares probability {} outside 0..=1000 per mille",
                    candidate.name,
                    candidate.probability.unwrap_or(0)
                ),
                candidate.range,
            ));
        }
        if candidate.cost.is_some_and(|c| c < 1) {
            out.push(diagnostic(
                format!(
                    "candidate {} declares cost 0; the conventional cost of reaching and activating an action is at least one input",
                    candidate.name
                ),
                candidate.range,
            ));
        }
    }
    out
}

fn diagnostic(name: String, range: SourceRange) -> ChromaticDiagnostic {
    ChromaticDiagnostic {
        code: "GY-E1021",
        name,
        range,
    }
}

/// Canonical lowering of a declaration to the engine's allocation policy
/// (§12: the developer authors one declaration; every compiler and resolver
/// lowers it the same way). Targets supply the capability table and the
/// registered intent set themselves — capacity is negotiated, never
/// assumed (§13.1).
pub fn allocation_policy(decl: &ChromaticsDecl) -> goya_chromatics::Policy {
    let mut policy = goya_chromatics::Policy::new(decl.primary);
    policy.fallback = decl.fallback;
    policy.preserve_slots = decl.preserve_slots;
    policy.ordered_slots = decl.ordered_slots;
    policy.exclude_native = decl.exclude_native;
    policy.exclude_duplicates = decl.exclude_duplicates;
    policy.stability = decl.stability;
    policy
}

/// Canonical lowering of the declared candidates to the engine's candidate
/// model. Authored numbers pass through unchanged; the fields that belong
/// to runtime state (`last_used`, `uses`, `expires`) start neutral, because
/// a scene cannot know them.
pub fn allocation_candidates(decl: &ChromaticsDecl) -> Vec<goya_chromatics::Candidate> {
    decl.candidates
        .iter()
        .map(|c| {
            let mut candidate = goya_chromatics::Candidate::new(&c.name, &c.intent, &c.label);
            candidate.entity = c.entity.clone();
            candidate.symbol = c.symbol.clone();
            candidate.description = c.description.clone();
            candidate.risk = c.risk;
            candidate.confirmation = c.confirmation;
            candidate.native = c.native.clone();
            candidate.rank = c.rank;
            candidate.favourite = c.favourite;
            candidate.position = c.position;
            candidate.cost = c.cost.unwrap_or(1);
            candidate.probability = c.probability.unwrap_or(0);
            candidate.contextual = c.contextual.unwrap_or(0);
            candidate.urgency = c.urgency.unwrap_or(0);
            candidate.preference = c.preference.unwrap_or(0);
            candidate.pinned = c.pinned.clone();
            candidate
        })
        .collect()
}
