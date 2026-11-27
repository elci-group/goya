//! The cascade: origin, importance, layers, specificity, source order
//! (directive §7.5.4). For each element, declarations from matching
//! selectors compete; the winner per property is deterministic.

use crate::cascade::conditional::{
    evaluate_media, evaluate_supports, report_unevaluable, Environment,
};
use crate::context::selector_matching::ElementRef;
use crate::context::selector_matching::{matches, Compiled, SelectorCache};
use crate::diagnostics::codes;
use crate::diagnostics::reporter::Reporter;
use crate::ir::casa_ir::{CasaIr, CasaRule, LayerRule, StyleRule};
use std::collections::BTreeMap;

/// Cascade origin. CasaIR currently holds author sheets only; the other
/// origins exist so the sort key is already correct when they arrive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    UserAgent,
    User,
    Author,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Importance {
    Normal,
    Important,
}

/// One competing declaration for one element.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Index of the sheet the declaration came from.
    pub sheet: usize,
    /// Index of the rule within its sheet, in source order. Rules nested
    /// inside group rules share the group's index plus their position.
    pub rule: usize,
    pub origin: Origin,
    pub importance: Importance,
    /// Layer index by first-encounter order; `None` is unlayered.
    pub layer: Option<usize>,
    pub specificity: u32,
    pub name: String,
    pub value: String,
    /// The selector text that matched.
    pub selector: String,
}

/// The winning declaration for one property on one element.
#[derive(Debug, Clone, PartialEq)]
pub struct Winner {
    pub value: String,
    pub important: bool,
    pub specificity: u32,
    /// Sheet/rule order, for diagnostics and provenance.
    pub order: usize,
    pub selector: String,
}

/// Cascade order key: (origin, importance, layer, specificity). Equal
/// keys are broken by source order — later wins — which falls out of
/// candidate iteration order (sheet-major, rule-minor).
type CascadeKey = (u8, u8, usize, u32);

impl Candidate {
    pub fn cascade_key(&self) -> CascadeKey {
        // Origin score encodes the CSS cascade order:
        //   UA normal < user normal < author normal
        //   < author important < user important < UA important
        let origin_score = match (self.origin, self.importance) {
            (Origin::UserAgent, Importance::Normal) => 0,
            (Origin::User, Importance::Normal) => 1,
            (Origin::Author, Importance::Normal) => 2,
            (Origin::Author, Importance::Important) => 3,
            (Origin::User, Importance::Important) => 4,
            (Origin::UserAgent, Importance::Important) => 5,
        };
        // Layers: unlayered normal declarations beat layered ones;
        // important declarations reverse the layer order (CSS Cascading
        // Level 5 §7.1).
        let layer_score = match (self.layer, self.importance) {
            (None, Importance::Normal) => usize::MAX,
            (Some(index), Importance::Normal) => index,
            (Some(index), Importance::Important) => usize::MAX - index,
            (None, Importance::Important) => 0,
        };
        (
            origin_score,
            self.importance as u8,
            layer_score,
            self.specificity,
        )
    }
}

/// Resolve the winner per property from competing candidates. Candidates
/// must be iterated in source order (sheet-major, rule-minor).
pub fn resolve(candidates: Vec<Candidate>) -> BTreeMap<String, Winner> {
    let mut best: BTreeMap<String, (CascadeKey, Candidate)> = BTreeMap::new();
    for candidate in candidates {
        let key = candidate.cascade_key();
        match best.get_mut(&candidate.name) {
            Some((incumbent_key, incumbent)) => {
                // Equal keys: this candidate is later in source order, so
                // it wins; a strictly greater key wins regardless.
                if key >= *incumbent_key {
                    *incumbent_key = key;
                    *incumbent = candidate;
                }
            }
            None => {
                best.insert(candidate.name.clone(), (key, candidate));
            }
        }
    }
    best.into_iter()
        .map(|(name, (_, candidate))| {
            let winner = Winner {
                value: candidate.value,
                important: candidate.importance == Importance::Important,
                specificity: candidate.specificity,
                order: candidate.sheet.wrapping_mul(1_000_000) + candidate.rule,
                selector: candidate.selector,
            };
            (name, winner)
        })
        .collect()
}

/// Compiled cascade state for one IR: layer order and selector cache.
#[derive(Debug, Default)]
pub struct CascadeEngine {
    layers: Vec<String>,
    selectors: SelectorCache,
}

impl CascadeEngine {
    /// Register layer names in first-encounter order (`@layer a, b;`
    /// statements first, then block layers, depth-first per sheet).
    pub fn new(ir: &CasaIr) -> Self {
        let mut engine = CascadeEngine::default();
        for sheet in &ir.sheets {
            engine.register_layers(&sheet.rules);
        }
        engine
    }

    fn register_layers(&mut self, rules: &[CasaRule]) {
        for rule in rules {
            if let CasaRule::Layer(LayerRule { names, rules, .. }) = rule {
                for name in names {
                    if !name.is_empty() && !self.layers.contains(name) {
                        self.layers.push(name.clone());
                    }
                }
                self.register_layers(rules);
            }
        }
    }

    pub fn layer_index(&self, name: Option<&str>) -> Option<usize> {
        name.and_then(|name| self.layers.iter().position(|known| known == name))
    }

    pub fn layers(&self) -> &[String] {
        &self.layers
    }

    /// All declarations matching one element, with their cascade keys.
    /// Iteration order is sheet-major, rule-minor, source order within
    /// group rules.
    pub fn candidates(
        &mut self,
        ir: &CasaIr,
        element: &ElementRef<'_>,
        environment: &Environment,
        reporter: &mut Reporter,
    ) -> Vec<Candidate> {
        let mut candidates = Vec::new();
        for (sheet_index, sheet) in ir.sheets.iter().enumerate() {
            for (rule_index, rule) in sheet.rules.iter().enumerate() {
                self.collect_rule(
                    sheet_index,
                    rule_index,
                    rule,
                    None,
                    element,
                    environment,
                    reporter,
                    &mut candidates,
                );
            }
        }
        candidates
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_rule(
        &mut self,
        sheet_index: usize,
        rule_index: usize,
        rule: &CasaRule,
        layer: Option<&str>,
        element: &ElementRef<'_>,
        environment: &Environment,
        reporter: &mut Reporter,
        candidates: &mut Vec<Candidate>,
    ) {
        match rule {
            CasaRule::Style(style) => {
                for selector in &style.selectors {
                    self.match_selector(
                        sheet_index,
                        rule_index,
                        style,
                        selector,
                        self.layer_index(layer),
                        element,
                        candidates,
                        reporter,
                    );
                }
            }
            CasaRule::Media(media) => {
                self.collect_group(
                    sheet_index,
                    rule_index,
                    &media.rules,
                    layer,
                    element,
                    environment,
                    reporter,
                    candidates,
                    evaluate_media(&media.condition, environment)
                        .map_err(|error| ("@media", format!("{}: {error}", media.condition))),
                );
            }
            CasaRule::Supports(supports) => {
                self.collect_group(
                    sheet_index,
                    rule_index,
                    &supports.rules,
                    layer,
                    element,
                    environment,
                    reporter,
                    candidates,
                    evaluate_supports(&supports.condition)
                        .map_err(|error| ("@supports", format!("{}: {error}", supports.condition))),
                );
            }
            CasaRule::Layer(layer_rule) => {
                let inner = layer_rule.names.first().map(|name| name.as_str());
                for (nested_index, nested) in layer_rule.rules.iter().enumerate() {
                    self.collect_rule(
                        sheet_index,
                        rule_index + nested_index,
                        nested,
                        inner,
                        element,
                        environment,
                        reporter,
                        candidates,
                    );
                }
            }
            _ => {}
        }
    }

    /// Evaluate a group rule's condition, then descend into its body.
    #[allow(clippy::too_many_arguments)]
    fn collect_group(
        &mut self,
        sheet_index: usize,
        rule_index: usize,
        rules: &[CasaRule],
        layer: Option<&str>,
        element: &ElementRef<'_>,
        environment: &Environment,
        reporter: &mut Reporter,
        candidates: &mut Vec<Candidate>,
        evaluated: Result<bool, (&str, String)>,
    ) {
        match evaluated {
            Ok(true) => {
                for (nested_index, nested) in rules.iter().enumerate() {
                    self.collect_rule(
                        sheet_index,
                        rule_index + nested_index,
                        nested,
                        layer,
                        element,
                        environment,
                        reporter,
                        candidates,
                    );
                }
            }
            Ok(false) => {}
            Err((kind, reason)) => report_unevaluable(reporter, kind, &reason),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn match_selector(
        &mut self,
        sheet_index: usize,
        rule_index: usize,
        style: &StyleRule,
        selector: &crate::ir::casa_ir::Selector,
        layer: Option<usize>,
        element: &ElementRef<'_>,
        candidates: &mut Vec<Candidate>,
        reporter: &mut Reporter,
    ) {
        let compiled: &Compiled = match self.selectors.get(&selector.text) {
            Ok(compiled) => compiled,
            Err(reason) => {
                reporter.emit(
                    codes::SELECTOR_UNMATCHABLE,
                    format!("selector not matchable, rule excluded: {reason}"),
                    None,
                );
                return;
            }
        };
        if !matches(compiled, element) {
            return;
        }
        for declaration in &style.declarations {
            candidates.push(Candidate {
                sheet: sheet_index,
                rule: rule_index,
                origin: Origin::Author,
                importance: if declaration.important {
                    Importance::Important
                } else {
                    Importance::Normal
                },
                layer,
                specificity: compiled.specificity,
                name: declaration.name.clone(),
                value: declaration.value.clone(),
                selector: selector.text.clone(),
            });
        }
    }
}

/// Convenience: the cascade result for one element.
pub fn cascade_element(
    engine: &mut CascadeEngine,
    ir: &CasaIr,
    element: &ElementRef<'_>,
    environment: &Environment,
    reporter: &mut Reporter,
) -> BTreeMap<String, Winner> {
    let candidates = engine.candidates(ir, element, environment, reporter);
    resolve(candidates)
}
