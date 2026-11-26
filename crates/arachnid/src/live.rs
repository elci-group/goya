//! Live scenes on webOS: presence configurations, interaction lowering and
//! the host contract. Contract `arachnid.live/0.0.1`.
//!
//! GSSH/3 live admission (`goya_lang::parse_strict_gssh3_live`) adds
//! `text-from`, `image-from`, `present-when` and `host` state. This module
//! turns them into things Arachnid can check at compile time and lower.
//!
//! **Presence configurations.** A node is present while every
//! `present-when` (`is` or `at-least`) on it and its ancestors holds.
//! Each condition key is partitioned into parts whose values answer every
//! condition alike, with one representative value per part. Every
//! combination of representatives is enumerated (up to
//! [`MAX_SIGNATURES`]) and de-duplicated by the set of present nodes, up to
//! [`MAX_CONFIGURATIONS`] distinct sets. Each set gets its own focus
//! graph, initial focus, reachability, trap and overlap checks. This is
//! conservative: combinations the app never reaches are still checked.
//! The runtime finds a configuration's focus graph by its set of present
//! buttons.
//!
//! **Interactions.** D1 and D2 programs are compiled and validated by
//! `goya_lang` first. Arachnid then lowers the authored straight-line
//! assignments with the reference VM's semantics:
//! * every read comes from the pre-transition snapshot;
//! * integer addition is checked, and an overflow commits nothing;
//! * all writes commit together.
//!
//! JavaScript numbers are exact only up to 2^53 − 1, so the runtime treats
//! any result beyond that as an overflow, and the build refuses larger
//! literals (`GY-AR-616`).
//!
//! **Platform services.** `host` state and D2 effects are served only by
//! Goya platform services the scene declares, today `@link`
//! (`goya.lanlink/0`, see `lanlink.rs`). There is no escape hatch for
//! app-supplied scripts: an app is its `.gscene`.

use crate::diagnostics::{Code, Diagnostic};
use goya_lang::{
    ConditionOp, EffectValueType, InteractionClass, InteractionDecl, InteractionExpr,
    InteractionType, InteractionValue, ParsedScene, SceneCell, StateLifetime,
};

pub const CONTRACT: &str = "arachnid.live/0.0.1";

/// Upper bound on distinct presence configurations (distinct sets of
/// present nodes), each with its own focus graph.
pub const MAX_CONFIGURATIONS: usize = 1024;
/// Upper bound on enumerated value combinations before de-duplication.
pub const MAX_SIGNATURES: usize = 1_000_000;

/// Largest integer the runtime represents exactly.
pub const MAX_SAFE_INT: i64 = (1 << 53) - 1;

/// One key used by `present-when`, with one representative value per part
/// of its value space. Two values are in the same part when every
/// condition on the key gives the same answer for both.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresenceKey {
    pub key: String,
    pub reps: Vec<InteractionValue>,
}

/// One condition on a node or an ancestor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cond {
    /// Index into [`Presence::keys`].
    pub key: usize,
    pub op: ConditionOp,
    pub value: InteractionValue,
}

impl Cond {
    pub fn holds(&self, v: &InteractionValue) -> bool {
        match (self.op, v, &self.value) {
            (ConditionOp::Is, v, w) => v == w,
            (ConditionOp::AtLeast, InteractionValue::Int(a), InteractionValue::Int(b)) => a >= b,
            (ConditionOp::AtLeast, _, _) => false,
        }
    }
}

/// A combination of representatives: one index into each key's `reps`.
pub type Signature = Vec<usize>;

/// Presence keys and, per preorder node, the conjunction of conditions on
/// it and its ancestors.
#[derive(Clone, Debug, Default)]
pub struct Presence {
    pub keys: Vec<PresenceKey>,
    pub node_conditions: Vec<Vec<Cond>>,
}

impl Presence {
    pub fn from_cells(cells: &[&SceneCell], parents: &[Option<usize>]) -> Self {
        let mut names: Vec<String> = Vec::new();
        let mut node_conditions: Vec<Vec<Cond>> = Vec::with_capacity(cells.len());
        for (i, cell) in cells.iter().enumerate() {
            let mut conds = parents[i].map_or_else(Vec::new, |p| node_conditions[p].clone());
            if let Some(c) = &cell.present_when {
                let k = names.iter().position(|n| *n == c.key).unwrap_or_else(|| {
                    names.push(c.key.clone());
                    names.len() - 1
                });
                conds.push(Cond { key: k, op: c.op, value: c.value.clone() });
            }
            node_conditions.push(conds);
        }
        // Representatives: candidate values around every literal, kept when
        // they answer the key's conditions differently from those kept so far.
        let mut keys = Vec::new();
        for (k, name) in names.iter().enumerate() {
            let conds: Vec<&Cond> = node_conditions.iter().flatten().filter(|c| c.key == k).collect();
            let mut candidates: Vec<InteractionValue> = Vec::new();
            for c in &conds {
                match &c.value {
                    InteractionValue::Int(l) => {
                        for d in [-1i64, 0, 1] {
                            candidates.push(InteractionValue::Int(l.saturating_add(d)));
                        }
                    }
                    InteractionValue::Text(t) => candidates.push(InteractionValue::Text(t.clone())),
                    InteractionValue::Bool(_) => {}
                }
            }
            if conds.iter().any(|c| matches!(c.value, InteractionValue::Bool(_))) {
                candidates.extend([InteractionValue::Bool(true), InteractionValue::Bool(false)]);
            }
            if conds.iter().any(|c| matches!(c.value, InteractionValue::Text(_))) {
                candidates.push(InteractionValue::Text("\u{1}any other value".into()));
            }
            let mut reps: Vec<InteractionValue> = Vec::new();
            let mut seen: Vec<Vec<bool>> = Vec::new();
            for v in candidates {
                let answers: Vec<bool> = conds.iter().map(|c| c.holds(&v)).collect();
                if !seen.contains(&answers) {
                    seen.push(answers);
                    reps.push(v);
                }
            }
            keys.push(PresenceKey { key: name.clone(), reps });
        }
        Presence { keys, node_conditions }
    }

    /// Keep only representatives for `values`, the values key `k` can
    /// actually hold (see [`literal_domain`]): one per distinct answer to
    /// the key's conditions.
    pub fn restrict(&mut self, k: usize, values: &[InteractionValue]) {
        let conds: Vec<Cond> = self.node_conditions.iter().flatten().filter(|c| c.key == k).cloned().collect();
        let mut reps = Vec::new();
        let mut seen: Vec<Vec<bool>> = Vec::new();
        for v in values {
            let answers: Vec<bool> = conds.iter().map(|c| c.holds(v)).collect();
            if !seen.contains(&answers) {
                seen.push(answers);
                reps.push(v.clone());
            }
        }
        if !reps.is_empty() {
            self.keys[k].reps = reps;
        }
    }

    /// Number of value combinations, or `None` past [`MAX_SIGNATURES`].
    pub fn signature_count(&self) -> Option<usize> {
        self.keys
            .iter()
            .try_fold(1usize, |n, k| n.checked_mul(k.reps.len().max(1)).filter(|n| *n <= MAX_SIGNATURES))
    }

    /// Visit every combination in a fixed order (the first key varies slowest).
    pub fn for_each_signature(&self, mut f: impl FnMut(&Signature)) {
        let mut sig = vec![0usize; self.keys.len()];
        loop {
            f(&sig);
            let mut k = self.keys.len();
            loop {
                if k == 0 {
                    return;
                }
                k -= 1;
                sig[k] += 1;
                if sig[k] < self.keys[k].reps.len() {
                    break;
                }
                sig[k] = 0;
            }
        }
    }

    pub fn present(&self, node: usize, sig: &Signature) -> bool {
        self.node_conditions[node]
            .iter()
            .all(|c| c.holds(&self.keys[c.key].reps[sig[c.key]]))
    }

    /// The combination whose representatives answer every condition as
    /// these values do.
    pub fn signature_of(&self, value_of: impl Fn(&str) -> InteractionValue) -> Signature {
        self.keys
            .iter()
            .enumerate()
            .map(|(k, key)| {
                let v = value_of(&key.key);
                let conds: Vec<&Cond> = self.node_conditions.iter().flatten().filter(|c| c.key == k).collect();
                let want: Vec<bool> = conds.iter().map(|c| c.holds(&v)).collect();
                key.reps
                    .iter()
                    .position(|r| conds.iter().map(|c| c.holds(r)).collect::<Vec<_>>() == want)
                    .unwrap_or(0)
            })
            .collect()
    }

    /// Human description such as `link = 5, mode = "WEB", web_rows = 3`.
    pub fn describe(&self, sig: &Signature) -> String {
        if self.keys.is_empty() {
            return "always".into();
        }
        self.keys
            .iter()
            .zip(sig)
            .map(|(k, &r)| format!("{} = {}", k.key, literal_text(&k.reps[r])))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub fn literal_text(v: &InteractionValue) -> String {
    match v {
        InteractionValue::Int(i) => i.to_string(),
        InteractionValue::Text(t) => format!("{t:?}"),
        InteractionValue::Bool(b) => b.to_string(),
    }
}

/// One declared state key as the runtime sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateKey {
    pub key: String,
    pub value_type: InteractionType,
    pub initial: InteractionValue,
    pub host: bool,
}

pub fn state_keys(parsed: &ParsedScene) -> Vec<StateKey> {
    parsed
        .declarations
        .state
        .iter()
        .flat_map(|(decls, _)| decls)
        .map(|d| StateKey {
            key: d.key.clone(),
            value_type: d.value_type,
            initial: d.initial.clone(),
            host: d.lifetime == StateLifetime::Host,
        })
        .collect()
}

/// The values a key can ever hold, when that is a finite list: a
/// `session` key that interactions only assign literals holds its initial
/// value or one of those literals. `None` for `durable` keys (a restored
/// record may hold any value of the type), `host` keys, and keys some
/// interaction computes.
pub fn literal_domain(parsed: &ParsedScene, key: &str) -> Option<Vec<InteractionValue>> {
    let decl = parsed
        .declarations
        .state
        .iter()
        .flat_map(|(decls, _)| decls)
        .find(|d| d.key == key)?;
    if decl.lifetime != StateLifetime::Session {
        return None;
    }
    let mut values = vec![decl.initial.clone()];
    for i in &parsed.scene.interactions {
        for a in i.assignments.iter().filter(|a| a.key == key) {
            match &a.value {
                InteractionExpr::Literal(v) => {
                    if !values.contains(v) {
                        values.push(v.clone());
                    }
                }
                _ => return None,
            }
        }
    }
    Some(values)
}

/// The durable store a scene uses on webOS (decisions D8, D11, D12).
///
/// Without durable state there is none. Otherwise the shared resolver
/// picks the webOS adapter: the default local store
/// `webos.local-storage.v1` (the app's `localStorage`, holding the D8
/// record) when the scene names none, with a warning, or the named one.
/// Arachnid implements only the local store; naming `webos.indexeddb.v1`
/// is refused at the declaration (`GY-AR-651`) rather than substituted.
/// There is no second implemented store, so nothing migrates (D13).
pub fn durable_store(
    parsed: &ParsedScene,
) -> Result<(Option<&'static str>, Vec<String>), Vec<Diagnostic>> {
    use goya_caprichios::state_persistence::{resolve, Declaration, Platform};
    let durable = parsed
        .declarations
        .state
        .iter()
        .flat_map(|(decls, _)| decls)
        .any(|d| d.lifetime == StateLifetime::Durable);
    if !durable {
        return Ok((None, Vec::new()));
    }
    let range = parsed
        .entries
        .iter()
        .find(|e| e.kind == "persistence")
        .map(|e| e.range);
    let declaration = parsed
        .declarations
        .persistence_cache_targets
        .as_ref()
        .map(|targets| Declaration { cache_targets: targets.clone() });
    match resolve(declaration.as_ref(), Platform::Webos) {
        Ok(r) if r.adapter == WEBOS_LOCAL_STORE => {
            let warnings = if r.defaulted {
                vec![format!(
                    "warning: durable state uses the default webOS store {WEBOS_LOCAL_STORE}; name it in @persistence to make the choice explicit (decision D11)"
                )]
            } else {
                Vec::new()
            };
            Ok((Some(WEBOS_LOCAL_STORE), warnings))
        }
        Ok(r) => Err(vec![Diagnostic::new(
            Code::PlatformStoreUnsupported,
            format!("durable store {} is not implemented by Arachnid; use {WEBOS_LOCAL_STORE}", r.adapter),
        )
        .at(range)]),
        Err(e) => Err(vec![Diagnostic::new(
            Code::PlatformStoreUnsupported,
            format!("no webOS durable store: {e:?}"),
        )
        .at(range)]),
    }
}

/// The one webOS store Arachnid implements.
pub const WEBOS_LOCAL_STORE: &str = "webos.local-storage.v1";

/// `durable` state needs a storage adapter Arachnid does not have yet.
pub fn refuse_durable(parsed: &ParsedScene) -> Vec<Diagnostic> {
    parsed
        .declarations
        .state
        .iter()
        .flat_map(|(decls, _)| decls)
        .filter(|d| d.lifetime == StateLifetime::Durable)
        .map(|d| {
            Diagnostic::new(
                Code::StateNotLowered,
                format!(
                    "durable state '{}' needs a webOS storage adapter, which is not built yet",
                    d.key
                ),
            )
            .at(Some(d.range))
        })
        .collect()
}

/// Literals beyond the runtime's exact integer range.
pub fn refuse_unsafe_ints(parsed: &ParsedScene) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let mut check = |v: &InteractionValue, what: String| {
        if let InteractionValue::Int(i) = v {
            if i.unsigned_abs() > MAX_SAFE_INT as u64 {
                out.push(Diagnostic::new(
                    Code::IntegerOutOfRange,
                    format!("{what} is {i}, beyond the exact range ±{MAX_SAFE_INT} of the webOS runtime"),
                ));
            }
        }
    };
    for d in parsed.declarations.state.iter().flat_map(|(d, _)| d) {
        check(&d.initial, format!("initial value of '{}'", d.key));
    }
    fn walk(e: &InteractionExpr, f: &mut dyn FnMut(&InteractionValue)) {
        match e {
            InteractionExpr::Literal(v) => f(v),
            InteractionExpr::Add(a, b) => {
                walk(a, f);
                walk(b, f);
            }
            _ => {}
        }
    }
    for i in &parsed.scene.interactions {
        for a in &i.assignments {
            walk(&a.value, &mut |v| check(v, format!("a literal in '{}'", i.name)));
        }
    }
    out
}

/// JS literal for a value. Text goes through JSON string escaping, with
/// `<` escaped so a literal can never close the script element.
pub fn js_value(v: &InteractionValue) -> String {
    match v {
        InteractionValue::Int(i) => i.to_string(),
        InteractionValue::Text(t) => crate::appinfo::json_string(t).replace('<', "\\u003c"),
        InteractionValue::Bool(b) => b.to_string(),
    }
}

/// JS expression for an authored expression over `s` (pre-state snapshot)
/// and `e` (effect results). `add` is the runtime's checked addition.
pub fn js_expr(e: &InteractionExpr) -> Result<String, String> {
    Ok(match e {
        InteractionExpr::Literal(v) => js_value(v),
        InteractionExpr::State(k) => format!("s[{}]", crate::appinfo::json_string(k)),
        InteractionExpr::Effect(r) => format!("e[{}]", crate::appinfo::json_string(r)),
        InteractionExpr::Add(a, b) => format!("add({}, {})", js_expr(a)?, js_expr(b)?),
        InteractionExpr::Input(k) => {
            return Err(format!("input '{k}' needs @input, which is not lowered to webOS yet"))
        }
    })
}

/// The lowered form of one interaction, keyed by its trigger button name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lowered {
    pub name: String,
    pub trigger_button: String,
    pub d2: bool,
    /// (request name, capability label, request text, result type).
    pub requests: Vec<(String, String, String, EffectValueType)>,
    /// (state key, JS expression).
    pub writes: Vec<(String, String)>,
}

pub fn lower(i: &InteractionDecl) -> Result<Lowered, String> {
    Ok(Lowered {
        name: i.name.clone(),
        trigger_button: i.trigger_button.clone(),
        d2: i.class == InteractionClass::D2,
        requests: i
            .requests
            .iter()
            .map(|r| {
                (
                    r.name.clone(),
                    r.capability.clone(),
                    r.request_utf8.clone(),
                    r.result_type,
                )
            })
            .collect(),
        writes: i
            .assignments
            .iter()
            .map(|a| Ok((a.key.clone(), js_expr(&a.value)?)))
            .collect::<Result<_, String>>()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_literals_cannot_close_the_script() {
        assert_eq!(
            js_value(&InteractionValue::Text("</script>".into())),
            "\"\\u003c/script>\""
        );
    }
}
