//! Each served platform has a host. None of them open a surface.

use goya_lang::InteractionValue;
use goya_resolver::{CaretMotion, Edit, Event, PointOutcome, Resolver, ScrollOutcome, Target};
use goya_resolver_host::{
    AndroidHost, BrowserHost, IosHost, LinuxHost, TelevisionHost, WindowsHost,
};

const LIVE: &str = include_str!("../../sash/fixtures/interactive-live.gscene");
const SCROLLED: &str = include_str!("../../goya-resolver/tests/fixtures/scrolled-button.gscene");

const WINDOWS_ONLY: &str = r#"@ontology app {
  @cell root { id: "root"; }
  @platform windows { "media": cede; }
}
"#;

const BOTH: &str = r#"@ontology app {
  @cell root { id: "root"; }
  @platform windows { "media": cede; }
  @platform linux { "media": cede; }
}
"#;

#[test]
fn every_host_projects_the_same_live_counter() {
    assert_same(&AndroidHost::open().expect("android"));
    assert_same(&TelevisionHost::open().expect("television"));
    assert_same(&BrowserHost::open().expect("browser"));
    assert_same(&IosHost::open().expect("ios"));
    assert_same(&LinuxHost::open().expect("linux"));
    assert_same(&WindowsHost::open().expect("windows"));
}

fn assert_same(host: &impl HostApi) {
    let application = host.load(LIVE).expect("load");
    assert_eq!(application.target(), Some(host.target()));
    assert_eq!(application.semantic_contract(), "GSSH/4-live");
    let mut instance = host.instantiate(&application).expect("instance");
    instance.start().unwrap();
    let outcome = instance
        .dispatch(&Event::Activate {
            action: "action.add".into(),
        })
        .unwrap();
    assert_eq!(outcome.revision, 1);
    assert_eq!(instance.state()[0].value, InteractionValue::Int(1));
    let phase = instance.phase();
    let revision = instance.revision();
    let surface = host.present(&mut instance).expect("project");
    assert!(!surface.opened);
    assert_eq!(surface.platform, host.target().id());
    assert_eq!(surface.surface, host.target().surface());
    assert_eq!(surface.compiler, host.target().compiler());
    assert_eq!(surface.revision, 1);
    let total = surface
        .nodes
        .iter()
        .find(|node| node.id.as_deref() == Some("window.total"))
        .expect("total");
    assert_eq!(total.text, "1");
    assert_eq!(total.role, "cell");
    assert_eq!(instance.phase(), phase);
    assert_eq!(instance.revision(), revision);
}

trait HostApi {
    fn target(&self) -> Target;
    fn load(&self, source: &str) -> Result<goya_resolver::Application, goya_resolver::Diagnostic>;
    fn instantiate(
        &self,
        application: &goya_resolver::Application,
    ) -> Result<goya_resolver::Instance, goya_resolver::Diagnostic>;
    fn present(
        &self,
        instance: &mut goya_resolver::Instance,
    ) -> Result<goya_resolver_host::NativeSurface, goya_resolver::Diagnostic>;
}

macro_rules! api {
    ($name:ident) => {
        impl HostApi for $name {
            fn target(&self) -> Target {
                Self::TARGET
            }
            fn load(
                &self,
                source: &str,
            ) -> Result<goya_resolver::Application, goya_resolver::Diagnostic> {
                $name::load(self, source)
            }
            fn instantiate(
                &self,
                application: &goya_resolver::Application,
            ) -> Result<goya_resolver::Instance, goya_resolver::Diagnostic> {
                $name::instantiate(self, application)
            }
            fn present(
                &self,
                instance: &mut goya_resolver::Instance,
            ) -> Result<goya_resolver_host::NativeSurface, goya_resolver::Diagnostic> {
                $name::present(self, instance)
            }
        }
    };
}

api!(AndroidHost);
api!(TelevisionHost);
api!(BrowserHost);
api!(IosHost);
api!(LinuxHost);
api!(WindowsHost);

#[test]
fn a_host_keeps_foreign_rules_and_refuses_its_own() {
    let linux = LinuxHost::open().unwrap();
    let windows = WindowsHost::open().unwrap();
    let browser = BrowserHost::open().unwrap();
    assert!(linux.load(WINDOWS_ONLY).is_ok());
    match windows.load(WINDOWS_ONLY) {
        Err(error) => {
            assert_eq!(error.code, "GY-RESOLVER-PLATFORM");
            assert_eq!(
                error.message,
                "unresolved windows platform exception for 'media'"
            );
            assert!(error.range.is_some());
        }
        Ok(_) => panic!("windows executed its own exception"),
    }
    assert!(browser.load(BOTH).is_ok());
    assert!(linux.load(BOTH).is_err());
    assert!(windows.load(BOTH).is_err());

    let headless = Resolver::builder().build().unwrap();
    match headless.load(WINDOWS_ONLY) {
        Err(error) => assert_eq!(error.code, "GY-RESOLVER-PLATFORM"),
        Ok(_) => panic!("headless accepted a platform exception"),
    }
}

#[test]
fn present_rejects_a_headless_instance_and_a_foreign_application() {
    let linux = LinuxHost::open().unwrap();
    let headless = Resolver::builder().build().unwrap();
    let application = headless.load(LIVE).unwrap();
    assert_eq!(application.target(), None);
    match linux.instantiate(&application) {
        Err(error) => {
            assert_eq!(error.code, "GY-RESOLVER-PLATFORM");
            assert!(error.message.contains("does not match linux"));
        }
        Ok(_) => panic!("linux host took a headless application"),
    }
    let mut instance = headless.instantiate(&application).unwrap();
    instance.start().unwrap();
    match linux.present(&mut instance) {
        Err(error) => assert_eq!(error.code, "GY-RESOLVER-PLATFORM"),
        Ok(_) => panic!("presented a headless instance"),
    }
    assert_eq!(instance.phase().as_str(), "running");
}

#[test]
fn present_uses_live_text_and_keeps_the_surface_closed() {
    let linux = LinuxHost::open().unwrap();
    let windows = WindowsHost::open().unwrap();
    let application = linux.load(LIVE).unwrap();
    let mut instance = linux.instantiate(&application).unwrap();
    instance.set_field("window.amount", "4").unwrap();
    instance.start().unwrap();
    instance
        .dispatch(&Event::Activate {
            action: "action.add".into(),
        })
        .unwrap();
    assert_eq!(instance.layout_generations(), 0);
    let surface = linux.present(&mut instance).unwrap();
    assert!(!surface.opened);
    assert_eq!(instance.layout_generations(), 1);
    let again = linux.present(&mut instance).unwrap();
    assert_eq!(again.nodes, surface.nodes);
    assert_eq!(instance.layout_generations(), 1);
    let amount = surface
        .nodes
        .iter()
        .find(|node| node.id.as_deref() == Some("window.amount"))
        .unwrap();
    assert_eq!(amount.role, "input");
    assert_eq!(amount.text, "4");
    assert_eq!(amount.x_mp, 50_000);
    let total = surface
        .nodes
        .iter()
        .find(|node| node.id.as_deref() == Some("window.total"))
        .unwrap();
    assert_eq!(total.text, "4");

    let other = windows.load(LIVE).unwrap();
    let mut instance = windows.instantiate(&other).unwrap();
    instance.set_field("window.amount", "4").unwrap();
    instance.start().unwrap();
    instance
        .dispatch(&Event::Activate {
            action: "action.add".into(),
        })
        .unwrap();
    let projected = windows.present(&mut instance).unwrap();
    assert_eq!(projected.surface, "win32");
    assert!(!projected.opened);
    assert_eq!(projected.nodes, surface.nodes);
    instance.terminate().unwrap();
    match windows.present(&mut instance) {
        Err(error) => {
            assert_eq!(error.code, "GY-RESOLVER-PHASE");
            assert_eq!(error.message, "cannot present while terminated");
        }
        Ok(_) => panic!("presented a terminated instance"),
    }
}

#[test]
fn reload_keeps_the_selected_target() {
    let linux = LinuxHost::open().unwrap();
    let application = linux.load(WINDOWS_ONLY).unwrap();
    let mut instance = linux.instantiate(&application).unwrap();
    instance.start().unwrap();
    instance.reload(WINDOWS_ONLY).expect("same scene");
    assert_eq!(instance.target(), Some(Target::Linux));
    match instance.reload(BOTH) {
        Err(error) => assert_eq!(error.code, "GY-RESOLVER-PLATFORM"),
        Ok(_) => panic!("reload accepted a linux exception"),
    }
    assert_eq!(instance.target(), Some(Target::Linux));
    assert_eq!(instance.phase().as_str(), "running");
    assert_eq!(instance.revision(), 0);
}

#[test]
fn a_point_activates_a_button_and_focuses_an_input() {
    let linux = LinuxHost::open().unwrap();
    let windows = WindowsHost::open().unwrap();
    let application = linux.load(LIVE).unwrap();
    let mut instance = linux.instantiate(&application).unwrap();
    match linux.point(&mut instance, 0, 0) {
        Err(error) => assert_eq!(error.code, "GY-RESOLVER-PHASE"),
        Ok(_) => panic!("pointed before start"),
    }
    assert_eq!(instance.layout_generations(), 0);
    instance.start().unwrap();
    let surface = linux.present(&mut instance).unwrap();
    let total = node(&surface.nodes, "window.total");
    let amount = node(&surface.nodes, "window.amount");
    let add = node(&surface.nodes, "window.add");
    assert!(matches!(
        linux
            .point(&mut instance, center(total).0, center(total).1)
            .unwrap(),
        PointOutcome::Miss
    ));
    assert_eq!(instance.revision(), 0);
    match linux
        .point(&mut instance, center(amount).0, center(amount).1)
        .unwrap()
    {
        PointOutcome::Focus { name, id } => {
            assert_eq!(name, "amount");
            assert_eq!(id.as_deref(), Some("window.amount"));
        }
        other => panic!("expected focus, got {other:?}"),
    }
    assert_eq!(instance.revision(), 0);
    match linux
        .point(&mut instance, center(add).0, center(add).1)
        .unwrap()
    {
        PointOutcome::Activate { name, outcome } => {
            assert_eq!(name, "add");
            assert_eq!(outcome.revision, 1);
            assert_eq!(outcome.affected_elements, ["window.total"]);
        }
        other => panic!("expected activate, got {other:?}"),
    }
    assert_eq!(instance.layout_generations(), 1);
    assert_eq!(instance.state()[0].value, InteractionValue::Int(1));
    assert!(matches!(
        linux.point(&mut instance, -1, -1).unwrap(),
        PointOutcome::Miss
    ));
    instance.suspend().unwrap();
    match linux.point(&mut instance, center(add).0, center(add).1) {
        Err(error) => assert_eq!(error.message, "cannot point while suspended"),
        Ok(_) => panic!("pointed while suspended"),
    }
    assert_eq!(instance.revision(), 1);

    let other = windows.load(LIVE).unwrap();
    let mut instance = windows.instantiate(&other).unwrap();
    instance.start().unwrap();
    match windows
        .point(&mut instance, center(add).0, center(add).1)
        .unwrap()
    {
        PointOutcome::Activate { name, outcome } => {
            assert_eq!(name, "add");
            assert_eq!(outcome.revision, 1);
        }
        other => panic!("expected activate, got {other:?}"),
    }
}

#[test]
fn enter_replaces_the_focused_input() {
    let linux = LinuxHost::open().unwrap();
    let windows = WindowsHost::open().unwrap();
    let application = linux.load(LIVE).unwrap();
    let mut early = linux.instantiate(&application).unwrap();
    match linux.enter(&mut early, "4") {
        Err(error) => assert_eq!(error.message, "cannot enter text while initialising"),
        Ok(_) => panic!("entered before start"),
    }
    assert_eq!(early.layout_generations(), 0);

    let mut instance = linux.instantiate(&application).unwrap();
    instance.start().unwrap();
    let surface = linux.present(&mut instance).unwrap();
    let amount = center(node(&surface.nodes, "window.amount"));
    let add = center(node(&surface.nodes, "window.add"));
    let total = center(node(&surface.nodes, "window.total"));
    assert_eq!(instance.layout_generations(), 1);

    match linux.enter(&mut instance, "4") {
        Err(error) => {
            assert_eq!(error.code, "GY-RESOLVER-FIELD");
            assert_eq!(error.message, "no input is focused");
        }
        Ok(_) => panic!("entered without focus"),
    }
    assert_eq!(instance.field("window.amount"), Some("1"));

    match linux.point(&mut instance, amount.0, amount.1).unwrap() {
        PointOutcome::Focus { name, id } => {
            assert_eq!(name, "amount");
            assert_eq!(id.as_deref(), Some("window.amount"));
        }
        other => panic!("expected focus, got {other:?}"),
    }
    assert_eq!(linux.enter(&mut instance, "4").unwrap(), "window.amount");
    assert_eq!(instance.field("window.amount"), Some("4"));
    assert_eq!(instance.revision(), 0);
    assert_eq!(instance.layout_generations(), 1);
    let projected = linux.present(&mut instance).unwrap();
    assert!(!projected.opened);
    assert_eq!(node(&projected.nodes, "window.amount").text, "4");
    assert_eq!(instance.layout_generations(), 1);

    match linux.point(&mut instance, add.0, add.1).unwrap() {
        PointOutcome::Activate { name, outcome } => {
            assert_eq!(name, "add");
            assert_eq!(outcome.revision, 1);
            assert_eq!(outcome.affected_elements, ["window.total"]);
        }
        other => panic!("expected activate, got {other:?}"),
    }
    assert_eq!(instance.state()[0].value, InteractionValue::Int(4));
    match linux.enter(&mut instance, "9") {
        Err(error) => assert_eq!(error.message, "no input is focused"),
        Ok(_) => panic!("button activation left the input focused"),
    }
    assert_eq!(instance.field("window.amount"), Some("4"));

    linux.point(&mut instance, amount.0, amount.1).unwrap();
    assert_eq!(linux.enter(&mut instance, "x").unwrap(), "window.amount");
    match linux.point(&mut instance, add.0, add.1) {
        Err(error) => assert_eq!(error.code, "GY-RESOLVER-FIELD"),
        Ok(_) => panic!("non-canonical integer published"),
    }
    assert_eq!(instance.revision(), 1);
    assert_eq!(instance.state()[0].value, InteractionValue::Int(4));
    assert_eq!(linux.enter(&mut instance, "2").unwrap(), "window.amount");

    linux.point(&mut instance, total.0, total.1).unwrap();
    match linux.enter(&mut instance, "8") {
        Err(error) => assert_eq!(error.message, "no input is focused"),
        Ok(_) => panic!("a cell kept focus"),
    }

    linux.point(&mut instance, amount.0, amount.1).unwrap();
    instance.suspend().unwrap();
    match linux.enter(&mut instance, "7") {
        Err(error) => assert_eq!(error.message, "cannot enter text while suspended"),
        Ok(_) => panic!("entered while suspended"),
    }
    instance.resume().unwrap();
    assert_eq!(linux.enter(&mut instance, "7").unwrap(), "window.amount");
    assert_eq!(instance.field("window.amount"), Some("7"));
    instance.reload(LIVE).unwrap();
    assert_eq!(linux.enter(&mut instance, "8").unwrap(), "window.amount");
    assert_eq!(instance.revision(), 1);

    let other = windows.load(LIVE).unwrap();
    let mut windows_instance = windows.instantiate(&other).unwrap();
    windows_instance.start().unwrap();
    windows
        .point(&mut windows_instance, amount.0, amount.1)
        .unwrap();
    assert_eq!(
        windows.enter(&mut windows_instance, "3").unwrap(),
        "window.amount"
    );
    match windows.enter(&mut instance, "3") {
        Err(error) => {
            assert_eq!(error.code, "GY-RESOLVER-PLATFORM");
            assert_eq!(error.message, "instance target does not match windows");
        }
        Ok(_) => panic!("windows host edited a linux instance"),
    }
    assert_eq!(instance.field("window.amount"), Some("8"));
}

#[test]
fn scroll_moves_a_clipped_button_into_the_hit_test() {
    let linux = LinuxHost::open().unwrap();
    let windows = WindowsHost::open().unwrap();
    let application = linux.load(SCROLLED).unwrap();
    let mut early = linux.instantiate(&application).unwrap();
    match linux.scroll(&mut early, 150_000, 150_000, 0, 1) {
        Err(error) => assert_eq!(error.message, "cannot scroll while initialising"),
        Ok(_) => panic!("scrolled before start"),
    }
    assert_eq!(early.layout_generations(), 0);

    let mut instance = linux.instantiate(&application).unwrap();
    instance.start().unwrap();
    assert!(matches!(
        linux.point(&mut instance, 210_000, 325_000).unwrap(),
        PointOutcome::Miss
    ));
    assert!(matches!(
        linux.point(&mut instance, 150_000, 175_000).unwrap(),
        PointOutcome::Miss
    ));
    assert_eq!(instance.revision(), 0);
    assert_eq!(instance.layout_generations(), 1);

    match linux
        .scroll(&mut instance, 150_000, 150_000, 20_000, 200_000)
        .unwrap()
    {
        ScrollOutcome::Moved { name, x_mp, y_mp } => {
            assert_eq!(name, "viewport");
            assert_eq!((x_mp, y_mp), (0, 130_000));
        }
        ScrollOutcome::Miss => panic!("viewport did not scroll"),
    }
    assert_eq!(instance.revision(), 0);
    assert_eq!(instance.layout_generations(), 1);
    let surface = linux.present(&mut instance).unwrap();
    assert!(!surface.opened);
    let action = node(&surface.nodes, "action.button");
    assert_eq!(action.y_mp, 170_000);
    assert_eq!(action.clip, Some((100_000, 100_000, 300_000, 120_000)));
    assert_eq!(instance.layout_generations(), 1);

    match linux.point(&mut instance, 150_000, 175_000).unwrap() {
        PointOutcome::Activate { name, outcome } => {
            assert_eq!(name, "action");
            assert_eq!(outcome.revision, 1);
            assert!(outcome.affected_elements.is_empty());
        }
        other => panic!("expected activate, got {other:?}"),
    }
    assert!(matches!(
        linux.point(&mut instance, 150_000, 225_000).unwrap(),
        PointOutcome::Miss
    ));
    assert!(matches!(
        linux.point(&mut instance, 210_000, 325_000).unwrap(),
        PointOutcome::Miss
    ));
    assert_eq!(instance.revision(), 1);
    assert_eq!(instance.state()[0].value, InteractionValue::Int(1));

    match linux
        .scroll(&mut instance, 150_000, 150_000, 0, -200_000)
        .unwrap()
    {
        ScrollOutcome::Moved { y_mp, .. } => assert_eq!(y_mp, 0),
        ScrollOutcome::Miss => panic!("scroll back missed"),
    }
    assert!(matches!(
        linux.point(&mut instance, 150_000, 175_000).unwrap(),
        PointOutcome::Miss
    ));

    linux
        .scroll(&mut instance, 150_000, 150_000, 0, 200_000)
        .unwrap();
    instance.suspend().unwrap();
    match linux.scroll(&mut instance, 150_000, 150_000, 0, -1) {
        Err(error) => assert_eq!(error.message, "cannot scroll while suspended"),
        Ok(_) => panic!("scrolled while suspended"),
    }
    instance.resume().unwrap();
    let surface = linux.present(&mut instance).unwrap();
    assert_eq!(node(&surface.nodes, "action.button").y_mp, 170_000);

    instance.reload(SCROLLED).unwrap();
    let surface = linux.present(&mut instance).unwrap();
    assert_eq!(node(&surface.nodes, "action.button").y_mp, 170_000);
    assert_eq!(instance.revision(), 1);
    assert_eq!(instance.layout_generations(), 2);

    let plain = SCROLLED.replace("scroll: true;", "");
    instance.reload(&plain).unwrap();
    let surface = linux.present(&mut instance).unwrap();
    assert_eq!(node(&surface.nodes, "action.button").y_mp, 220_000);
    assert_eq!(node(&surface.nodes, "action.button").clip, None);
    assert!(matches!(
        linux
            .scroll(&mut instance, 150_000, 150_000, 0, 10)
            .unwrap(),
        ScrollOutcome::Miss
    ));

    let other = windows.load(SCROLLED).unwrap();
    let mut windows_instance = windows.instantiate(&other).unwrap();
    windows_instance.start().unwrap();
    match windows
        .scroll(&mut windows_instance, 150_000, 150_000, 0, 200_000)
        .unwrap()
    {
        ScrollOutcome::Moved { name, y_mp, .. } => {
            assert_eq!(name, "viewport");
            assert_eq!(y_mp, 130_000);
        }
        ScrollOutcome::Miss => panic!("windows viewport did not scroll"),
    }
    match windows.scroll(&mut instance, 0, 0, 0, 1) {
        Err(error) => {
            assert_eq!(error.code, "GY-RESOLVER-PLATFORM");
            assert_eq!(error.message, "instance target does not match windows");
        }
        Ok(_) => panic!("windows host scrolled a linux instance"),
    }
}

#[test]
fn the_caret_inserts_and_deletes_graphemes() {
    let linux = LinuxHost::open().unwrap();
    let windows = WindowsHost::open().unwrap();
    let application = linux.load(LIVE).unwrap();
    let mut early = linux.instantiate(&application).unwrap();
    match linux.edit(&mut early, Edit::Insert("9".into())) {
        Err(error) => assert_eq!(error.message, "cannot edit while initialising"),
        Ok(_) => panic!("edited before start"),
    }
    assert_eq!(early.layout_generations(), 0);

    let mut instance = linux.instantiate(&application).unwrap();
    instance.start().unwrap();
    let surface = linux.present(&mut instance).unwrap();
    let amount = center(node(&surface.nodes, "window.amount"));
    let add = center(node(&surface.nodes, "window.add"));
    assert!(node(&surface.nodes, "window.amount").caret.is_none());

    match linux.edit(&mut instance, Edit::Move(CaretMotion::Home)) {
        Err(error) => assert_eq!(error.message, "no input is focused"),
        Ok(_) => panic!("moved without focus"),
    }
    linux.point(&mut instance, amount.0, amount.1).unwrap();
    assert_eq!(linux.enter(&mut instance, "4").unwrap(), "window.amount");
    let surface = linux.present(&mut instance).unwrap();
    assert_eq!(node(&surface.nodes, "window.amount").text, "4");
    assert_eq!(node(&surface.nodes, "window.amount").caret, Some(1));
    assert_eq!(instance.layout_generations(), 1);
    assert_eq!(instance.revision(), 0);

    assert_eq!(
        linux
            .edit(&mut instance, Edit::Move(CaretMotion::Home))
            .unwrap()
            .byte,
        0
    );
    linux.edit(&mut instance, Edit::Delete).unwrap();
    assert_eq!(instance.field("window.amount"), Some(""));
    let place = linux
        .edit(&mut instance, Edit::Insert("🙂".into()))
        .unwrap();
    assert_eq!(place.byte, "🙂".len());
    assert_eq!(
        linux
            .edit(&mut instance, Edit::Move(CaretMotion::Left))
            .unwrap()
            .byte,
        0
    );
    let place = linux.edit(&mut instance, Edit::Insert("é".into())).unwrap();
    assert_eq!(instance.field("window.amount"), Some("é🙂"));
    assert_eq!(place.byte, "é".len());
    assert_eq!(
        linux
            .edit(&mut instance, Edit::Move(CaretMotion::Right))
            .unwrap()
            .byte,
        "é🙂".len()
    );
    assert_eq!(
        linux
            .edit(&mut instance, Edit::Move(CaretMotion::Left))
            .unwrap()
            .byte,
        "é".len()
    );
    linux.edit(&mut instance, Edit::Backspace).unwrap();
    assert_eq!(instance.field("window.amount"), Some("🙂"));
    linux
        .edit(&mut instance, Edit::Move(CaretMotion::End))
        .unwrap();
    linux.edit(&mut instance, Edit::Backspace).unwrap();
    assert_eq!(instance.field("window.amount"), Some(""));
    linux.edit(&mut instance, Edit::Insert("9".into())).unwrap();
    assert_eq!(instance.field("window.amount"), Some("9"));

    match linux.point(&mut instance, add.0, add.1).unwrap() {
        PointOutcome::Activate { outcome, .. } => {
            assert_eq!(outcome.revision, 1);
            assert_eq!(outcome.affected_elements, ["window.total"]);
        }
        other => panic!("expected activate, got {other:?}"),
    }
    assert_eq!(instance.state()[0].value, InteractionValue::Int(9));
    match linux.edit(&mut instance, Edit::Backspace) {
        Err(error) => assert_eq!(error.message, "no input is focused"),
        Ok(_) => panic!("button activation left the caret"),
    }

    linux.point(&mut instance, amount.0, amount.1).unwrap();
    instance.suspend().unwrap();
    match linux.edit(&mut instance, Edit::Move(CaretMotion::End)) {
        Err(error) => assert_eq!(error.message, "cannot edit while suspended"),
        Ok(_) => panic!("edited while suspended"),
    }
    instance.resume().unwrap();
    assert_eq!(
        linux
            .edit(&mut instance, Edit::Move(CaretMotion::End))
            .unwrap()
            .byte,
        1
    );
    linux
        .edit(&mut instance, Edit::Move(CaretMotion::Home))
        .unwrap();
    instance.reload(LIVE).unwrap();
    assert_eq!(instance.field("window.amount"), Some("9"));
    assert_eq!(
        linux
            .edit(&mut instance, Edit::Move(CaretMotion::Right))
            .unwrap()
            .byte,
        1
    );

    let other = windows.load(LIVE).unwrap();
    let mut windows_instance = windows.instantiate(&other).unwrap();
    windows_instance.start().unwrap();
    windows
        .point(&mut windows_instance, amount.0, amount.1)
        .unwrap();
    let place = windows
        .edit(&mut windows_instance, Edit::Insert("4".into()))
        .unwrap();
    assert_eq!(place.byte, 2);
    assert_eq!(windows_instance.field("window.amount"), Some("14"));
    match windows.edit(&mut instance, Edit::Insert("4".into())) {
        Err(error) => {
            assert_eq!(error.code, "GY-RESOLVER-PLATFORM");
            assert_eq!(error.message, "instance target does not match windows");
        }
        Ok(_) => panic!("windows host edited a linux instance"),
    }
    assert_eq!(instance.field("window.amount"), Some("9"));
}

fn node<'a>(
    nodes: &'a [goya_resolver::ProjectedNode],
    id: &str,
) -> &'a goya_resolver::ProjectedNode {
    nodes
        .iter()
        .find(|node| node.id.as_deref() == Some(id))
        .expect(id)
}

fn center(node: &goya_resolver::ProjectedNode) -> (i64, i64) {
    (
        node.x_mp + node.width_mp / 2,
        node.y_mp + node.height_mp / 2,
    )
}
