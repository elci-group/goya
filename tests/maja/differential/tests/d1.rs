//! Canonical host-oracle fixtures only: no browser target is under test.
use goya_executable::{D1Snapshot, D1Value, InputSnapshot};
use goya_sdk::D1Application;
use std::collections::BTreeMap;

const SOURCE: &str = include_str!("../fixtures/counter.gscene");

fn values(count: i64) -> BTreeMap<String, D1Value> {
    BTreeMap::from([("count".into(), D1Value::Int(count))])
}
fn input(amount: i64) -> BTreeMap<String, D1Value> {
    BTreeMap::from([("amount".into(), D1Value::Int(amount))])
}

#[test]
fn host_owner_agrees_with_canonical_transition_for_sequences() {
    let scene = goya_lang::parse_strict(SOURCE).unwrap().scene;
    let interaction = goya_lang::compile_d1_interactions(&scene)
        .unwrap()
        .remove(0);
    for (start, amounts) in [
        (0, vec![1, 2, -4]),
        (i64::MAX - 1, vec![1]),
        (i64::MIN + 1, vec![-1]),
    ] {
        let mut app = D1Application::new(&scene, values(start)).unwrap();
        let mut state = D1Snapshot::new(0, values(start)).unwrap();
        for amount in amounts {
            let snapshot = InputSnapshot::new(input(amount)).unwrap();
            let expected = interaction.execute(&state, &snapshot).unwrap();
            app.dispatch("action.add", input(amount)).unwrap();
            assert_eq!(app.values(), &expected.next_state);
            assert_eq!(app.revision(), expected.revision_after);
            state = D1Snapshot::new(expected.revision_after, expected.next_state).unwrap();
        }
    }
}

#[test]
fn failures_preserve_the_committed_state_and_revision() {
    let scene = goya_lang::parse_strict(SOURCE).unwrap().scene;
    for (initial, revision, bad_input) in [
        (i64::MAX, 0, input(1)),
        (i64::MIN, 0, input(-1)),
        (5, u64::MAX, input(1)),
        (
            5,
            0,
            BTreeMap::from([("amount".into(), D1Value::Bool(true))]),
        ),
        (5, 0, BTreeMap::new()),
    ] {
        let mut app = D1Application::restore(&scene, revision, values(initial)).unwrap();
        assert!(app.dispatch("action.add", bad_input).is_err());
        assert_eq!(app.values(), &values(initial));
        assert_eq!(app.revision(), revision);
    }
}
