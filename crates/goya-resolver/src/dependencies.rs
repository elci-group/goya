//! Display dependencies recorded once, during resolution.
//!
//! This index is built once. A later write looks up sites for the written
//! keys. Layout is a pure function of the scene and the viewport and is not
//! rerun from here. This module does not claim the geometry walk inside
//! `goya_lang::layout` is incremental.

use std::collections::{BTreeMap, BTreeSet};

use goya_lang::display_binding;
use goya_lang::{InteractionValue, SceneCell};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DisplaySite {
    pub element_id: Option<String>,
    pub state_key: String,
    pub context: String,
    pub order: u32,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DisplayIndex {
    sites_by_key: BTreeMap<String, Vec<DisplaySite>>,
    element_ids: BTreeMap<String, Vec<String>>,
}

impl DisplayIndex {
    /// Authored ids of sites bound to `key`, in document order. Empty when the key has no site.
    pub(crate) fn element_ids(&self, key: &str) -> &[String] {
        self.element_ids.get(key).map(Vec::as_slice).unwrap_or(&[])
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Propagation {
    pub affected_elements: Vec<String>,
    pub announcement: Option<String>,
    pub sites_evaluated: usize,
}

pub(crate) fn display_index(root: &SceneCell) -> DisplayIndex {
    let mut sites_by_key: BTreeMap<String, Vec<DisplaySite>> = BTreeMap::new();
    let mut element_ids: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut pending = vec![root];
    let mut order = 0u32;
    while let Some(cell) = pending.pop() {
        if let Some(key) = cell.text_from.as_deref() {
            let state_key = key.to_owned();
            let element_id = cell.authored_id.clone();
            if let Some(id) = element_id.as_deref().filter(|id| !id.is_empty()) {
                element_ids
                    .entry(state_key.clone())
                    .or_default()
                    .push(id.to_owned());
            }
            sites_by_key
                .entry(state_key.clone())
                .or_default()
                .push(DisplaySite {
                    element_id,
                    state_key,
                    context: display_binding::context(cell.text.as_deref(), key),
                    order,
                });
            order += 1;
        }
        pending.extend(cell.children.iter().rev());
    }
    DisplayIndex {
        sites_by_key,
        element_ids,
    }
}

pub(crate) fn propagate(
    index: &DisplayIndex,
    writes: &[String],
    before: &BTreeMap<String, InteractionValue>,
    after: &BTreeMap<String, InteractionValue>,
) -> Propagation {
    let mut seen_keys = BTreeSet::new();
    let mut selected: Vec<(&DisplaySite, &InteractionValue, &InteractionValue)> = Vec::new();
    for key in writes {
        if !seen_keys.insert(key.as_str()) {
            continue;
        }
        let (Some(old), Some(new)) = (before.get(key), after.get(key)) else {
            continue;
        };
        if old == new {
            continue;
        }
        let Some(sites) = index.sites_by_key.get(key) else {
            continue;
        };
        for site in sites {
            selected.push((site, old, new));
        }
    }
    selected.sort_by_key(|(site, _, _)| site.order);

    let mut affected_elements = Vec::new();
    let mut seen_ids = BTreeSet::new();
    for (site, _, _) in &selected {
        let Some(id) = site.element_id.as_deref().filter(|id| !id.is_empty()) else {
            continue;
        };
        if seen_ids.insert(id) {
            affected_elements.push(id.to_owned());
        }
    }
    let announcement = display_binding::announcement(
        selected
            .iter()
            .map(|(site, old, new)| (site.context.as_str(), *old, *new)),
    );
    Propagation {
        affected_elements,
        announcement,
        sites_evaluated: selected.len(),
    }
}
