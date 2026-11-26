//! Property tests (§26.2) and randomised dispatch sequences (§26.3).
//!
//! A fixed xorshift generator keeps every run reproducible without a
//! dependency. Each property runs over a few thousand generated cases.

use std::collections::BTreeSet;

use goya_chromatics::{
    Candidate, Capability, Dispatch, Engine, Policy, Registry, Request, Risk, Slot, SlotStatus,
    Strategy, Trigger,
};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i as u64 + 1) as usize;
            v.swap(i, j);
        }
    }
}

const NAMES: [&str; 6] = ["red", "green", "yellow", "blue", "f1", "f2"];
const INTENTS: [&str; 3] = ["open", "play", "delete"];
const STRATEGIES: [Strategy; 7] = [
    Strategy::Manual,
    Strategy::Preferential,
    Strategy::Ranked,
    Strategy::Temporal,
    Strategy::Frequent,
    Strategy::Positional,
    Strategy::Utility,
];
const TRIGGERS: [Trigger; 5] = [
    Trigger::Context,
    Trigger::Policy,
    Trigger::Capability,
    Trigger::Background,
    Trigger::Invalidation,
];

fn capability(r: &mut Rng) -> Capability {
    let n = r.below(NAMES.len() as u64 + 1) as usize;
    let slots = NAMES[..n]
        .iter()
        .map(|name| Slot {
            name: (*name).to_string(),
            status: match r.below(6) {
                0 => SlotStatus::Reserved,
                1 => SlotStatus::Unsupported,
                _ => SlotStatus::Available,
            },
        })
        .collect();
    let mut native = BTreeSet::new();
    if r.chance(50) {
        native.insert("volume".to_string());
    }
    Capability { slots, native }
}

fn candidate(r: &mut Rng, i: usize) -> Candidate {
    // Occasional duplicate ids and shared entities exercise malformed input.
    let id = format!("c{}", if r.chance(5) { i / 2 } else { i });
    let mut c = Candidate::new(&id, INTENTS[r.below(4) as usize % 3], "label");
    if r.chance(3) {
        c.intent = "unregistered".into();
    }
    if r.chance(3) {
        c.label = String::new();
    }
    c.entity = r.chance(60).then(|| format!("e{}", r.below(12)));
    c.available = !r.chance(10);
    c.authorised = !r.chance(10);
    c.risk = match r.below(8) {
        0 => Risk::Destructive,
        1 => Risk::Sensitive,
        2 => Risk::Operational,
        _ => Risk::Navigational,
    };
    c.confirmation = r.chance(40);
    c.native = r.chance(10).then(|| "volume".to_string());
    c.rank = r.chance(70).then(|| r.below(20) as u32);
    c.favourite = r.chance(30).then(|| r.below(5) as u32);
    c.position = r.chance(50).then(|| r.below(30) as u32);
    c.last_used = r.chance(50).then(|| r.below(1000));
    c.uses = r.below(5) as u32;
    c.pinned = r
        .chance(8)
        .then(|| NAMES[r.below(7) as usize % 6].to_string());
    c.cost = 1 + r.below(10) as u32;
    c.probability = r.below(1001) as u32;
    c.urgency = r.below(500) as i64;
    c.expires = r.chance(10).then(|| r.below(100));
    c
}

fn policy(r: &mut Rng) -> Policy {
    let mut p = Policy::new(STRATEGIES[r.below(7) as usize]);
    p.fallback = r.chance(50).then(|| STRATEGIES[r.below(7) as usize]);
    p.preserve_slots = r.chance(70);
    p.ordered_slots = r.chance(20);
    p.exclude_native = r.chance(80);
    p.exclude_duplicates = r.chance(80);
    p.stability.min_improvement = r.below(300) as i64;
    p.stability.min_residency = r.below(20);
    p.stability.max_deferral = 1 + r.below(50);
    p
}

fn valid(c: &Candidate, intents: &BTreeSet<String>, now: u64) -> bool {
    c.available
        && c.authorised
        && c.expires.is_none_or(|t| t > now)
        && (c.confirmation || !c.risk.needs_confirmation())
        && intents.contains(&c.intent)
        && !c.label.trim().is_empty()
}

#[test]
fn every_allocation_respects_the_invariants() {
    let mut r = Rng(0x9E37_79B9_7F4A_7C15);
    for _case in 0..1500 {
        let cap = capability(&mut r);
        let mut engine = Engine::new(cap.clone(), INTENTS).expect("capability");
        let intents: BTreeSet<String> = INTENTS.iter().map(|s| s.to_string()).collect();
        let pol = policy(&mut r);
        let n = r.below(40) as usize;
        let mut cands: Vec<Candidate> = (0..n).map(|i| candidate(&mut r, i)).collect();
        let mut now = 0;
        for _step in 0..12 {
            now += r.below(10);
            let req = Request {
                context: if r.chance(10) { "other" } else { "ctx" },
                policy: &pol,
                candidates: &cands,
                trigger: TRIGGERS[r.below(5) as usize],
                now,
                interacting: r.chance(20),
            };
            let a = engine.allocate(&req).expect("allocator output must verify");
            let g = &a.generation;
            // Capacity correctness.
            assert!(g.bindings.len() <= cap.capacity());
            let mut slots = BTreeSet::new();
            let mut ids = BTreeSet::new();
            let mut ents = BTreeSet::new();
            for b in &g.bindings {
                assert_eq!(cap.slots[usize::from(b.slot)].status, SlotStatus::Available);
                assert!(slots.insert(b.slot), "slot uniqueness");
                assert!(ids.insert(b.candidate.clone()));
                let same: Vec<&Candidate> = cands.iter().filter(|c| c.id == b.candidate).collect();
                assert_eq!(same.len(), 1, "duplicate ids are never bound");
                let c = same[0];
                assert!(valid(c, &intents, now), "invalid candidate bound: {c:?}");
                if pol.exclude_native {
                    assert!(!c.native.as_ref().is_some_and(|k| cap.native.contains(k)));
                }
                if pol.exclude_duplicates {
                    if let Some(e) = &b.entity {
                        assert!(ents.insert(e.clone()), "duplicate entity bound");
                    }
                }
            }
            // Mutate a little for the next step.
            for c in cands.iter_mut() {
                if r.chance(15) {
                    *c = Candidate {
                        id: c.id.clone(),
                        ..candidate(&mut r, 0)
                    };
                }
            }
        }
    }
}

#[test]
fn allocation_ignores_candidate_order() {
    let mut r = Rng(0xD1B5_4A32_D192_ED03);
    for _ in 0..1500 {
        let cap = capability(&mut r);
        let pol = policy(&mut r);
        let n = r.below(40) as usize;
        let cands: Vec<Candidate> = (0..n).map(|i| candidate(&mut r, i)).collect();
        let trigger = TRIGGERS[r.below(5) as usize];
        let now = r.below(50);
        let run = |cs: &[Candidate]| {
            let mut e = Engine::new(cap.clone(), INTENTS).unwrap();
            let warm = Request {
                context: "ctx",
                policy: &pol,
                candidates: cs,
                trigger: Trigger::Context,
                now: 0,
                interacting: false,
            };
            e.allocate(&warm).unwrap();
            let req = Request {
                trigger,
                now,
                ..warm
            };
            let a = e.allocate(&req).unwrap();
            (a.generation.bindings, a.rejected)
        };
        let base = run(&cands);
        let mut shuffled = cands.clone();
        r.shuffle(&mut shuffled);
        assert_eq!(run(&shuffled), base);
        // Unchanged inputs, unchanged assignment.
        let mut e = Engine::new(cap.clone(), INTENTS).unwrap();
        let req = Request {
            context: "ctx",
            policy: &pol,
            candidates: &cands,
            trigger: Trigger::Context,
            now: 1,
            interacting: false,
        };
        let first = e.allocate(&req).unwrap().generation.assignment();
        let again = e
            .allocate(&Request { trigger, ..req })
            .unwrap()
            .generation
            .assignment();
        assert_eq!(first, again, "re-resolving unchanged input moved a binding");
    }
}

#[test]
fn dispatch_never_redirects_or_runs_revoked_bindings() {
    let mut r = Rng(0xA076_1D64_78BD_642F);
    let cap = Capability::webos_colour_keys();
    let (mut executed, mut refused) = (0u32, 0u32);
    for _ in 0..400 {
        let mut engine = Engine::new(cap.clone(), INTENTS).unwrap();
        let mut reg = Registry::new();
        let pol = Policy::new(Strategy::Utility);
        let mut cands: Vec<Candidate> = (0..10)
            .map(|i| {
                let mut c = Candidate::new(&format!("c{i}"), "open", "L");
                c.urgency = r.below(100) as i64;
                c
            })
            .collect();
        let contexts = ["a", "b"];
        let mut revoked: BTreeSet<String> = BTreeSet::new();
        let mut held: [Option<goya_chromatics::Press>; 4] = Default::default();
        for step in 0..200u64 {
            match r.below(9) {
                0 | 1 => {
                    for c in cands.iter_mut() {
                        c.urgency = r.below(100) as i64;
                    }
                    let ctx = contexts[r.below(2) as usize];
                    let req = Request {
                        context: ctx,
                        policy: &pol,
                        candidates: &cands,
                        trigger: TRIGGERS[r.below(5) as usize],
                        now: step,
                        interacting: false,
                    };
                    let g = engine.allocate(&req).unwrap().generation;
                    reg.activate(ctx);
                    if r.chance(50) {
                        reg.publish(g);
                    } else {
                        reg.stage(g);
                    }
                }
                2 => {
                    if let Some(id) = reg.staged().map(|g| g.id) {
                        assert!(reg.presented(id));
                    }
                }
                3 => {
                    let id = format!("c{}", r.below(10));
                    reg.revoke(&id);
                    revoked.insert(id);
                }
                4 | 5 => {
                    let s = usize::from(r.below(4) as u8);
                    match reg.press(s as u8) {
                        Some(p) => held[s] = Some(p),
                        None => assert!(held[s].is_some(), "only held keys repeat"),
                    }
                }
                6 => {
                    reg.cancel_presses();
                    held = Default::default();
                }
                _ => {
                    let s = usize::from(r.below(4) as u8);
                    let deny = r.chance(10);
                    let d = reg.release(s as u8, &|_: &str, _: Option<&str>| !deny);
                    let pressed = held[s].take();
                    if matches!(d, Dispatch::Rejected(_)) {
                        refused += 1;
                    }
                    if let Dispatch::Execute { candidate, .. } = d {
                        executed += 1;
                        let p = pressed.expect("executed without a press");
                        assert!(!p.ambiguous && !p.inactive, "executed through a transition");
                        let meant = p.target.map(|t| t.0);
                        assert_eq!(meant.as_deref(), Some(candidate.as_str()), "redirected");
                        assert!(!revoked.contains(&candidate), "revoked binding executed");
                        assert!(!deny, "executed without permission");
                        assert_eq!(reg.active(), p.context.as_deref());
                    }
                }
            }
        }
    }
    assert!(
        executed > 300 && refused > 1000,
        "executed {executed}, refused {refused}"
    );
}
