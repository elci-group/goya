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
            let initial_focus = populate(&content, &tree.borrow(), &on_action);
            window.add(&content);
            window.show_all();
            if let Some(widget) = initial_focus {
                // GTK cannot reliably focus a widget before it is realized.
                // Apply projected focus only after it is visible in the window.
                widget.grab_focus();
            }

            if let Some(updates) = updates.borrow_mut().take() {
                let tree = tree.clone();
                let content = content.clone();
                let on_action = on_action.clone();
                gtk::glib::timeout_add_local(Duration::from_millis(40), move || {
                    let latest = match take_latest(&updates) {
                        Ok(snapshot) => snapshot,
                        Err(()) => return gtk::glib::ControlFlow::Break,
                    };
                    if let Some(snapshot) = latest {
                        *tree.borrow_mut() = snapshot;
                        while let Some(child) = content.children().first() {
                            content.remove(child);
                        }
                        let focus = populate(&content, &tree.borrow(), &on_action);
                        content.show_all();
                        if let Some(widget) = focus {
                            // Rebuilt controls are not focusable until the
                            // replacement snapshot has been shown.
                            widget.grab_focus();
                        }
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
            Err(mpsc::TryRecvError::Disconnected) => return Err(()),
        }
    }
}

fn populate(
    content: &GtkBox,
    tree: &A11yTree,
    on_action: &std::rc::Rc<dyn Fn(String, AtAction)>,
) -> Option<Widget> {
    let mut focused = None;
    for root in tree.nodes.values().filter(|node| node.parent.is_none()) {
        let widget = make_node(tree, root, on_action, &mut focused);
        content.pack_start(&widget, false, false, 0);
    }
    focused
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_ddh::{DdhNode, Dig, InteractionState, SemanticRole};

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
    }
}

fn make_node(
    tree: &A11yTree,
    node: &A11yNode,
    on_action: &std::rc::Rc<dyn Fn(String, AtAction)>,
    focused: &mut Option<Widget>,
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
                let child = make_node(tree, child, on_action, focused);
                panel.pack_start(&child, false, false, 0);
            }
            panel.upcast()
        }
        A11yRole::StaticText => Label::new(Some(&node.accessible_name)).upcast(),
        A11yRole::Button => {
            let button = gtk::Button::with_label(&node.accessible_name);
            let action = on_action.clone();
            let action_id = id.clone();
            button.connect_clicked(move |_| action(action_id.clone(), AtAction::Press));
            button.upcast()
        }
        A11yRole::TextInput => {
            let entry = Entry::new();
            entry.set_placeholder_text(Some(&node.accessible_name));
            let action = on_action.clone();
            let action_id = id.clone();
            entry.connect_changed(move |entry| {
                action(
                    action_id.clone(),
                    AtAction::SetValue(entry.text().to_string()),
                )
            });
            entry.upcast()
        }
        A11yRole::Slider => {
            let scale = Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 1.0);
            scale.set_draw_value(true);
            let action = on_action.clone();
            let action_id = id.clone();
            scale.connect_value_changed(move |scale| {
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
    widget.set_sensitive(node.enabled);
    if node.role.is_interactive() {
        widget.set_can_focus(true);
    }
    // GTK controls derive their accessible name from their label or
    // placeholder. Set an explicit ATK name as well for scales and groups.
    if let Some(accessible) = widget.accessible() {
        accessible.set_name(&node.accessible_name);
    }
    if node.focused && node.enabled && node.role.is_interactive() {
        *focused = Some(widget.clone());
    }
    widget
}
