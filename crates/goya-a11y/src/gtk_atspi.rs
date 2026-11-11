//! GTK3 accessibility mirror exported through AT-SPI.
//!
//! GTK's ATK bridge publishes the widgets built here to the desktop's AT-SPI
//! registry. This keeps the semantic tree available to Orca and other AT-SPI
//! clients even when the visual Goya surface is rendered by a custom host.

use super::{A11yNode, A11yRole, A11yTree};
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box as GtkBox, Entry, Label, MenuItem, Orientation, Scale,
    Widget,
};
use std::collections::HashMap;
use std::fmt;
use std::sync::mpsc;
use std::time::Duration;

/// User action dispatched from the accessible mirror to the host runtime.
#[derive(Debug, Clone, PartialEq)]
pub enum AtAction {
    /// Activate a button or menu item by its stable Goya node id.
    Press,
    /// Set a text or numeric value.
    SetValue(String),
}

/// Failure to initialize the GTK/AT-SPI mirror.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GtkAtspiError(pub String);

impl fmt::Display for GtkAtspiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for GtkAtspiError {}

/// Publishes a projected Goya interaction tree as an AT-SPI-accessible GTK
/// window. The host receives semantic actions and can route them to Goya's
/// runtime using the stable node ids.
#[derive(Debug, Clone)]
pub struct GtkAtspiBridge {
    tree: A11yTree,
    title: String,
}

impl GtkAtspiBridge {
    pub fn new(tree: A11yTree, title: impl Into<String>) -> Self {
        Self {
            tree,
            title: title.into(),
        }
    }

    /// Run a GTK main loop and expose the semantic tree over AT-SPI.
    ///
    /// This call blocks until the accessible window closes. The supplied
    /// callback routes keyboard/screen-reader actions back to the host. Use
    /// [`run_with_updates`](Self::run_with_updates) to propagate later state
    /// changes without closing and reopening the accessible window.
    pub fn run(&self, on_action: impl Fn(String, AtAction) + 'static) -> Result<(), GtkAtspiError> {
        self.run_inner(None, on_action)
    }

    /// Run the bridge while accepting replacement snapshots from another
    /// thread. Each received tree is projected into the existing window;
    /// callers should send a fresh `A11yTree::from_dig` after state changes.
    /// A bounded sync channel is recommended so stale snapshots cannot build
    /// up when the GTK main loop is busy.
    pub fn run_with_updates(
        &self,
        updates: mpsc::Receiver<A11yTree>,
        on_action: impl Fn(String, AtAction) + 'static,
    ) -> Result<(), GtkAtspiError> {
        self.run_inner(Some(updates), on_action)
    }

    fn run_inner(
        &self,
        updates: Option<mpsc::Receiver<A11yTree>>,
        on_action: impl Fn(String, AtAction) + 'static,
    ) -> Result<(), GtkAtspiError> {
        let app_id = format!("org.goya.a11y.p{}", std::process::id());
        let app = Application::new(Some(&app_id), Default::default());
        let tree = std::rc::Rc::new(std::cell::RefCell::new(self.tree.clone()));
        let updates = std::rc::Rc::new(std::cell::RefCell::new(updates));
        let widgets = std::rc::Rc::new(std::cell::RefCell::new(HashMap::new()));
        let suppress_actions = std::rc::Rc::new(std::cell::Cell::new(false));
        let title = self.title.clone();
        let on_action: std::rc::Rc<dyn Fn(String, AtAction)> = std::rc::Rc::new(on_action);

        app.connect_activate(move |app| {
            let window = ApplicationWindow::new(app);
            window.set_title(&title);
            window.set_default_size(420, 320);
            let content = GtkBox::new(Orientation::Vertical, 8);
            content.set_margin_top(12);
            content.set_margin_bottom(12);
            content.set_margin_start(12);
            content.set_margin_end(12);
            let initial_focus = populate(
                &content,
                &tree.borrow(),
                &on_action,
                &mut widgets.borrow_mut(),
                suppress_actions.clone(),
            );
            window.add(&content);
            window.show_all();
            if let Some(widget) = initial_focus {
                // GTK cannot reliably focus a widget before it is realized.
                // Apply projected focus only after it is visible in the window.
                widget.grab_focus();
            }

            if let Some(updates) = updates.borrow_mut().take() {
                let app_for_updates = app.clone();
                let tree = tree.clone();
                let content = content.clone();
                let on_action = on_action.clone();
                let widgets = widgets.clone();
                let suppress_actions = suppress_actions.clone();
                gtk::glib::timeout_add_local(Duration::from_millis(40), move || {
                    let latest = match take_latest(&updates) {
                        Ok(snapshot) => snapshot,
                        Err(()) => {
                            // 语义 owner 断开后退出镜像，避免留下可交互的过期窗口。
                            app_for_updates.quit();
                            return gtk::glib::ControlFlow::Break;
                        }
                    };
                    if let Some(snapshot) = latest {
                        let mut current_tree = tree.borrow_mut();
                        if same_structure(&current_tree, &snapshot) {
                            suppress_actions.set(true);
                            update_widgets(&current_tree, &snapshot, &widgets.borrow());
                            suppress_actions.set(false);
                            if projected_focus_changed(&current_tree, &snapshot) {
                                if let Some(widget) = snapshot
                                    .iter()
                                    .find(|node| should_restore_focus(node))
                                    .and_then(|node| widgets.borrow().get(&node.id).cloned())
                                {
                                    widget.grab_focus();
                                }
                            }
                        } else {
                            let previous_focus = widgets
                                .borrow()
                                .iter()
                                .find(|(_, widget)| widget.has_focus())
                                .map(|(id, _)| id.clone());
                            let saved = save_entry_state(&widgets.borrow());
                            while let Some(child) = content.children().first() {
                                content.remove(child);
                            }
                            widgets.borrow_mut().clear();
                            *current_tree = snapshot.clone();
                            let focus = populate(
                                &content,
                                &current_tree,
                                &on_action,
                                &mut widgets.borrow_mut(),
                                suppress_actions.clone(),
                            );
                            suppress_actions.set(true);
                            restore_entry_state(&saved, &widgets.borrow());
                            suppress_actions.set(false);
                            content.show_all();
                            let previous = previous_focus
                                .and_then(|id| widgets.borrow().get(&id).cloned())
                                .filter(|widget| widget.is_sensitive() && widget.can_focus());
                            // The snapshot's focused node wins. The previous
                            // widget is only a fallback when this snapshot
                            // marks no enabled interactive focus target.
                            let focus = projected_focus_or_previous(focus, previous);
                            if let Some(widget) = focus {
                                widget.grab_focus();
                            }
                        }
                        *current_tree = snapshot;
                    }
                    gtk::glib::ControlFlow::Continue
                });
            }
        });

        app.run();
        Ok(())
    }
}

fn take_latest(updates: &mpsc::Receiver<A11yTree>) -> Result<Option<A11yTree>, ()> {
    let mut latest = None;
    loop {
        match updates.try_recv() {
            Ok(snapshot) => latest = Some(snapshot),
            Err(mpsc::TryRecvError::Empty) => return Ok(latest),
            Err(mpsc::TryRecvError::Disconnected) => {
                return if latest.is_some() { Ok(latest) } else { Err(()) };
            }
        }
    }
}

fn populate(
    content: &GtkBox,
    tree: &A11yTree,
    on_action: &std::rc::Rc<dyn Fn(String, AtAction)>,
    widgets: &mut HashMap<String, Widget>,
    suppress_actions: std::rc::Rc<std::cell::Cell<bool>>,
) -> Option<Widget> {
    let mut focused = None;
    for root in tree.nodes.values().filter(|node| node.parent.is_none()) {
        let widget = make_node(
            tree,
            root,
            on_action,
            &mut focused,
            widgets,
            suppress_actions.clone(),
        );
        content.pack_start(&widget, false, false, 0);
    }
    focused
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_ddh::{DdhNode, Dig, InteractionState, SemanticAttributes, SemanticRole};

    fn snapshot(label: &str) -> A11yTree {
        let mut dig = Dig::new();
        dig.put(
            DdhNode {
                id: "root".into(),
                revision: 1,
                role: SemanticRole::Window,
                label: label.into(),
                hit_region: goya_change::PixelRect::default(),
                state: InteractionState {
                    enabled: true,
                    visible: true,
                    focused: false,
                },
                actions: vec![],
                exposed: true,
                parent: None,
                semantics: SemanticAttributes::default(),
            },
            1,
        );
        A11yTree::from_dig(&dig)
    }

    #[test]
    fn update_poll_coalesces_to_latest_snapshot_and_stops_when_sender_drops() {
        let (tx, rx) = mpsc::channel();
        tx.send(snapshot("old")).unwrap();
        tx.send(snapshot("new")).unwrap();
        assert_eq!(
            take_latest(&rx)
                .unwrap()
                .unwrap()
                .get("root")
                .unwrap()
                .accessible_name,
            "new"
        );
        assert!(take_latest(&rx).unwrap().is_none());
        drop(tx);
        assert!(take_latest(&rx).is_err());

        let (tx, rx) = mpsc::channel();
        tx.send(snapshot("queued")).unwrap();
        drop(tx);
        assert_eq!(
            take_latest(&rx)
                .unwrap()
                .unwrap()
                .get("root")
                .unwrap()
                .accessible_name,
            "queued"
        );
        assert!(take_latest(&rx).is_err());
    }

    #[test]
    fn rebuild_focus_prefers_the_snapshot_target() {
        assert_eq!(projected_focus_or_previous(Some("next"), Some("old")), Some("next"));
        assert_eq!(projected_focus_or_previous(None, Some("old")), Some("old"));
        assert_eq!(projected_focus_or_previous::<&str>(None, None), None);
    }

    #[test]
    fn projected_focus_skips_disabled_and_noninteractive_nodes() {
        let mut node = A11yNode {
            id: "submit".into(),
            role: A11yRole::Button,
            accessible_name: "Submit".into(),
            enabled: true,
            focused: true,
            editable: false,
            focusable: true,
            hit_region: super::super::A11yRect::default(),
            parent: None,
            live_text: None,
            semantics: super::super::SemanticAttributes::default(),
        };
        assert!(should_restore_focus(&node));

        node.enabled = false;
        assert!(!should_restore_focus(&node));
        node.enabled = true;
        node.role = A11yRole::StaticText;
        assert!(!should_restore_focus(&node));
        node.role = A11yRole::Button;
        node.focused = false;
        assert!(!should_restore_focus(&node));
    }

    #[test]
    fn stable_structure_accepts_state_and_name_changes_without_rebuilding() {
        let old = snapshot("Old window title");
        let updated = snapshot("Updated window title");
        assert!(same_structure(&old, &updated));
        assert!(!projected_focus_changed(&old, &updated));
    }

    #[test]
    fn structure_change_requires_reconciliation() {
        let old = snapshot("Window");
        let mut dig = Dig::new();
        dig.put(
            DdhNode {
                id: "root".into(),
                revision: 2,
                role: SemanticRole::Window,
                label: "Window".into(),
                hit_region: goya_change::PixelRect::default(),
                state: InteractionState {
                    enabled: true,
                    visible: true,
                    focused: false,
                },
                actions: vec![],
                exposed: true,
                parent: None,
                semantics: SemanticAttributes::default(),
            },
            2,
        );
        dig.put(
            DdhNode {
                id: "child".into(),
                revision: 2,
                role: SemanticRole::Button,
                label: "Go".into(),
                hit_region: goya_change::PixelRect::default(),
                state: InteractionState {
                    enabled: true,
                    visible: true,
                    focused: false,
                },
                actions: vec!["press".into()],
                exposed: true,
                parent: Some("root".into()),
                semantics: SemanticAttributes::default(),
            },
            2,
        );
        assert!(!same_structure(&old, &A11yTree::from_dig(&dig)));
    }

    #[test]
    fn slider_configuration_uses_validated_semantics_and_clamps_value() {
        let range = goya_ddh::NumericRange::new(-10.0, 10.0, 0.5).unwrap();
        let mut node = A11yNode {
            id: "volume".into(),
            role: A11yRole::Slider,
            accessible_name: "Volume".into(),
            enabled: true,
            focused: false,
            editable: false,
            focusable: true,
            hit_region: super::super::A11yRect::default(),
            parent: None,
            live_text: None,
            semantics: super::super::SemanticAttributes {
                value: Some("12".into()),
                range: Some(range),
                ..Default::default()
            },
        };
        assert_eq!(slider_config(&node), Some((-10.0, 10.0, 0.5, 10.0)));
        node.semantics.value = Some("NaN".into());
        assert_eq!(slider_config(&node), Some((-10.0, 10.0, 0.5, -10.0)));
        node.semantics.range = None;
        assert_eq!(slider_config(&node), None);
    }
}

fn make_node(
    tree: &A11yTree,
    node: &A11yNode,
    on_action: &std::rc::Rc<dyn Fn(String, AtAction)>,
    focused: &mut Option<Widget>,
    widgets: &mut HashMap<String, Widget>,
    suppress_actions: std::rc::Rc<std::cell::Cell<bool>>,
) -> Widget {
    let id = node.id.clone();
    let child_nodes: Vec<_> = tree
        .nodes
        .values()
        .filter(|child| child.parent.as_deref() == Some(node.id.as_str()))
        .collect();

    let widget: Widget = match node.role {
        A11yRole::Window | A11yRole::Group => {
            let panel = GtkBox::new(Orientation::Vertical, 6);
            for child in child_nodes {
                let child = make_node(
                    tree,
                    child,
                    on_action,
                    focused,
                    widgets,
                    suppress_actions.clone(),
                );
                panel.pack_start(&child, false, false, 0);
            }
            panel.upcast()
        }
        A11yRole::StaticText => Label::new(Some(
            node.semantics
                .value
                .as_deref()
                .unwrap_or(&node.accessible_name),
        ))
        .upcast(),
        A11yRole::Button => {
            let button: Widget = if node.semantics.selected.is_some() {
                let button = gtk::ToggleButton::with_label(&node.accessible_name);
                button.set_active(node.semantics.selected.unwrap_or(false));
                button.upcast()
            } else {
                gtk::Button::with_label(&node.accessible_name).upcast()
            };
            let action = on_action.clone();
            let action_id = id.clone();
            if let Some(button) = button.downcast_ref::<gtk::Button>() {
                button.connect_clicked(move |_| action(action_id.clone(), AtAction::Press));
            }
            button
        }
        A11yRole::TextInput => {
            let entry = Entry::new();
            entry.set_placeholder_text(Some(&node.accessible_name));
            // 输入实时值来自同一语义快照；仅看通用 value 会漏掉 composer.live_text。
            entry.set_text(text_input_value(node));
            let action = on_action.clone();
            let action_id = id.clone();
            let suppress = suppress_actions.clone();
            entry.connect_changed(move |entry| {
                if suppress.get() {
                    return;
                }
                action(
                    action_id.clone(),
                    AtAction::SetValue(entry.text().to_string()),
                )
            });
            entry.upcast()
        }
        A11yRole::Slider => {
            let config = slider_config(node);
            let (min, max, step, value) = config.unwrap_or((0.0, 1.0, 0.1, 0.0));
            let scale = Scale::with_range(Orientation::Horizontal, min, max, step);
            if config.is_some() {
                scale.set_value(value);
            } else {
                // A slider without a declared bounded range cannot truthfully
                // accept a value; keep it visible while disabling interaction.
                scale.set_sensitive(false);
            }
            scale.set_draw_value(true);
            let action = on_action.clone();
            let action_id = id.clone();
            let suppress = suppress_actions.clone();
            scale.connect_value_changed(move |scale| {
                if suppress.get() {
                    return;
                }
                action(
                    action_id.clone(),
                    AtAction::SetValue(scale.value().to_string()),
                )
            });
            scale.upcast()
        }
        A11yRole::MenuItem => {
            let item = MenuItem::with_label(&node.accessible_name);
            let action = on_action.clone();
            let action_id = id.clone();
            item.connect_activate(move |_| action(action_id.clone(), AtAction::Press));
            item.upcast()
        }
    };

    widget.set_widget_name(&node.id);
    widget.set_sensitive(
        node.enabled && (node.role != A11yRole::Slider || node.semantics.range.is_some()),
    );
    if node.role.is_interactive() {
        widget.set_can_focus(true);
    }
    // GTK controls derive their accessible name from their label or
    // placeholder. Set an explicit ATK name as well for scales and groups.
    if let Some(accessible) = widget.accessible() {
        accessible.set_name(&node.accessible_name);
        if let Some(description) = node.semantics.description.as_deref() {
            accessible.set_description(description);
        }
    }
    if should_restore_focus(node) {
        *focused = Some(widget.clone());
    }
    widgets.insert(node.id.clone(), widget.clone());
    widget
}

fn same_structure(old: &A11yTree, new: &A11yTree) -> bool {
    old.nodes.len() == new.nodes.len()
        && old.nodes.iter().all(|(id, node)| {
            new.nodes.get(id).is_some_and(|other| {
                node.role == other.role
                    && node.parent == other.parent
                    && node.semantics.selected.is_some() == other.semantics.selected.is_some()
            })
        })
}

fn projected_focus_or_previous<T>(projected: Option<T>, previous: Option<T>) -> Option<T> {
    projected.or(previous)
}

fn projected_focus_changed(old: &A11yTree, new: &A11yTree) -> bool {
    let focused = |tree: &A11yTree| {
        tree.iter()
            .find(|node| should_restore_focus(node))
            .map(|node| node.id.clone())
    };
    focused(old) != focused(new)
}

fn text_input_value(node: &A11yNode) -> &str {
    node.live_text
        .as_deref()
        .or(node.semantics.value.as_deref())
        .unwrap_or("")
}

fn update_widgets(old: &A11yTree, tree: &A11yTree, widgets: &HashMap<String, Widget>) {
    for node in tree.iter() {
        let Some(widget) = widgets.get(&node.id) else {
            continue;
        };
        widget.set_sensitive(
            node.enabled && (node.role != A11yRole::Slider || node.semantics.range.is_some()),
        );
        if let Some(accessible) = widget.accessible() {
            accessible.set_name(&node.accessible_name);
            accessible.set_description(node.semantics.description.as_deref().unwrap_or(""));
        }
        match node.role {
            A11yRole::StaticText => {
                if let Some(label) = widget.downcast_ref::<Label>() {
                    label.set_text(
                        node.semantics
                            .value
                            .as_deref()
                            .unwrap_or(&node.accessible_name),
                    );
                }
            }
            A11yRole::Button => {
                if let Some(button) = widget.downcast_ref::<gtk::ToggleButton>() {
                    button.set_label(&node.accessible_name);
                    if let Some(selected) = node.semantics.selected {
                        button.set_active(selected);
                    }
                } else if let Some(button) = widget.downcast_ref::<gtk::Button>() {
                    button.set_label(&node.accessible_name);
                }
            }
            A11yRole::TextInput => {
                if let Some(entry) = widget.downcast_ref::<Entry>() {
                    entry.set_placeholder_text(Some(&node.accessible_name));
                    let old_value = old.get(&node.id).map(text_input_value).unwrap_or("");
                    let value = text_input_value(node);
                    if value != old_value {
                        let position = entry.position();
                        let selection = entry.selection_bounds();
                        entry.set_text(value);
                        let length = value.chars().count() as i32;
                        if let Some((start, end)) = selection {
                            entry.select_region(start.min(length), end.min(length));
                        } else {
                            entry.set_position(position.min(length));
                        }
                    }
                }
            }
            A11yRole::MenuItem => {
                if let Some(item) = widget.downcast_ref::<MenuItem>() {
                    item.set_label(&node.accessible_name);
                }
            }
            A11yRole::Slider => {
                if let Some(scale) = widget.downcast_ref::<Scale>() {
                    if let Some((min, max, step, value)) = slider_config(node) {
                        scale.set_range(min, max);
                        scale.set_increments(step, step * 10.0);
                        scale.set_value(value);
                    }
                }
            }
            A11yRole::Window | A11yRole::Group => {}
        }
    }
}

fn slider_config(node: &A11yNode) -> Option<(f64, f64, f64, f64)> {
    let range = node.semantics.range?;
    let value = node
        .semantics
        .value
        .as_deref()
        .and_then(|value| value.parse::<f64>().ok())
        .and_then(|value| range.clamp(value))
        .unwrap_or(range.minimum);
    Some((range.minimum, range.maximum, range.step, value))
}

#[derive(Debug)]
struct SavedEntryState {
    text: String,
    position: i32,
    selection: Option<(i32, i32)>,
}

fn save_entry_state(widgets: &HashMap<String, Widget>) -> HashMap<String, SavedEntryState> {
    widgets
        .iter()
        .filter_map(|(id, widget)| {
            widget.downcast_ref::<Entry>().map(|entry| {
                (
                    id.clone(),
                    SavedEntryState {
                        text: entry.text().to_string(),
                        position: entry.position(),
                        selection: entry.selection_bounds(),
                    },
                )
            })
        })
        .collect()
}

fn restore_entry_state(
    saved: &HashMap<String, SavedEntryState>,
    widgets: &HashMap<String, Widget>,
) {
    for (id, state) in saved {
        if let Some(entry) = widgets
            .get(id)
            .and_then(|widget| widget.downcast_ref::<Entry>())
        {
            entry.set_text(&state.text);
            if let Some((start, end)) = state.selection {
                entry.select_region(start, end);
            } else {
                entry.set_position(state.position);
            }
        }
    }
}

fn should_restore_focus(node: &A11yNode) -> bool {
    node.focused && node.enabled && node.role.is_interactive()
}
