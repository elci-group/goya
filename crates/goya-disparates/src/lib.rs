//! GY-DISP-003 — translation orchestration for the Disparates programme.
//!
//! A thin, deterministic pipeline driver: the full extraction → assessment →
//! report flow as one API ([`Pipeline::run`]) and one CLI (`disparate run`).
//! The orchestrator adds no analysis policy of its own — the capability map is
//! passed through unchanged, extraction stays GY-DISP-001's, assessment stays
//! GY-COVERAGE/1's. Deterministic: identical inputs and map yield
//! byte-identical canonical output. Foreign code is never executed; no LLM
//! participates.

use goya_disparate_coverage::{CapabilityMap, DeficitReport};
use goya_fair::{FairDocument, RepresentabilityStatus};
use std::path::Path;

/// Run failure modes, wrapping the underlying pipeline stages' errors.
#[derive(Debug)]
pub enum RunError {
    /// Extraction (GY-DISP-001) failed.
    Extraction(goya_disparate_electron::ExtractionError),
    /// Assessment or report derivation (GY-COVERAGE/1) failed.
    Assessment(String),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Extraction(e) => write!(f, "extraction failed: {e}"),
            Self::Assessment(m) => write!(f, "assessment failed: {m}"),
        }
    }
}

impl std::error::Error for RunError {}

/// Output of one pipeline run, in pipeline order.
#[derive(Debug, Clone)]
pub struct RunOutput {
    /// Extracted FAIR document (pre-assessment), as produced by GY-DISP-001.
    pub fair: FairDocument,
    /// The same document after GY-COVERAGE/1 assessment.
    pub assessed: FairDocument,
    /// Derived capability-deficit report over the assessed document.
    pub deficit: DeficitReport,
    /// Elements that entered assessment as `extracted` and left with a
    /// classification (rule match, or the `NO_COVERAGE_RULE` fall-through).
    pub upgraded: usize,
    /// Elements that already carried a classification and were preserved
    /// unchanged (`upgraded + untouched` equals the element count).
    pub untouched: usize,
}

/// The Disparates pipeline: extraction → assessment → deficit reporting.
#[derive(Debug, Clone)]
pub struct Pipeline {
    map: CapabilityMap,
}

impl Pipeline {
    /// A pipeline over the default GY-COVERAGE/1 v1 capability map.
    pub fn default_map() -> Self {
        Self {
            map: goya_disparate_coverage::default_map(),
        }
    }

    /// A pipeline over a caller-supplied capability map. The map is data:
    /// assessment policy lives in the map, not in this driver.
    pub fn with_map(map: CapabilityMap) -> Self {
        Self { map }
    }

    /// Run the full pipeline over the application rooted at `app_root`:
    /// extraction (GY-DISP-001), assessment (GY-COVERAGE/1), report
    /// derivation, in that order. Deterministic; foreign code is never
    /// executed.
    pub fn run(&self, app_root: &Path) -> Result<RunOutput, RunError> {
        let fair = goya_disparate_electron::extract_app(app_root)
            .map_err(RunError::Extraction)?;
        let assessment = goya_disparate_coverage::assess(&fair, &self.map)
            .map_err(RunError::Assessment)?;
        let deficit = DeficitReport::derive(&assessment.document);
        // Counter bookkeeping is derived here, from pre-assessment statuses:
        // preserved classifications are `untouched`, everything that entered
        // assessment as `extracted` is `upgraded`. This keeps the exact
        // partition invariant even where the underlying counters overlap (the
        // `NO_COVERAGE_RULE` fall-through is both an upgrade and an
        // unmatched-element case).
        let untouched = fair
            .elements
            .iter()
            .filter(|e| e.representability.status != RepresentabilityStatus::Extracted)
            .count();
        let upgraded = fair.elements.len() - untouched;
        Ok(RunOutput {
            fair,
            assessed: assessment.document,
            deficit,
            upgraded,
            untouched,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The existing extractor fixture app, referenced from this crate's
    /// manifest directory.
    fn fixture_app() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../goya-disparate-electron/fixtures/sample-electron-app")
    }

    fn canonical_triple(output: &RunOutput) -> String {
        format!(
            "{}{}{}",
            output.fair.canonical_json(),
            output.assessed.canonical_json(),
            output.deficit.canonical_json()
        )
    }

    #[test]
    fn end_to_end_run_over_fixture() {
        let output = Pipeline::default_map().run(&fixture_app()).expect("run succeeds");
        assert_eq!(output.fair.validate(), Ok(()));
        assert_eq!(output.assessed.validate(), Ok(()));
        assert_eq!(output.deficit.elements, output.fair.elements.len());
        assert_eq!(output.deficit.contract, goya_disparate_coverage::COVERAGE_CONTRACT);
    }

    #[test]
    fn totals_sum_to_element_count() {
        let output = Pipeline::default_map().run(&fixture_app()).unwrap();
        let summed: usize = output.deficit.totals.values().sum();
        assert_eq!(summed, output.deficit.elements);
        assert_eq!(summed, output.fair.elements.len());
        assert_eq!(summed, output.assessed.elements.len());
    }

    #[test]
    fn upgraded_plus_untouched_equals_element_count() {
        let output = Pipeline::default_map().run(&fixture_app()).unwrap();
        assert_eq!(
            output.upgraded + output.untouched,
            output.fair.elements.len()
        );
    }

    #[test]
    fn run_is_deterministic() {
        let a = Pipeline::default_map().run(&fixture_app()).unwrap();
        let b = Pipeline::default_map().run(&fixture_app()).unwrap();
        assert_eq!(canonical_triple(&a), canonical_triple(&b));
    }

    #[test]
    fn custom_map_round_trips_through_json() {
        let json = serde_json::to_string(&goya_disparate_coverage::default_map())
            .expect("map serialises");
        let map = CapabilityMap::from_json(&json).expect("map round-trips");
        let output = Pipeline::with_map(map).run(&fixture_app()).unwrap();
        assert_eq!(output.fair.validate(), Ok(()));
        assert_eq!(output.assessed.validate(), Ok(()));
        assert_eq!(
            output.upgraded + output.untouched,
            output.fair.elements.len()
        );
    }

    #[test]
    fn missing_manifest_is_reported_as_extraction_error() {
        let empty = std::env::temp_dir().join(format!(
            "goya-disparates-empty-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&empty).unwrap();
        let err = Pipeline::default_map().run(&empty).expect_err("must fail");
        std::fs::remove_dir(&empty).ok();
        assert!(matches!(err, RunError::Extraction(_)));
        let displayed = err.to_string();
        assert!(displayed.starts_with("extraction failed:"), "{displayed}");
    }
}
