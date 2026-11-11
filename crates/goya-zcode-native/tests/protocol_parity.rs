use goya_zcode_native::agent::{spawn_agent, MonitorHandle};
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn fake_app_server_hydrates_and_queries_ambiguous_command_without_resending() {
    let workspace = unique_temp_dir();
    fs::create_dir_all(&workspace).unwrap();
    let fake = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-app-server.py");
    let handle = spawn_agent(
        fake.to_str().unwrap(),
        &workspace,
        Some("synthetic-session".into()),
    );
    let _cleanup = Cleanup {
        handle: handle.clone(),
        workspace: workspace.clone(),
    };

    let ready_deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        let ready = {
            let view = handle.view.lock().unwrap();
            view.history
                .iter()
                .any(|line| line == "> Persisted synthetic input")
                || view
                    .tail
                    .iter()
                    .any(|line| line == "> Persisted synthetic input")
        };
        if ready {
            break;
        }
        assert!(
            std::time::Instant::now() < ready_deadline,
            "fake app-server did not hydrate: {:?}",
            handle.view.lock().unwrap().status
        );
        thread::sleep(Duration::from_millis(20));
    }

    assert!(handle.send_text("synthetic input".into(), true));
    let log_path = workspace.join(".fake-app-server.log");
    let send_deadline = std::time::Instant::now() + Duration::from_secs(20);
    let log = loop {
        let log = fs::read_to_string(&log_path).unwrap_or_default();
        if log.contains("v4/commands/query") {
            break log;
        }
        assert!(
            std::time::Instant::now() < send_deadline,
            "fake app-server command query not observed: {log}"
        );
        thread::sleep(Duration::from_millis(20));
    };
    let requests: Vec<serde_json::Value> = log
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let sends: Vec<_> = requests
        .iter()
        .filter(|request| request["method"] == "v4/command")
        .collect();
    let queries: Vec<_> = requests
        .iter()
        .filter(|request| request["method"] == "v4/commands/query")
        .collect();
    assert_eq!(sends.len(), 1, "ambiguous command must never be resent");
    assert_eq!(queries.len(), 1);
    assert_eq!(
        sends[0]["params"]["commandId"],
        queries[0]["params"]["commands"][0]["commandId"]
    );
    assert_eq!(sends[0]["params"]["payload"]["text"], "synthetic input");
    let outcome_deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < outcome_deadline
        && handle.view.lock().unwrap().status != "sending"
    {
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(handle.view.lock().unwrap().status, "sending");
}

struct Cleanup {
    handle: MonitorHandle,
    workspace: PathBuf,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        self.handle.shutdown();
        let _ = fs::remove_dir_all(&self.workspace);
    }
}

fn unique_temp_dir() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("goya-v4-fake-{}-{stamp}", std::process::id()))
}

#[test]
#[cfg(target_os = "linux")]
fn shutdown_cancels_startup_wait_and_reaps_unresponsive_app_server() {
    use std::os::unix::fs::PermissionsExt;
    let workspace = unique_temp_dir();
    fs::create_dir_all(&workspace).unwrap();
    let fake = workspace.join("quiet.py");
    fs::write(&fake, "#!/usr/bin/env python3\nimport os,sys,pathlib\npathlib.Path('child.pid').write_text(str(os.getpid()))\nfor line in sys.stdin: pass\n").unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
    let handle = spawn_agent(fake.to_str().unwrap(), &workspace, None);
    let _cleanup = Cleanup {
        handle: handle.clone(),
        workspace: workspace.clone(),
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    let pid = loop {
        if let Ok(pid) = fs::read_to_string(workspace.join("child.pid")) {
            break pid;
        }
        assert!(std::time::Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    };
    let started = std::time::Instant::now();
    handle.shutdown();
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "shutdown waited for full RPC timeout"
    );
    assert!(
        !std::path::Path::new("/proc").join(pid).exists(),
        "child was not reaped"
    );
}
