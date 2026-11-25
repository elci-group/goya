//! These tests check that a local write does not reevaluate unrelated displays
//! and does not rerun layout. They do not measure time or memory, and they do
//! not claim a frame rate.

use std::collections::BTreeMap;

use goya_lang::{InteractionValue, RectMp};
use goya_resolver::Resolver;

const LIVE: &str = include_str!("../../sash/fixtures/interactive-live.gscene");

fn resolver() -> Resolver {
    Resolver::builder()
        .viewport(320.0, 160.0)
        .build()
        .expect("viewport")
}

fn running(source: &str) -> goya_resolver::Instance {
    let resolver = resolver();
    let application = resolver.load(source).expect("load");
    let mut instance = resolver.instantiate(&application).expect("instance");
    instance.start().expect("start");
    instance
}

fn observed(instance: &goya_resolver::Instance) -> BTreeMap<String, InteractionValue> {
    instance
        .state()
        .into_iter()
        .map(|entry| (entry.key, entry.value))
        .collect()
}

fn local_scene() -> String {
    let mut source = String::from(
        r#"@ontology local {
  @state {
    count: int = 0 session;
    title: text = "a" session;
    idle: int = 0 session;
  }
  @cell root {
    id: "root";
    @cell title {
      id: "title";
      text: "Title: ";
      text-from: state.title;
    }
    @cell count {
      id: "count";
      text: "Count: ";
      text-from: state.count;
    }
"#,
    );
    for index in 0..200 {
        source.push_str(&format!(
            "    @cell idle{index} {{ id: \"idle.{index}\"; text-from: state.idle; }}\n"
        ));
    }
    source.push_str(
        r#"    @input amount {
      id: "amount";
      text: "1";
      @iden { label: "Amount"; hint: "n"; }
    }
    @button add {
      id: "add";
      text: "Add";
    }
  }
  @interaction add {
    id: "action.add";
    trigger: button.add.activate;
    class: D1;
    state { read count: int; write title: text; write count: int; }
    input { amount: int from "amount"; }
    set title = "b";
    set count = state.count + input.amount;
  }
}
"#,
    );
    source
}

#[test]
fn a_local_write_skips_unrelated_displays() {
    let source = local_scene();
    let resolver = resolver();
    let application = resolver.load(&source).expect("load");
    assert_eq!(application.semantic_contract(), "GSSH/4-live");
    assert_eq!(application.dependents("idle").len(), 200);
    assert_eq!(application.dependents("count"), ["count"]);
    assert_eq!(application.dependents("title"), ["title"]);
    let component_count = application.components().len();

    let mut instance = resolver.instantiate(&application).expect("instance");
    instance.start().unwrap();
    let before = observed(&instance);
    let added = instance.activate("action.add").unwrap();
    let after = observed(&instance);

    assert_eq!(added.revision, 1);
    assert_eq!(instance.revision(), 1);
    assert_eq!(added.sites_evaluated, 2);
    assert_ne!(added.sites_evaluated, 202);
    assert_ne!(added.sites_evaluated, component_count);
    assert_eq!(added.affected_elements, ["title", "count"]);
    assert_eq!(
        added.announcement,
        goya_lang::display_binding::scene_announcement(&application.scene().root, &before, &after)
    );
    assert_eq!(
        added.announcement.as_deref(),
        Some("Title changed from \"a\" to \"b\"; Count increased by 1, from 0 to 1")
    );
}

#[test]
fn an_unchanged_write_evaluates_no_display() {
    let mut instance = running(LIVE);
    instance.set_field("window.amount", "0").unwrap();
    let same = instance.activate("action.add").unwrap();
    let count = &instance.state()[0];
    assert_eq!(count.key, "count");
    assert_eq!(count.value, InteractionValue::Int(0));
    assert_eq!(same.revision, 1);
    assert_eq!(instance.revision(), 1);
    assert!(same.announcement.is_none());
    assert!(same.affected_elements.is_empty());
    assert_eq!(same.sites_evaluated, 0);
}

#[test]
fn layout_is_not_rerun_after_a_state_write() {
    let mut instance = running(LIVE);
    let expected = RectMp {
        x: 20_000,
        y: 60_000,
        width: 120_000,
        height: 30_000,
    };
    let before = instance.frame().find("amount").expect("amount").rect;
    assert_eq!(instance.layout_generations(), 1);
    assert_eq!(before, expected);

    instance.set_field("window.amount", "1").unwrap();
    let dispatched = instance.activate("action.add").unwrap();
    assert_eq!(dispatched.revision, 1);
    let after = instance.frame().find("amount").expect("amount").rect;
    assert_eq!(instance.layout_generations(), 1);
    assert_eq!(after, expected);
}
