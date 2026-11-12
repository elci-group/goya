//! Resize-aware layout for the native ZCode monitor.
//!
//! The `.gscene` file owns visual styles and stable node identities. This
//! adapter resolves its current geometry against the configured surface size,
//! generates the transcript rows that fit, and returns the one frame shared by
//! rendering, hit testing, accessibility, and text-input geometry.

use crate::view::WorkflowControl;
use goya_lang::{layout, Frame, Scene, SceneCell, Vec3};

const HEADER_HEIGHT: f64 = 28.0;
const ROW_TOP: f64 = 32.0;
const ROW_HEIGHT: f64 = 18.0;
const FOOTER_HEIGHT: f64 = 110.0;
const FOOTER_BOTTOM: f64 = 4.0;
const FOOTER_GAP: f64 = 8.0;
/// Prevent a malformed or hostile configure height from allocating unbounded
/// scene nodes. This comfortably exceeds the bounded transcript history.
const MAX_VISIBLE_ROWS: usize = 256;
const WORKFLOW_ACTION_BUTTONS: &[&str] = &[
    "queue_remove_00",
    "queue_remove_01",
    "queue_remove_02",
    "queue_remove_03",
    "permission_allow",
    "permission_deny",
    "permission_option_00",
    "permission_option_01",
    "permission_option_02",
    "permission_option_03",
    "question_option_00",
    "question_option_01",
    "question_option_02",
    "question_option_03",
    "question_submit",
    "stop",
    "goal_pause",
    "goal_resume",
    "compact_session",
    "queue_now_00",
    "queue_now_01",
    "queue_now_02",
    "queue_now_03",
    "queue_earlier_00",
    "queue_earlier_01",
    "queue_earlier_02",
    "queue_earlier_03",
    "work_cancel_00",
    "work_cancel_01",
    "work_cancel_02",
    "work_cancel_03",
];

/// Resolved geometry for one configured surface size.
#[derive(Debug, Clone)]
pub struct MonitorLayout {
    /// Current scene frame; all native hit/render/a11y geometry derives from it.
    pub frame: Frame,
    /// Number of transcript rows represented by the frame.
    pub visible_rows: usize,
    /// Configured surface size in pixels (Goya's current 1 px = 1 pt mapping).
    pub width_px: i32,
    pub height_px: i32,
}

/// Resolve the monitor scene at a compositor-configured pixel size.
///
/// At the initial 400×320 size this preserves the existing 14 rows. Wider
/// windows widen the content; taller windows add transcript rows. Tiny sizes
/// clamp all extents to the surface and produce no transcript rows when the
/// header/footer leave no content viewport. The row count is capped so a
/// malformed extreme configure event cannot allocate unbounded scene nodes.
pub fn layout_monitor(scene: &Scene, width_px: i32, height_px: i32) -> MonitorLayout {
    layout_monitor_with_controls(scene, width_px, height_px, &[])
}

pub fn layout_monitor_with_controls(
    scene: &Scene,
    width_px: i32,
    height_px: i32,
    controls: &[WorkflowControl],
) -> MonitorLayout {
    let width = f64::from(width_px.max(0));
    let height = f64::from(height_px.max(0));
    let mut responsive = scene.clone();

    // An omitted root extent makes the root track the actual viewport. The
    // scene's fixed initial extent was the source of clipped resized content.
    responsive.root.extent = None;

    let header_height = HEADER_HEIGHT.min(height / 2.0);
    let footer_height = FOOTER_HEIGHT.min((height - header_height).max(0.0));
    let footer_y = (height - footer_height - FOOTER_BOTTOM).max(header_height);
    let composer_x = 6.0_f64.min(width);
    let compact_quit_width = (width / 3.0).min(80.0);
    let composer_width = if footer_height < 50.0 {
        (width - compact_quit_width - 22.0 - composer_x).max(0.0)
    } else {
        (width - composer_x - 6.0).max(0.0)
    };

    let content_top = ROW_TOP.min(height);
    let content_bottom = (footer_y - FOOTER_GAP).max(content_top);
    let visible_rows =
        (((content_bottom - content_top) / ROW_HEIGHT).floor() as usize).min(MAX_VISIBLE_ROWS);

    set_geometry(
        &mut responsive.root,
        "header",
        (0.0, 0.0),
        (width, header_height),
    );
    set_geometry(
        &mut responsive.root,
        "title",
        (30.0_f64.min(width), 7.0_f64.min(header_height)),
        (
            (width - 136.0).max(0.0),
            (header_height - 8.0).clamp(0.0, 14.0),
        ),
    );
    set_geometry(
        &mut responsive.root,
        "status",
        ((width - 124.0).max(0.0), 7.0_f64.min(header_height)),
        (width.min(90.0), (header_height - 8.0).clamp(0.0, 14.0)),
    );
    set_geometry(
        &mut responsive.root,
        "composer",
        (composer_x, footer_y),
        (composer_width, 24.0_f64.min(footer_height)),
    );
    let second_row_y = footer_y + 26.0;
    for (name, x, w) in [
        ("queue_mode", 6.0_f64, 64.0_f64),
        ("guide_mode", 74.0_f64, 64.0_f64),
        ("send", 142.0_f64, 58.0_f64),
        ("queue_auto_drain", 204.0_f64, 96.0_f64),
    ] {
        let px = x.min(width);
        let pw = w.min((width - px).max(0.0));
        let visible = footer_height >= 50.0
            && controls
                .iter()
                .any(|control| control.id == name && control.enabled);
        set_geometry(
            &mut responsive.root,
            name,
            if visible {
                (px, second_row_y)
            } else {
                (0.0, 0.0)
            },
            if visible { (pw, 20.0) } else { (0.0, 0.0) },
        );
    }
    let quit_width = if footer_height < 50.0 {
        compact_quit_width
    } else {
        80.0_f64.min(width)
    };
    let quit_x = (width - quit_width - 6.0).max(0.0);
    let (quit_y, quit_h) = if footer_height >= 50.0 {
        (second_row_y, 20.0)
    } else {
        (footer_y, footer_height.min(24.0))
    };
    set_geometry(
        &mut responsive.root,
        "quit",
        (quit_x, quit_y),
        (quit_width.min(width - quit_x), quit_h),
    );
    let details_height = if footer_height >= 86.0 { 12.0 } else { 0.0 };
    set_geometry(
        &mut responsive.root,
        "workflow_title",
        (6.0, footer_y + 50.0),
        ((width - 12.0).max(0.0), details_height),
    );
    set_geometry(
        &mut responsive.root,
        "workflow_detail",
        (6.0, footer_y + 62.0),
        ((width - 12.0).max(0.0), details_height),
    );
    let visible_actions = controls
        .iter()
        .filter(|control| {
            control.enabled
                && !matches!(
                    control.id.as_str(),
                    "queue_mode"
                        | "guide_mode"
                        | "queue_auto_drain"
                        | "send"
                        | "session_prev"
                        | "session_next"
                        | "quit"
                )
        })
        .take(8)
        .collect::<Vec<_>>();
    let slot_gap = 4.0;
    let slot_width = ((width - 12.0 - 3.0 * slot_gap) / 4.0).max(0.0);
    for name in WORKFLOW_ACTION_BUTTONS {
        set_geometry(&mut responsive.root, name, (0.0, 0.0), (0.0, 0.0));
    }
    for (index, control) in visible_actions.iter().enumerate() {
        let row = index / 4;
        let column = index % 4;
        let visible = footer_height >= 110.0;
        set_geometry(
            &mut responsive.root,
            &control.id,
            if visible {
                (
                    6.0 + column as f64 * (slot_width + slot_gap),
                    footer_y + 74.0 + row as f64 * 20.0,
                )
            } else {
                (0.0, 0.0)
            },
            if visible {
                (slot_width, 18.0)
            } else {
                (0.0, 0.0)
            },
        );
    }
    for control in controls {
        if !visible_actions
            .iter()
            .any(|visible| visible.id == control.id)
            && !matches!(
                control.id.as_str(),
                "queue_mode"
                    | "guide_mode"
                    | "queue_auto_drain"
                    | "send"
                    | "session_prev"
                    | "session_next"
                    | "quit"
            )
        {
            set_geometry(&mut responsive.root, &control.id, (0.0, 0.0), (0.0, 0.0));
        }
    }
    for name in ["session_prev", "session_next"] {
        let visible = controls
            .iter()
            .any(|control| control.id == name && control.enabled);
        if visible {
            let x = if name == "session_prev" {
                5.0
            } else {
                (width - 24.0).max(0.0)
            };
            set_geometry(
                &mut responsive.root,
                name,
                (x, 4.0),
                (18.0_f64.min(width), 20.0_f64.min(header_height)),
            );
        } else {
            set_geometry(&mut responsive.root, name, (0.0, 0.0), (0.0, 0.0));
        }
    }
    if !controls
        .iter()
        .any(|control| control.id == "queue_auto_drain" && control.enabled)
    {
        set_geometry(
            &mut responsive.root,
            "queue_auto_drain",
            (0.0, 0.0),
            (0.0, 0.0),
        );
    }
    for name in ["session_prev", "session_next"] {
        if controls
            .iter()
            .any(|control| control.id == name && control.enabled)
        {
            let x = if name == "session_prev" {
                5.0
            } else {
                (width - 24.0).max(0.0)
            };
            set_geometry(&mut responsive.root, name, (x, 4.0), (18.0, 20.0));
        } else {
            set_geometry(&mut responsive.root, name, (0.0, 0.0), (0.0, 0.0));
        }
    }

    // Rows are authored once as a styled template. Clone it into the current
    // viewport so both rendering and downstream geometry share exact row ids.
    let template = responsive.find_node("row_00").cloned();
    if let Some(template) = template {
        let alternate = responsive
            .find_node("row_01")
            .cloned()
            .unwrap_or_else(|| template.clone());
        let rows = (0..visible_rows)
            .map(|i| {
                let mut row = if i % 2 == 0 {
                    template.clone()
                } else {
                    alternate.clone()
                };
                row.name = format!("row_{i:02}");
                row.position = Some(Vec3 {
                    x: 6.0,
                    y: ROW_TOP + ROW_HEIGHT * i as f64,
                    z: 0.0,
                });
                row.extent = Some(Vec3 {
                    x: (width - 12.0).max(0.0),
                    y: ROW_HEIGHT,
                    z: 0.0,
                });
                row.position_p = None;
                row.extent_p = None;
                row
            })
            .collect::<Vec<_>>();
        replace_rows(&mut responsive.root, &rows);
    }

    let mut frame = layout(&responsive, width, height);
    // 场景布局按 1000mp/单位输出，而 Wayland 使用 250mp/像素；
    // 原实现未转换导致 400px 窗口内容放大四倍，底部按钮无法点击。
    for node in &mut frame.nodes {
        for rect in std::iter::once(&mut node.rect).chain(node.clip.iter_mut()) {
            rect.x /= 4;
            rect.y /= 4;
            rect.width /= 4;
            rect.height /= 4;
        }
        node.text_size = node.text_size.map(|size| size / 4.0);
        node.stroke_width = node.stroke_width.map(|size| size / 4.0);
    }
    MonitorLayout {
        frame,
        visible_rows,
        width_px: width_px.max(0),
        height_px: height_px.max(0),
    }
}

fn set_geometry(root: &mut SceneCell, name: &str, position: (f64, f64), extent: (f64, f64)) {
    if root.name == name {
        root.position = Some(Vec3 {
            x: position.0,
            y: position.1,
            z: 0.0,
        });
        root.extent = Some(Vec3 {
            x: extent.0,
            y: extent.1,
            z: 0.0,
        });
        root.position_p = None;
        root.extent_p = None;
        return;
    }
    for child in &mut root.children {
        set_geometry(child, name, position, extent);
    }
}

fn replace_rows(root: &mut SceneCell, rows: &[SceneCell]) {
    let first_row = root
        .children
        .iter()
        .position(|child| child.name.starts_with("row_"));
    if let Some(index) = first_row {
        root.children
            .retain(|child| !child.name.starts_with("row_"));
        root.children.splice(index..index, rows.iter().cloned());
        return;
    }
    for child in &mut root.children {
        replace_rows(child, rows);
    }
}

#[cfg(test)]
mod tests {
    use super::{layout_monitor, layout_monitor_with_controls, MAX_VISIBLE_ROWS};
    use crate::view::WorkflowControl;
    use goya_lang::{hit_test, parse};

    fn scene() -> goya_lang::Scene {
        parse(include_str!("../../../apps/zcode-native.gscene")).expect("monitor scene parses")
    }

    #[test]
    fn resize_tracks_surface_and_generates_rows_for_available_height() {
        let scene = scene();
        let initial = layout_monitor(&scene, 400, 320);
        let expanded = layout_monitor(&scene, 720, 600);
        assert_eq!(initial.visible_rows, 9);
        assert_eq!(expanded.visible_rows, 24);
        assert_eq!(expanded.frame.find("window").unwrap().rect.width, 180_000);
        assert_eq!(expanded.frame.find("composer").unwrap().rect.y, 121_500);
        assert_eq!(expanded.frame.find("row_23").unwrap().rect.y, 111_500);
        assert!(expanded.frame.find("row_24").is_none());
        assert_eq!(initial.frame.find("row_00").unwrap().rect.width, 97_000);
    }

    #[test]
    fn minimum_dimensions_clamp_controls_and_remove_transcript_rows() {
        let layout = layout_monitor(&scene(), 80, 48);
        let quit = layout.frame.find("quit").unwrap().rect;
        let composer = layout.frame.find("composer").unwrap().rect;
        let header = layout.frame.find("header").unwrap().rect;
        assert_eq!(layout.visible_rows, 0);
        assert!(header.y + header.height <= composer.y);
        assert!(quit.x >= 0 && quit.width >= 0 && quit.x + quit.width <= 20_000);
        assert!(composer.x >= 0 && composer.width >= 0 && composer.x + composer.width <= 20_000);
        assert!(layout.frame.find("row_00").is_none());
    }

    #[test]
    fn zero_and_extreme_sizes_remain_bounded() {
        let scene = scene();
        let zero = layout_monitor(&scene, 0, 0);
        assert_eq!(zero.visible_rows, 0);
        for node in &zero.frame.nodes {
            assert!(node.rect.width >= 0 && node.rect.height >= 0);
            assert_eq!(node.rect.width, 0);
            assert_eq!(node.rect.height, 0);
        }

        let extreme = layout_monitor(&scene, i32::MAX, i32::MAX);
        assert_eq!(extreme.visible_rows, MAX_VISIBLE_ROWS);
        assert!(extreme.frame.find("row_255").is_some());
        assert!(extreme.frame.find("row_256").is_none());
    }

    #[test]
    fn generated_frame_is_the_hit_test_geometry() {
        let layout = layout_monitor(&scene(), 640, 480);
        let quit = layout.frame.find("quit").unwrap().rect;
        assert_eq!(
            layout.frame.find("window").unwrap().rect.width / goya_wayland::MP_PER_PX,
            640
        );
        assert!(quit.y + quit.height <= 480 * goya_wayland::MP_PER_PX);
        let center_x = 580 * goya_wayland::MP_PER_PX;
        let center_y = 402 * goya_wayland::MP_PER_PX;
        assert_eq!(
            hit_test(&layout.frame, center_x, center_y).as_deref(),
            Some("quit")
        );
    }

    #[test]
    fn only_projected_workflow_actions_receive_native_hit_targets() {
        let controls = vec![
            WorkflowControl {
                id: "queue_mode".into(),
                label: "Send as queued input".into(),
                enabled: true,
            },
            WorkflowControl {
                id: "guide_mode".into(),
                label: "Send as guided input".into(),
                enabled: true,
            },
            WorkflowControl {
                id: "send".into(),
                label: "Send message".into(),
                enabled: true,
            },
            WorkflowControl {
                id: "queue_remove_00".into(),
                label: "Remove queued input: next".into(),
                enabled: true,
            },
        ];
        let layout = layout_monitor_with_controls(&scene(), 400, 320, &controls);
        let queue = layout.frame.find("queue_mode").unwrap().rect;
        let guide = layout.frame.find("guide_mode").unwrap().rect;
        let remove = layout.frame.find("queue_remove_00").unwrap().rect;
        let absent = layout.frame.find("queue_remove_01").unwrap().rect;
        assert!(queue.width > 0 && guide.width > 0 && remove.width > 0);
        assert_eq!(absent.width, 0);
        assert!(queue.x + queue.width <= guide.x || guide.x + guide.width <= queue.x);
        assert_eq!(
            hit_test(
                &layout.frame,
                remove.x + remove.width / 2,
                remove.y + remove.height / 2
            )
            .as_deref(),
            Some("queue_remove_00")
        );
    }
}
