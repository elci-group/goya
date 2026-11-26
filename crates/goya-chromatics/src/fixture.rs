//! Conformance fixtures (§14): a line-based script shared by every
//! implementation (this crate, generated target runtimes, resolvers).
//!
//! The format is deliberately plain so a JavaScript or Kotlin runtime can
//! read the same files. One command per line; `#` starts a comment; values
//! with spaces are double-quoted. See `fixtures/chromatics/README.md`.

use std::collections::{BTreeMap, BTreeSet};

use crate::allocate::{Allocation, Note, Request};
use crate::engine::Engine;
use crate::generation::Selection;
use crate::model::{Candidate, Capability, Policy, Risk, Slot, SlotStatus, Strategy, Trigger};
use crate::registry::{Dispatch, Registry};
use crate::scope::{compose, Inheritance, ScopeKind, Scopes};

fn tokens(line: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut any = false;
    for ch in line.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => {
                cur.push(c);
                any = true;
            }
        }
    }
    if quoted {
        return Err("unterminated quote".into());
    }
    if any {
        out.push(cur);
    }
    Ok(out)
}

fn kv(tok: &str) -> Result<(&str, &str), String> {
    tok.split_once('=')
        .ok_or_else(|| format!("expected key=value, got `{tok}`"))
}

fn yes(v: &str) -> Result<bool, String> {
    match v {
        "yes" | "true" => Ok(true),
        "no" | "false" => Ok(false),
        _ => Err(format!("expected yes/no, got `{v}`")),
    }
}

fn num<T: std::str::FromStr>(v: &str) -> Result<T, String> {
    v.parse().map_err(|_| format!("bad number `{v}`"))
}

fn opt(v: &str) -> Option<String> {
    (v != "-").then(|| v.to_string())
}

fn apply(c: &mut Candidate, key: &str, v: &str) -> Result<(), String> {
    match key {
        "intent" => c.intent = v.to_string(),
        "label" => c.label = v.to_string(),
        "entity" => c.entity = opt(v),
        "available" => c.available = yes(v)?,
        "authorised" => c.authorised = yes(v)?,
        "risk" => c.risk = Risk::parse(v).ok_or_else(|| format!("bad risk `{v}`"))?,
        "confirm" => c.confirmation = yes(v)?,
        "native" => c.native = opt(v),
        "rank" => c.rank = if v == "-" { None } else { Some(num(v)?) },
        "favourite" => c.favourite = if v == "-" { None } else { Some(num(v)?) },
        "position" => c.position = if v == "-" { None } else { Some(num(v)?) },
        "last_used" => c.last_used = if v == "-" { None } else { Some(num(v)?) },
        "uses" => c.uses = num(v)?,
        "pinned" => c.pinned = opt(v),
        "cost" => c.cost = num(v)?,
        "probability" => c.probability = num(v)?,
        "contextual" => c.contextual = num(v)?,
        "urgency" => c.urgency = num(v)?,
        "preference" => c.preference = num(v)?,
        "expires" => c.expires = if v == "-" { None } else { Some(num(v)?) },
        "symbol" => c.symbol = opt(v),
        "describe" => c.description = opt(v),
        _ => return Err(format!("unknown candidate key `{key}`")),
    }
    Ok(())
}

struct State {
    slots: Vec<Slot>,
    native: BTreeSet<String>,
    intents: Vec<String>,
    policy: Policy,
    context: String,
    /// Context set by a flat `context` command; `pop` back to depth 0
    /// restores it.
    flat_context: String,
    /// Candidates authored with no scope open.
    candidates: Vec<Candidate>,
    /// Open scopes, outermost first. Entry `i` is scope `i`'s declared
    /// inheritance policy and its own candidate pool; the last entry is the
    /// innermost scope, which `candidate`/`set`/`remove`/`clear` author
    /// into.
    pools: Vec<(Inheritance, Vec<Candidate>)>,
    scopes: Scopes,
    engine: Option<Engine>,
    last: Option<Allocation>,
    registry: Registry,
    denied: BTreeSet<String>,
    resolves: usize,
    dispatches: usize,
}

impl State {
    fn capability(&self) -> Capability {
        Capability {
            slots: self.slots.clone(),
            native: self.native.clone(),
        }
    }

    /// The bucket candidate commands author into: the innermost open
    /// scope's own pool, or the depth-0 list.
    fn authoring(&mut self) -> &mut Vec<Candidate> {
        self.pools
            .last_mut()
            .map(|(_, own)| own)
            .unwrap_or(&mut self.candidates)
    }

    /// The pool the active scope resolves against (§6): each open scope
    /// from the outside in replaces the accumulated pool or composes its
    /// own candidates onto it, per its declared inheritance.
    fn effective_pool(&self) -> Vec<Candidate> {
        if self.pools.is_empty() {
            return self.candidates.clone();
        }
        let mut pool = Vec::new();
        for (inheritance, own) in &self.pools {
            pool = match inheritance {
                Inheritance::None => own.clone(),
                Inheritance::Parent => compose(own.clone(), &pool),
            };
        }
        pool
    }

    fn sync_capability(&mut self) -> Result<(), String> {
        let cap = self.capability();
        if let Some(e) = &mut self.engine {
            e.set_capability(cap).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn set_status(&mut self, names: &[String], status: SlotStatus) -> Result<(), String> {
        for n in names {
            let s = self
                .slots
                .iter_mut()
                .find(|s| &s.name == n)
                .ok_or_else(|| format!("no slot `{n}`"))?;
            s.status = status;
        }
        self.sync_capability()
    }

    fn last(&self) -> Result<&Allocation, String> {
        self.last
            .as_ref()
            .ok_or_else(|| "no allocation yet".to_string())
    }

    fn slot(&self, name: &str) -> Result<u8, String> {
        self.capability()
            .slot(name)
            .ok_or_else(|| format!("no slot `{name}`"))
    }
}

/// Summary of a passing fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub resolves: usize,
    pub dispatches: usize,
}

/// Run a fixture. Errors name the failing line.
pub fn run(source: &str) -> Result<Outcome, String> {
    let mut st = State {
        slots: Vec::new(),
        native: BTreeSet::new(),
        intents: Vec::new(),
        policy: Policy::new(Strategy::Ranked),
        context: "app".into(),
        flat_context: "app".into(),
        candidates: Vec::new(),
        pools: Vec::new(),
        scopes: Scopes::new(),
        engine: None,
        last: None,
        registry: Registry::new(),
        denied: BTreeSet::new(),
        resolves: 0,
        dispatches: 0,
    };
    for (n, raw) in source.lines().enumerate() {
        let line = raw.split_once('#').map_or(raw, |(a, _)| a).trim();
        if line.is_empty() {
            continue;
        }
        step(&mut st, line).map_err(|e| format!("line {}: {e}\n  {raw}", n + 1))?;
    }
    Ok(Outcome {
        resolves: st.resolves,
        dispatches: st.dispatches,
    })
}

fn step(st: &mut State, line: &str) -> Result<(), String> {
    let t = tokens(line)?;
    let (cmd, args) = t.split_first().ok_or("empty")?;
    match cmd.as_str() {
        "capacity" => {
            st.slots = args
                .iter()
                .map(|n| Slot {
                    name: n.clone(),
                    status: SlotStatus::Available,
                })
                .collect();
            st.sync_capability()?;
        }
        "reserved" => st.set_status(args, SlotStatus::Reserved)?,
        "unsupported" => st.set_status(args, SlotStatus::Unsupported)?,
        "available" => st.set_status(args, SlotStatus::Available)?,
        "native" => {
            st.native = args.iter().cloned().collect();
            st.sync_capability()?;
        }
        "intents" => {
            if st.engine.is_some() {
                return Err("intents must precede the first resolve".into());
            }
            st.intents = args.to_vec();
        }
        "policy" => {
            for a in args {
                let (k, v) = kv(a)?;
                let p = &mut st.policy;
                match k {
                    "primary" => {
                        p.primary =
                            Strategy::parse(v).ok_or_else(|| format!("bad strategy `{v}`"))?
                    }
                    "fallback" => {
                        p.fallback = if v == "-" {
                            None
                        } else {
                            Some(Strategy::parse(v).ok_or_else(|| format!("bad strategy `{v}`"))?)
                        }
                    }
                    "preserve" => p.preserve_slots = yes(v)?,
                    "ordered" => p.ordered_slots = yes(v)?,
                    "exclude_native" => p.exclude_native = yes(v)?,
                    "exclude_duplicates" => p.exclude_duplicates = yes(v)?,
                    "min_improvement" => p.stability.min_improvement = num(v)?,
                    "min_residency" => p.stability.min_residency = num(v)?,
                    "max_deferral" => p.stability.max_deferral = num(v)?,
                    _ => return Err(format!("unknown policy key `{k}`")),
                }
            }
        }
        "context" => {
            if !st.pools.is_empty() {
                return Err("context is implied by push/pop once a scope is open".into());
            }
            let name = args.first().ok_or("context needs a name")?.clone();
            st.context = name.clone();
            st.flat_context = name;
        }
        "push" => {
            // Enter a scope. Its nesting rules are the Scopes ones (fixture
            // scopes are views, which nest). `push NAME inherits` declares
            // the §6 inheritance policy.
            let name = args.first().ok_or("push needs a scope name")?;
            let inherits = match args.len() {
                1 => false,
                2 if args[1] == "inherits" => true,
                _ => return Err("push NAME [inherits]".into()),
            };
            let context = if inherits {
                st.scopes
                    .enter_inheriting(ScopeKind::View, name)
                    .map_err(|e| e.to_string())?
            } else {
                st.scopes
                    .enter(ScopeKind::View, name)
                    .map_err(|e| e.to_string())?
            };
            st.pools.push((st.scopes.inheritance(), Vec::new()));
            st.context = context;
        }
        "pop" => {
            let context = st.scopes.leave().map_err(|e| e.to_string())?;
            st.pools.pop().ok_or("pop with no scope open")?;
            st.context = context.unwrap_or_else(|| st.flat_context.clone());
        }
        "candidate" => {
            let (id, rest) = args.split_first().ok_or("candidate needs an id")?;
            let mut c = Candidate::new(id, "", "");
            for a in rest {
                let (k, v) = kv(a)?;
                apply(&mut c, k, v)?;
            }
            st.authoring().push(c);
        }
        "set" => {
            let (id, rest) = args.split_first().ok_or("set needs an id")?;
            let mut found = false;
            for c in st.authoring().iter_mut().filter(|c| &c.id == id) {
                found = true;
                for a in rest {
                    let (k, v) = kv(a)?;
                    apply(c, k, v)?;
                }
            }
            if !found {
                return Err(format!("no candidate `{id}`"));
            }
        }
        "remove" => {
            let id = args.first().ok_or("remove needs an id")?;
            st.authoring().retain(|c| &c.id != id);
        }
        "clear" => st.authoring().clear(),
        "resolve" | "resolve-refused" => {
            let mut trigger = Trigger::Context;
            let mut now = 0;
            let mut interacting = false;
            for a in args {
                let (k, v) = kv(a)?;
                match k {
                    "trigger" => {
                        trigger = Trigger::parse(v).ok_or_else(|| format!("bad trigger `{v}`"))?
                    }
                    "now" => now = num(v)?,
                    "interacting" => interacting = yes(v)?,
                    _ => return Err(format!("unknown resolve key `{k}`")),
                }
            }
            let pool = st.effective_pool();
            if st.engine.is_none() {
                st.engine = Some(
                    Engine::new(st.capability(), st.intents.clone()).map_err(|e| e.to_string())?,
                );
            }
            let Some(engine) = st.engine.as_mut() else {
                return Err("no engine".into());
            };
            let req = Request {
                context: &st.context,
                policy: &st.policy,
                candidates: &pool,
                trigger,
                now,
                interacting,
            };
            let result = engine.allocate(&req);
            st.resolves += 1;
            let refused_expected = cmd == "resolve-refused";
            match (refused_expected, result) {
                (false, Ok(a)) => st.last = Some(a),
                (false, Err(v)) => return Err(format!("verification refused: {v:?}")),
                (true, Err(_)) => {}
                (true, Ok(_)) => return Err("expected verification to refuse".into()),
            }
        }
        "expect" => {
            let got: BTreeMap<String, String> =
                st.last()?.generation.assignment().into_iter().collect();
            for a in args {
                let (slot, want) = kv(a)?;
                st.slot(slot)?;
                let have = got.get(slot).map_or("-", String::as_str);
                if have != want {
                    return Err(format!(
                        "slot {slot}: expected {want}, got {have}; all {got:?}"
                    ));
                }
            }
        }
        "expect-empty" => {
            let g = &st.last()?.generation;
            if !g.bindings.is_empty() {
                return Err(format!("expected no bindings, got {:?}", g.assignment()));
            }
        }
        "expect-rejected" => {
            let [id, code] = args else {
                return Err("expect-rejected ID CODE".into());
            };
            let r = st.last()?.rejection(id).map(|r| r.code());
            if r != Some(code.as_str()) {
                return Err(format!("{id}: expected rejection {code}, got {r:?}"));
            }
        }
        "expect-selection" => {
            let [id, want] = args else {
                return Err("expect-selection ID KIND".into());
            };
            let b = st
                .last()?
                .generation
                .by_candidate(id)
                .ok_or_else(|| format!("{id} is not bound"))?;
            let have = match b.selected {
                Selection::Pinned => "pinned",
                Selection::Primary(_) => "primary",
                Selection::Fallback(_) => "fallback",
                Selection::Retained => "retained",
            };
            if have != want {
                return Err(format!("{id}: expected selection {want}, got {have}"));
            }
        }
        "expect-symbol" | "expect-describe" => {
            let got = st.last()?.generation.bindings.clone();
            for a in args {
                let (slot, want) = kv(a)?;
                let slot = st.slot(slot)?;
                let b = got
                    .iter()
                    .find(|b| b.slot == slot)
                    .ok_or_else(|| format!("slot {slot} is not bound"))?;
                let field = if cmd == "expect-symbol" {
                    &b.symbol
                } else {
                    &b.description
                };
                let have = field.as_deref().unwrap_or("-");
                if have != want {
                    return Err(format!("slot {slot}: expected {want:?}, got {have:?}"));
                }
            }
        }
        "expect-note" => {
            let kind = args.first().ok_or("expect-note KIND")?;
            let notes = &st.last()?.notes;
            let ok = notes.iter().any(|n| {
                matches!(
                    (kind.as_str(), n),
                    ("deferred", Note::Deferred)
                        | ("retained", Note::Retained { .. })
                        | ("displaced", Note::Displaced { .. })
                        | ("pin-ignored", Note::PinIgnored { .. })
                )
            });
            if !ok {
                return Err(format!("no `{kind}` note in {notes:?}"));
            }
        }
        "expect-generation" => {
            let want: u64 = num(args.first().ok_or("expect-generation N")?)?;
            let have = st.last()?.generation.id;
            if have != want {
                return Err(format!("expected generation {want}, got {have}"));
            }
        }
        "publish" => {
            let g = st.last()?.generation.clone();
            st.registry.activate(&g.context);
            st.registry.publish(g);
        }
        "stage" => {
            let g = st.last()?.generation.clone();
            st.registry.activate(&g.context);
            st.registry.stage(g);
        }
        "present" => {
            let id = st.registry.staged().map(|g| g.id).ok_or("nothing staged")?;
            if !st.registry.presented(id) {
                return Err("present did not commit".into());
            }
        }
        "activate" => st
            .registry
            .activate(args.first().ok_or("activate CONTEXT")?),
        "revoke" => st.registry.revoke(args.first().ok_or("revoke ID")?),
        "reinstate" => st.registry.reinstate(args.first().ok_or("reinstate ID")?),
        "deny" => {
            st.denied.insert(args.first().ok_or("deny INTENT")?.clone());
        }
        "allow" => {
            st.denied.remove(args.first().ok_or("allow INTENT")?);
        }
        "cancel-presses" => st.registry.cancel_presses(),
        "press" => {
            let name = args.first().ok_or("press SLOT")?;
            let slot = st.slot(name)?;
            let repeat = args.get(1).map(String::as_str) == Some("repeat");
            let p = st.registry.press(slot);
            match (repeat, p.is_some()) {
                (false, false) => return Err("press was swallowed as a repeat".into()),
                (true, true) => return Err("expected an auto-repeat".into()),
                _ => {}
            }
        }
        "release" => {
            let name = args.first().ok_or("release SLOT expect=…")?;
            let slot = st.slot(name)?;
            let want = args
                .get(1)
                .map(|a| kv(a))
                .transpose()?
                .filter(|(k, _)| *k == "expect")
                .map(|(_, v)| v.to_string())
                .ok_or("release needs expect=")?;
            let denied = st.denied.clone();
            let auth = move |intent: &str, _: Option<&str>| !denied.contains(intent);
            let d = st.registry.release(slot, &auth);
            st.dispatches += 1;
            let have = match &d {
                Dispatch::Execute { candidate, .. } => format!("execute:{candidate}"),
                Dispatch::Unassigned => "unassigned".to_string(),
                Dispatch::Rejected(r) => format!("rejected:{}", r.code()),
            };
            if have != want {
                return Err(format!("release {name}: expected {want}, got {have}"));
            }
        }
        other => return Err(format!("unknown command `{other}`")),
    }
    Ok(())
}
