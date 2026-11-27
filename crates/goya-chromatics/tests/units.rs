//! Unit tests for verification, scopes and capability negotiation.

use std::error::Error;

use goya_chromatics::{
    verify, Candidate, Capability, CapabilityError, Engine, Policy, Rejection, Request, Risk,
    ScopeError, ScopeKind, Scopes, Slot, SlotStatus, Strategy, Trigger, Violation, MAX_SLOTS,
};

fn cands() -> Vec<Candidate> {
    (0..4)
        .map(|i| {
            let mut c = Candidate::new(&format!("c{i}"), "open", &format!("C{i}"));
            c.rank = Some(i);
            c.entity = Some(format!("e{i}"));
            c
        })
        .collect()
}

fn allocate(
    cap: &Capability,
    cs: &[Candidate],
    policy: &Policy,
) -> Result<goya_chromatics::Generation, Box<dyn Error>> {
    let mut e = Engine::new(cap.clone(), ["open"])?;
    let req = Request {
        context: "v",
        policy,
        candidates: cs,
        trigger: Trigger::Context,
        now: 0,
        interacting: false,
    };
    Ok(e.allocate(&req)?.generation)
}

#[test]
fn verification_rejects_tampered_generations() -> Result<(), Box<dyn Error>> {
    let cap = Capability::webos_colour_keys();
    let policy = Policy::new(Strategy::Ranked);
    let cs = cands();
    let intents = ["open".to_string()].into_iter().collect();
    let good = allocate(&cap, &cs, &policy)?;
    assert_eq!(verify(&good, "v", &cap, &policy, &cs, &intents), Ok(()));

    let mut reused = good.clone();
    reused.bindings[1].slot = 0;
    reused.bindings[1].slot_name = "red".into();
    let v = verify(&reused, "v", &cap, &policy, &cs, &intents)
        .err()
        .unwrap_or_default();
    assert!(v.contains(&Violation::SlotReused("red".into())));

    let mut revoked = cs.clone();
    revoked[0].authorised = false;
    let v = verify(&good, "v", &cap, &policy, &revoked, &intents)
        .err()
        .unwrap_or_default();
    assert!(v.contains(&Violation::Invalid("c0".into(), Rejection::Unauthorised)));

    let mut reserved = cap.clone();
    reserved.slots[2].status = SlotStatus::Reserved;
    let v = verify(&good, "v", &reserved, &policy, &cs, &intents)
        .err()
        .unwrap_or_default();
    assert!(v.contains(&Violation::SlotNotAvailable("yellow".into())));

    let mut relabelled = good.clone();
    relabelled.bindings[0].intent = "files.delete".into();
    let v = verify(&relabelled, "v", &cap, &policy, &cs, &intents)
        .err()
        .unwrap_or_default();
    assert!(v.contains(&Violation::BindingMismatch("c0".into())));

    let v = verify(&good, "w", &cap, &policy, &cs, &intents)
        .err()
        .unwrap_or_default();
    assert!(matches!(v.first(), Some(Violation::ContextMismatch { .. })));

    let mut ghost = good.clone();
    ghost.bindings[3].candidate = "ghost".into();
    let v = verify(&ghost, "v", &cap, &policy, &cs, &intents)
        .err()
        .unwrap_or_default();
    assert!(v.contains(&Violation::UnknownCandidate("ghost".into())));
    Ok(())
}

#[test]
fn malformed_capabilities_are_refused() {
    let many = Capability {
        slots: (0..=MAX_SLOTS)
            .map(|i| Slot {
                name: format!("s{i}"),
                status: SlotStatus::Available,
            })
            .collect(),
        native: Default::default(),
    };
    assert_eq!(
        Engine::new(many, ["open"]).err(),
        Some(CapabilityError::TooManySlots(MAX_SLOTS + 1))
    );
    let dup = Capability::available(&["red", "red"]);
    assert_eq!(
        Engine::new(dup, ["open"]).err(),
        Some(CapabilityError::DuplicateSlot("red".into()))
    );
    assert_eq!(
        Engine::new(Capability::available(&[""]), ["open"]).err(),
        Some(CapabilityError::EmptySlotName)
    );
}

#[test]
fn utility_follows_the_directive_formula() {
    let mut c = Candidate::new("x", "open", "X");
    c.probability = 500;
    c.cost = 9; // eight inputs saved
    c.contextual = 10;
    c.urgency = 20;
    c.preference = 30;
    assert_eq!(c.utility(), 500 * 8 + 60);
    c.probability = 5000; // clamped to 1000
    assert_eq!(c.utility(), 1000 * 8 + 60);
    c.cost = 0;
    assert_eq!(c.utility(), 60);
}

#[test]
fn risky_actions_only_bind_through_confirmation() {
    for risk in [Risk::Sensitive, Risk::Destructive, Risk::Privileged] {
        assert!(risk.needs_confirmation());
    }
    for risk in [Risk::Navigational, Risk::Reversible, Risk::Operational] {
        assert!(!risk.needs_confirmation());
    }
}

#[test]
fn scopes_nest_from_broad_to_narrow() -> Result<(), ScopeError> {
    let mut s = Scopes::new();
    assert_eq!(s.enter(ScopeKind::Application, "launcher")?, "launcher");
    assert_eq!(s.enter(ScopeKind::Profile, "owner")?, "launcher/owner");
    s.enter(ScopeKind::View, "shows")?;
    s.enter(ScopeKind::View, "details")?;
    assert_eq!(
        s.enter(ScopeKind::Profile, "guest").err(),
        Some(ScopeError::Nesting {
            outer: ScopeKind::View,
            inner: ScopeKind::Profile
        })
    );
    assert_eq!(
        s.enter(ScopeKind::Modal, "confirm")?,
        "launcher/owner/shows/details/confirm"
    );
    assert_eq!(s.active_kind(), Some(ScopeKind::Modal));
    assert_eq!(s.leave()?.as_deref(), Some("launcher/owner/shows/details"));
    assert_eq!(
        s.enter(ScopeKind::View, "a/b").err(),
        Some(ScopeError::Separator("a/b".into()))
    );
    assert_eq!(
        s.enter(ScopeKind::View, "").err(),
        Some(ScopeError::EmptyName)
    );
    while s.depth() > 0 {
        s.leave()?;
    }
    assert_eq!(s.context(), None);
    assert_eq!(s.leave().err(), Some(ScopeError::NothingToClose));
    Ok(())
}

#[test]
fn forgetting_a_profile_drops_only_its_history() -> Result<(), Box<dyn Error>> {
    let cap = Capability::webos_colour_keys();
    let policy = Policy::new(Strategy::Ranked);
    let cs = cands();
    let mut e = Engine::new(cap, ["open"])?;
    for ctx in ["owner/shows", "owner/films", "family/shows"] {
        let req = Request {
            context: ctx,
            policy: &policy,
            candidates: &cs,
            trigger: Trigger::Context,
            now: 0,
            interacting: false,
        };
        e.allocate(&req)?;
    }
    e.forget("owner/");
    assert!(e.previous("owner/shows").is_none());
    assert!(e.previous("owner/films").is_none());
    assert!(e.previous("family/shows").is_some());
    Ok(())
}

#[test]
fn adversarial_collections_stay_bounded() -> Result<(), Box<dyn Error>> {
    // 10,000 candidates sharing one id, one entity, or one pinned slot.
    let cap = Capability::webos_colour_keys();
    let policy = Policy::new(Strategy::Utility);
    let mut cs: Vec<Candidate> = (0..10_000)
        .map(|i| {
            let mut c = Candidate::new(&format!("c{i}"), "open", "L");
            c.entity = Some("same".into());
            c.pinned = Some("red".into());
            c.urgency = i64::MAX - i;
            c
        })
        .collect();
    cs.push(Candidate::new("c0", "open", "dup"));
    let g = allocate(&cap, &cs, &policy)?;
    assert_eq!(
        g.bindings.len(),
        1,
        "one entity, one binding: {:?}",
        g.assignment()
    );
    assert_eq!(g.bindings[0].candidate, "c1");
    Ok(())
}

#[test]
fn unlabelled_bindings_are_refused_at_publication() -> Result<(), Box<dyn Error>> {
    // §21: colour never identifies a binding. Eligibility refuses a
    // label-less candidate at the source; verification re-checks the
    // published generation without trusting the allocator.
    let cap = Capability::webos_colour_keys();
    let policy = Policy::new(Strategy::Ranked);
    let cs = cands();
    let intents = ["open".to_string()].into_iter().collect();
    let mut good = allocate(&cap, &cs, &policy)?;
    good.bindings[0].label = "  ".into();
    let v = verify(&good, "v", &cap, &policy, &cs, &intents)
        .err()
        .unwrap_or_default();
    assert!(v.contains(&Violation::Unlabelled("c0".into())), "{v:?}");
    Ok(())
}

#[test]
fn bindings_carry_the_accessibility_metadata() -> Result<(), Box<dyn Error>> {
    // §7/§21: symbol and accessible description ride the candidate into the
    // published binding, so the presentation layer need not rely on colour.
    let cap = Capability::webos_colour_keys();
    let policy = Policy::new(Strategy::Ranked);
    let mut cs = cands();
    cs[0].symbol = Some("star".into());
    cs[0].description = Some("Opens the first item".into());
    let g = allocate(&cap, &cs, &policy)?;
    let b = g.binding(0).expect("c0 binds first");
    assert_eq!(b.symbol.as_deref(), Some("star"));
    assert_eq!(b.description.as_deref(), Some("Opens the first item"));
    assert_eq!(g.binding(1).unwrap().symbol, None);
    Ok(())
}

#[test]
fn inheritance_composes_the_chain_with_the_inner_scope_first() {
    // compose: own candidates win an id conflict; the parent's fill the rest.
    let mut own = Candidate::new("confirm", "open", "Confirm");
    own.rank = Some(0);
    let mut parent_a = Candidate::new("fav-a", "open", "A");
    parent_a.rank = Some(1);
    let mut shared = Candidate::new("confirm", "stale", "Stale");
    shared.rank = Some(2);
    let pool = goya_chromatics::compose(vec![own, parent_a], &[shared]);
    let ids: Vec<&str> = pool.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["confirm", "fav-a"]);
    assert_eq!(pool[0].intent, "open", "the inner declaration wins");

    // Scopes: inheritance is declared per scope, and the nesting rules
    // still hold.
    let mut s = Scopes::new();
    s.enter(ScopeKind::Profile, "sal").unwrap();
    assert_eq!(s.inheritance(), goya_chromatics::Inheritance::None);
    s.enter_inheriting(ScopeKind::View, "shows").unwrap();
    assert_eq!(s.inheritance(), goya_chromatics::Inheritance::Parent);
    assert_eq!(s.context().as_deref(), Some("sal/shows"));
    s.enter_inheriting(ScopeKind::View, "details").unwrap();
    assert_eq!(
        s.context().as_deref(),
        Some("sal/shows/details"),
        "inheriting views nest like any other"
    );
    assert!(matches!(
        s.enter(ScopeKind::Profile, "other").unwrap_err(),
        ScopeError::Nesting { .. }
    ));
    s.leave().unwrap();
    assert_eq!(s.context().as_deref(), Some("sal/shows"));
}
