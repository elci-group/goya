//! A failed replacement leaves the running instance in place.
//!
//! The CLI `--watch` test polls a temporary file. These tests do not measure
//! time or memory.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use goya_lang::InteractionValue;
use goya_resolver::{Diagnostic, Instance, Resolver};

// `label` is not an operand of the add interaction. GSSH/4-live still
// requires every declared key to be used, and a `text-from` binding counts.
const BASE: &str = r#"@ontology app {
  @state {
    count: int = 0 session;
    label: text = "a" session;
  }
  @cell root {
    id: "root";
    text-from: state.label;
    @input amount {
      id: "amount";
      text: "1";
      @iden { label: "Amount"; hint: "n"; }
    }
    @button add {
      id: "add";
      text: "Add";
    }
    @cell total {
      id: "total";
      text: "Count: ";
      text-from: state.count;
    }
  }
  @interaction add {
    id: "action.add";
    trigger: button.add.activate;
    class: D1;
    state { read count: int; write count: int; }
    input { amount: int from "amount"; }
    set count = state.count + input.amount;
  }
}
"#;

const WITH_EXTRA: &str = r#"@ontology app {
  @state {
    count: int = 0 session;
    label: text = "a" session;
    extra: int = 7 session;
  }
  @cell root {
    id: "root";
    text-from: state.label;
    @input amount {
      id: "amount";
      text: "1";
      @iden { label: "Amount"; hint: "n"; }
    }
    @button add {
      id: "add";
      text: "Add";
    }
    @cell total {
      id: "total";
      text: "Count: ";
      text-from: state.count;
    }
    @cell extra {
      id: "extra";
      text-from: state.extra;
    }
  }
  @interaction add {
    id: "action.add";
    trigger: button.add.activate;
    class: D1;
    state { read count: int; write count: int; }
    input { amount: int from "amount"; }
    set count = state.count + input.amount;
  }
}
"#;

const AS_TEXT: &str = r#"@ontology app {
  @state {
    count: text = "z" session;
    label: text = "a" session;
  }
  @cell root {
    id: "root";
    text-from: state.label;
    @input amount {
      id: "amount";
      text: "1";
      @iden { label: "Amount"; hint: "n"; }
    }
    @button add {
      id: "add";
      text: "Add";
    }
    @cell total {
      id: "total";
      text: "Count: ";
      text-from: state.count;
    }
  }
}
"#;

const D2: &str = r#"@ontology app {
  @state { count: int = 0 session; }
  @cell root {
    id: "root";
    @button go { id: "go"; text: "Go"; }
  }
  @interaction fetch {
    id: "action.fetch";
    trigger: button.go.activate;
    class: D2;
    effects { network; }
    requests { lookup: effect network capability "network" request "static request" result int64; }
    state { write count: int; }
    input { }
    set count = effect.lookup;
  }
}
"#;

fn resolver() -> Resolver {
    Resolver::builder()
        .viewport(320.0, 160.0)
        .build()
        .expect("viewport")
}

fn running(source: &str) -> Instance {
    let resolver = resolver();
    let application = resolver.load(source).expect("load");
    assert_eq!(application.semantic_contract(), "GSSH/4-live");
    let mut instance = resolver.instantiate(&application).expect("instance");
    instance.start().expect("start");
    instance
}

fn state_value(instance: &Instance, key: &str) -> InteractionValue {
    instance
        .state()
        .into_iter()
        .find(|entry| entry.key == key)
        .unwrap_or_else(|| panic!("missing state key {key}"))
        .value
}

fn added() -> Instance {
    let mut instance = running(BASE);
    instance.set_field("amount", "1").expect("amount");
    let outcome = instance.activate("action.add").expect("add");
    assert_eq!(outcome.revision, 1);
    assert_eq!(instance.revision(), 1);
    assert_eq!(state_value(&instance, "count"), InteractionValue::Int(1));
    instance
}

fn refused(result: Result<goya_resolver::ReloadReport, Diagnostic>) -> Diagnostic {
    match result {
        Ok(_) => panic!("reload was accepted"),
        Err(error) => error,
    }
}

#[test]
fn reload_keeps_a_compatible_value_and_field() {
    let mut instance = added();
    instance.set_field("amount", "4").expect("amount");
    let total_rect = instance.frame().find("total").expect("total").rect;
    assert_eq!(instance.layout_generations(), 1);

    let report = instance.reload(BASE).expect("reload");
    assert_eq!(instance.revision(), 1);
    assert_eq!(state_value(&instance, "count"), InteractionValue::Int(1));
    assert_eq!(
        state_value(&instance, "label"),
        InteractionValue::Text("a".to_owned())
    );
    assert_eq!(instance.field("amount"), Some("4"));
    assert_eq!(instance.phase().as_str(), "running");
    assert!(report.preserved_keys.iter().any(|key| key == "count"));
    assert!(report.preserved_keys.iter().any(|key| key == "label"));
    assert_eq!(report.preserved_fields, ["amount"]);
    assert_eq!(instance.layout_generations(), 1);

    let total_again = instance.frame().find("total").expect("total").rect;
    assert_eq!(instance.layout_generations(), 2);
    assert_eq!(total_again, total_rect);
}

#[test]
fn a_rejected_reload_does_not_change_the_instance() {
    let mut instance = added();
    assert!(instance.reload("this is not a scene").is_err());
    assert_eq!(instance.revision(), 1);
    assert_eq!(state_value(&instance, "count"), InteractionValue::Int(1));
    assert_eq!(instance.phase().as_str(), "running");

    let error = refused(instance.reload(AS_TEXT));
    assert_eq!(error.code, "GY-RESOLVER-RELOAD");
    assert!(error.message.contains("changed from int to text"));
    let range = error.range.expect("replacement state range");
    let quoted = &AS_TEXT[range.start..range.end];
    assert!(quoted.contains("count:"), "{quoted}");
    assert!(quoted.contains("text"), "{quoted}");
    assert_eq!(state_value(&instance, "count"), InteractionValue::Int(1));
    assert_eq!(instance.revision(), 1);

    let error = refused(instance.reload(D2));
    assert_eq!(error.code, "GY-RESOLVER-D2");
    assert_eq!(state_value(&instance, "count"), InteractionValue::Int(1));
}

#[test]
fn reload_adds_and_drops_keys_without_resetting_revision() {
    let mut instance = added();
    let report = instance.reload(WITH_EXTRA).expect("reload extra");
    assert_eq!(instance.revision(), 1);
    assert_eq!(state_value(&instance, "count"), InteractionValue::Int(1));
    assert_eq!(state_value(&instance, "extra"), InteractionValue::Int(7));
    assert_eq!(report.initialised_keys, ["extra"]);
    assert!(report.preserved_keys.iter().any(|key| key == "count"));

    let report = instance.reload(BASE).expect("reload base");
    assert_eq!(instance.revision(), 1);
    assert_eq!(state_value(&instance, "count"), InteractionValue::Int(1));
    assert_eq!(report.dropped_keys, ["extra"]);
    assert!(instance.state().iter().all(|entry| entry.key != "extra"));
}

#[test]
fn terminated_instance_refuses_reload() {
    let mut instance = running(BASE);
    instance.terminate().expect("terminate");
    let error = refused(instance.reload(BASE));
    assert_eq!(error.code, "GY-RESOLVER-PHASE");
    assert_eq!(instance.phase().as_str(), "terminated");
}

#[test]
fn cli_watch_reloads_bytes_and_keeps_a_rejected_replacement() {
    let bin = env!("CARGO_BIN_EXE_goya-resolve");
    let dir = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let stamp = format!("{}-{nanos}", std::process::id());
    let path = dir.join(format!("goya-resolve-watch-{stamp}.gscene"));
    let tmp = dir.join(format!("goya-resolve-watch-{stamp}.gscene.tmp"));
    fs::write(&path, BASE).expect("seed");
    let mut child = Command::new(bin)
        .args([
            "--target",
            "linux",
            "--dispatch",
            "action.add",
            "--present",
            "--watch",
            path.to_str().expect("utf-8 path"),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    let stdout = line_reader(child.stdout.take().expect("stdout"));
    let stderr = line_reader(child.stderr.take().expect("stderr"));
    let session = WatchSession {
        child: Some(child),
        path,
        tmp,
    };
    let timeout = Duration::from_secs(5);

    let ready = wait_line(&stdout, "present linux wayland closed revision 1", timeout);
    assert!(
        ready.iter().any(|line| line.contains("phase: running")),
        "{ready:?}"
    );
    assert!(
        ready
            .iter()
            .any(|line| line.contains("state count int 1 session")),
        "{ready:?}"
    );
    assert!(
        ready.iter().all(|line| !line.contains("opened")),
        "{ready:?}"
    );

    session.replace(WITH_EXTRA.as_bytes());
    let added = wait_line(
        &stdout,
        "reload revision 1 preserved count,label initialised extra dropped -",
        timeout,
    );
    let presented = wait_line(&stdout, "present linux wayland closed revision 1", timeout);
    assert!(
        presented.iter().all(|line| !line.contains("opened")),
        "{added:?} {presented:?}"
    );

    fs::remove_file(&session.path).expect("delete");
    let missing = wait_line(&stderr, "No such file or directory", timeout);
    assert!(
        missing.iter().any(|line| line.starts_with("goya-resolve:")),
        "{missing:?}"
    );
    settle(&stdout);

    session.replace(b"this is not a scene");
    let rejected = wait_line(&stderr, "GY-E1000", timeout);
    assert!(
        rejected.iter().all(|line| !line.starts_with("reload ")),
        "{rejected:?}"
    );
    settle(&stdout);

    session.replace(&[0xff, 0xfe]);
    let _ = wait_line(&stderr, "replacement is not UTF-8", timeout);
    settle(&stdout);

    session.replace(BASE.as_bytes());
    let restored = wait_line(
        &stdout,
        "reload revision 1 preserved count,label initialised - dropped extra",
        timeout,
    );
    assert!(
        restored
            .iter()
            .all(|line| !line.contains("initialised extra")),
        "{restored:?}"
    );
    let _ = wait_line(&stdout, "present linux wayland closed revision 1", timeout);
}

struct WatchSession {
    child: Option<Child>,
    path: PathBuf,
    tmp: PathBuf,
}

impl WatchSession {
    fn replace(&self, bytes: &[u8]) {
        fs::write(&self.tmp, bytes).expect("write replacement");
        fs::rename(&self.tmp, &self.path).expect("rename replacement");
    }
}

impl Drop for WatchSession {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_file(&self.tmp);
    }
}

fn line_reader<R>(pipe: R) -> Receiver<String>
where
    R: std::io::Read + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut lines = BufReader::new(pipe);
        let mut line = String::new();
        loop {
            line.clear();
            match lines.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let trimmed = line.trim_end_matches(['\n', '\r']).to_owned();
                    if tx.send(trimmed).is_err() {
                        break;
                    }
                }
            }
        }
    });
    rx
}

fn wait_line(rx: &Receiver<String>, needle: &str, timeout: Duration) -> Vec<String> {
    let start = std::time::Instant::now();
    let mut seen = Vec::new();
    loop {
        let remaining = timeout.saturating_sub(start.elapsed());
        if remaining.is_zero() {
            panic!("timed out waiting for {needle:?}; saw {seen:?}");
        }
        match rx.recv_timeout(remaining) {
            Ok(line) => {
                let matched = line.contains(needle);
                seen.push(line);
                if matched {
                    return seen;
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                panic!("timed out waiting for {needle:?}; saw {seen:?}");
            }
            Err(RecvTimeoutError::Disconnected) => {
                panic!("watcher closed while waiting for {needle:?}; saw {seen:?}");
            }
        }
    }
}

fn settle(rx: &Receiver<String>) {
    thread::sleep(Duration::from_millis(200));
    while let Ok(line) = rx.try_recv() {
        assert!(
            !line.starts_with("reload "),
            "a rejected replacement printed {line}"
        );
    }
}
