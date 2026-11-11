//! Bounded semantic snapshot / action carrier for the native AT-SPI helper.
use goya_a11y::{A11yRole, A11yTree};
use goya_ddh::{DdhNode, Dig, InteractionState, SemanticRole};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Condvar, Mutex,
};
use std::thread::JoinHandle;

pub const MAX_SNAPSHOT_BYTES: usize = 1024 * 1024;
const MAX_NODES: usize = 512;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub enum AccessibleAction {
    Press { id: String },
    SetValue { id: String, value: String },
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NodeSnapshot {
    id: String,
    role: String,
    label: String,
    parent: Option<String>,
    enabled: bool,
    focused: bool,
    bounds: [i32; 4],
    live_text: Option<String>,
    value: Option<String>,
    description: Option<String>,
    selected: Option<bool>,
    range: Option<[f64; 3]>,
}

pub fn encode_snapshot(tree: &A11yTree) -> Result<Vec<u8>, String> {
    if tree.len() > MAX_NODES {
        return Err("accessibility snapshot has too many nodes".into());
    }
    let nodes: Vec<_> = tree
        .iter()
        .map(|node| NodeSnapshot {
            id: node.id.clone(),
            role: format!("{:?}", node.role),
            label: node.accessible_name.clone(),
            parent: node.parent.clone(),
            enabled: node.enabled,
            focused: node.focused,
            bounds: [
                node.hit_region.x,
                node.hit_region.y,
                node.hit_region.width,
                node.hit_region.height,
            ],
            live_text: node.live_text.clone(),
            value: node.semantics.value.clone(),
            description: node.semantics.description.clone(),
            selected: node.semantics.selected,
            range: node
                .semantics
                .range
                .map(|range| [range.minimum, range.maximum, range.step]),
        })
        .collect();
    let mut bytes = serde_json::to_vec(&nodes).map_err(|_| "accessibility encoding failed")?;
    if bytes.len() >= MAX_SNAPSHOT_BYTES {
        return Err("accessibility snapshot exceeds byte limit".into());
    }
    bytes.push(b'\n');
    Ok(bytes)
}

pub fn decode_snapshot(bytes: &[u8]) -> Result<A11yTree, String> {
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err("accessibility snapshot exceeds byte limit".into());
    }
    let nodes: Vec<NodeSnapshot> =
        serde_json::from_slice(bytes).map_err(|_| "invalid accessibility snapshot")?;
    if nodes.len() > MAX_NODES {
        return Err("accessibility snapshot has too many nodes".into());
    }
    let mut ids = HashSet::new();
    for node in &nodes {
        if node.id.is_empty()
            || node.id.len() > 1024
            || !ids.insert(node.id.as_str())
            || node.bounds[2] < 0
            || node.bounds[3] < 0
        {
            return Err("invalid accessibility node".into());
        }
    }
    for node in &nodes {
        let mut visited = HashSet::new();
        let mut parent = node.parent.as_deref();
        while let Some(id) = parent {
            if id == node.id || !visited.insert(id) {
                return Err("cyclic accessibility parents".into());
            }
            let ancestor = nodes
                .iter()
                .find(|candidate| candidate.id == id)
                .ok_or("missing accessibility parent")?;
            parent = ancestor.parent.as_deref();
        }
    }
    let mut dig = Dig::new();
    for node in &nodes {
        let role = match node.role.as_str() {
            "Window" => SemanticRole::Window,
            "Group" => SemanticRole::Panel,
            "StaticText" => SemanticRole::Label,
            "Button" => SemanticRole::Button,
            "TextInput" => SemanticRole::Textbox,
            "Slider" => SemanticRole::Slider,
            "MenuItem" => SemanticRole::MenuItem,
            _ => return Err("unknown accessibility role".into()),
        };
        dig.put(
            DdhNode {
                id: node.id.clone(),
                revision: 1,
                role,
                label: node.label.clone(),
                parent: node.parent.clone(),
                hit_region: goya_change::PixelRect {
                    x: node.bounds[0],
                    y: node.bounds[1],
                    width: node.bounds[2],
                    height: node.bounds[3],
                },
                state: InteractionState {
                    enabled: node.enabled,
                    visible: true,
                    focused: node.focused,
                },
                actions: vec![],
                exposed: true,
                semantics: goya_ddh::SemanticAttributes {
                    value: node.value.clone(),
                    description: node.description.clone(),
                    selected: node.selected,
                    range: match node.range {
                        Some([min, max, step]) => Some(
                            goya_ddh::NumericRange::new(min, max, step)
                                .ok_or("invalid accessible range")?,
                        ),
                        None => None,
                    },
                },
            },
            1,
        );
    }
    let mut tree = A11yTree::from_dig(&dig);
    for node in nodes {
        tree.set_live_text(&node.id, node.live_text);
    }
    Ok(tree)
}

type LatestSnapshot = Arc<(Mutex<Option<Vec<u8>>>, Condvar)>;

/// Owns one mirror process and bounded I/O. Display facts remain with the native host.
pub struct AccessibilityBridge {
    child: Child,
    latest: LatestSnapshot,
    stop: Arc<AtomicBool>,
    actions: mpsc::Receiver<AccessibleAction>,
    workers: Vec<JoinHandle<()>>,
}
impl AccessibilityBridge {
    pub fn spawn(executable: &Path) -> Result<Self, String> {
        let mut child = Command::new(executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("accessibility helper unavailable: {error}"))?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or("accessibility helper stdin unavailable")?;
        let stdout = child
            .stdout
            .take()
            .ok_or("accessibility helper stdout unavailable")?;
        let latest: LatestSnapshot = Arc::new((Mutex::new(None), Condvar::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (sender, actions) = mpsc::sync_channel(64);
        let write_latest = Arc::clone(&latest);
        let write_stop = Arc::clone(&stop);
        let writer = std::thread::spawn(move || loop {
            let (slot, wake) = &*write_latest;
            let Ok(mut pending) = slot.lock() else {
                return;
            };
            while pending.is_none() && !write_stop.load(Ordering::Acquire) {
                pending = match wake.wait(pending) {
                    Ok(value) => value,
                    Err(_) => return,
                };
            }
            if write_stop.load(Ordering::Acquire) {
                return;
            }
            let snapshot = pending.take();
            drop(pending);
            if let Some(bytes) = snapshot {
                if stdin.write_all(&bytes).and_then(|_| stdin.flush()).is_err() {
                    write_stop.store(true, Ordering::Release);
                    return;
                }
            }
        });
        let read_stop = Arc::clone(&stop);
        let reader = std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                let result = std::io::Read::take(&mut reader, 65537).read_until(b'\n', &mut line);
                if !matches!(result, Ok(n) if n > 0) || line.len() > 65536 || !line.ends_with(b"\n")
                {
                    break;
                }
                let Ok(action) = serde_json::from_slice::<AccessibleAction>(&line) else {
                    break;
                };
                if sender.try_send(action).is_err() {
                    break;
                }
            }
            read_stop.store(true, Ordering::Release);
        });
        Ok(Self {
            child,
            latest,
            stop,
            actions,
            workers: vec![writer, reader],
        })
    }
    pub fn publish(&self, tree: &A11yTree) -> Result<(), String> {
        if self.stop.load(Ordering::Acquire) {
            return Err("accessibility helper disconnected".into());
        }
        let bytes = encode_snapshot(tree)?;
        *self
            .latest
            .0
            .lock()
            .map_err(|_| "accessibility snapshot lock failed")? = Some(bytes);
        self.latest.1.notify_one();
        Ok(())
    }
    pub fn try_action(&self, current: &A11yTree) -> Option<AccessibleAction> {
        while let Ok(action) = self.actions.try_recv() {
            let id = match &action {
                AccessibleAction::Press { id } | AccessibleAction::SetValue { id, .. } => id,
            };
            let Some(node) = current.get(id) else {
                continue;
            };
            let valid_role = match &action {
                AccessibleAction::Press { .. } => {
                    node.role == A11yRole::Button || node.role == A11yRole::MenuItem
                }
                AccessibleAction::SetValue { .. } => node.editable,
            };
            if node.enabled && valid_role {
                return Some(action);
            }
        }
        None
    }
    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.latest.1.notify_all();
        let _ = self.child.kill();
        let _ = self.child.wait();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}
impl Drop for AccessibilityBridge {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn owned_helper_filters_stale_actions_and_joins_on_shutdown() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-a11y-bridge.py");
        let mut bridge = AccessibilityBridge::spawn(&fixture).unwrap();
        let pid = bridge.child.id();
        let node = |id: &str, enabled: bool| serde_json::json!({"id":id,"role":"Button","label":id,"parent":null,"enabled":enabled,"focused":false,"bounds":[0,0,10,10],"liveText":null});
        let tree = decode_snapshot(
            &serde_json::to_vec(&vec![node("quit", true), node("disabled", false)]).unwrap(),
        )
        .unwrap();
        bridge.publish(&tree).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let action = loop {
            if let Some(action) = bridge.try_action(&tree) {
                break action;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        };
        assert!(matches!(action, AccessibleAction::Press { id } if id == "quit"));
        bridge.shutdown();
        assert!(bridge.child.try_wait().unwrap().is_some());
        #[cfg(target_os = "linux")]
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }

    #[test]
    fn snapshots_reject_duplicates_cycles_missing_parents_and_byte_overflow() {
        let node = |id: &str, parent: Option<&str>| serde_json::json!({"id":id,"role":"TextInput","label":"Message","parent":parent,"enabled":true,"focused":true,"bounds":[1,2,3,4],"liveText":"typed"});
        let snapshot = serde_json::to_vec(&vec![node("input", None)]).unwrap();
        let tree = decode_snapshot(&snapshot).unwrap();
        assert_eq!(tree.get("input").unwrap().resolved_name(), "typed");
        let roundtrip = decode_snapshot(&encode_snapshot(&tree).unwrap()).unwrap();
        assert_eq!(
            roundtrip.iter().collect::<Vec<_>>(),
            tree.iter().collect::<Vec<_>>()
        );
        assert!(decode_snapshot(
            &serde_json::to_vec(&vec![node("a", None), node("a", None)]).unwrap()
        )
        .is_err());
        assert!(decode_snapshot(
            &serde_json::to_vec(&vec![node("a", Some("b")), node("b", Some("a"))]).unwrap()
        )
        .is_err());
        assert!(
            decode_snapshot(&serde_json::to_vec(&vec![node("a", Some("missing"))]).unwrap())
                .is_err()
        );
        assert!(decode_snapshot(&vec![b' '; MAX_SNAPSHOT_BYTES + 1]).is_err());
    }
}
