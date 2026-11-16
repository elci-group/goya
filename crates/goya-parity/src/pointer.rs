//! Hit-testing for the admitted interactive counter.
//!
//! Rectangles are the DIP values Sash admitted. The walk is front to back.
//! This module does not compare pixels and does not open a window.

/// One admitted control rectangle, in device-independent pixels.
pub struct HitControl {
    pub semantic_id: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Controls in admission order. The first rectangle is the client.
pub struct HitMap {
    pub controls: Vec<HitControl>,
}

/// Admit `source` and copy each control rectangle.
///
/// A static scene is `SASH-CAP`. Any other admission error is returned unchanged.
pub fn hit_map(source: &str) -> Result<HitMap, sash::Diagnostic> {
    let admitted = match sash::admit::open(source)? {
        sash::admit::SceneTarget::Interactive(admitted) => admitted,
        sash::admit::SceneTarget::Static(_) => {
            return Err(cap("this run admits only the interactive counter"));
        }
    };
    Ok(HitMap {
        controls: admitted
            .controls
            .into_iter()
            .map(|control| HitControl {
                semantic_id: control.semantic_id,
                x: control.x_dip,
                y: control.y_dip,
                width: control.width_dip,
                height: control.height_dip,
            })
            .collect(),
    })
}

impl HitMap {
    /// Front-most control that contains `(x, y)`.
    ///
    /// The walk is last to first. A rectangle contains a point when
    /// `x >= left`, `y >= top`, `x < left + width`, and `y < top + height`.
    /// The first control is the client. A point outside it is `None`.
    pub fn at(&self, x: i32, y: i32) -> Option<&HitControl> {
        let (left, top, width, height) = {
            let client = self.controls.first()?;
            (client.x, client.y, client.width, client.height)
        };
        if !contains_rect(left, top, width, height, x, y) {
            return None;
        }
        self.controls
            .iter()
            .rev()
            .find(|control| contains(control, x, y))
    }
}

fn contains(control: &HitControl, x: i32, y: i32) -> bool {
    contains_rect(control.x, control.y, control.width, control.height, x, y)
}

fn contains_rect(left: i32, top: i32, width: i32, height: i32, x: i32, y: i32) -> bool {
    let x = i64::from(x);
    let y = i64::from(y);
    let left = i64::from(left);
    let top = i64::from(top);
    x >= left && y >= top && x < left + i64::from(width) && y < top + i64::from(height)
}

fn cap(message: impl Into<String>) -> sash::Diagnostic {
    sash::Diagnostic::new(
        "SASH-CAP",
        "admit",
        message,
        "Author the interactive counter. This run uses the integers Sash parsed and does not infer a missing value.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATIC_SOURCE: &str = include_str!("../../sash/fixtures/static-window.gscene");

    fn counter() -> HitMap {
        hit_map(crate::COUNTER_SOURCE).expect("counter admits")
    }

    fn named<'a>(map: &'a HitMap, id: &str) -> &'a HitControl {
        map.controls
            .iter()
            .find(|control| control.semantic_id == id)
            .unwrap_or_else(|| panic!("missing {id}"))
    }

    fn interior(control: &HitControl) -> (i32, i32) {
        assert!(control.width > 0 && control.height > 0);
        (
            control.x + control.width / 2,
            control.y + control.height / 2,
        )
    }

    #[test]
    fn rectangles_follow_admission_order() {
        let map = counter();
        let opened = sash::admit::open(crate::COUNTER_SOURCE).expect("counter admits");
        let sash::admit::SceneTarget::Interactive(admitted) = opened else {
            panic!("counter is interactive");
        };
        assert_eq!(map.controls.len(), admitted.controls.len());
        for (copied, control) in map.controls.iter().zip(&admitted.controls) {
            assert_eq!(copied.semantic_id, control.semantic_id);
            assert_eq!(copied.x, control.x_dip);
            assert_eq!(copied.y, control.y_dip);
            assert_eq!(copied.width, control.width_dip);
            assert_eq!(copied.height, control.height_dip);
        }
        assert_eq!(
            map.controls
                .first()
                .map(|control| control.semantic_id.as_str()),
            Some(admitted.semantic_id.as_str())
        );
    }

    #[test]
    fn button_contains_its_top_left_but_not_its_right_or_bottom_edge() {
        let map = counter();
        let button = named(&map, "window.add");
        assert!(button.width > 0 && button.height > 0);
        assert_eq!(
            map.at(button.x, button.y)
                .map(|hit| hit.semantic_id.as_str()),
            Some("window.add")
        );
        assert_ne!(
            map.at(button.x + button.width, button.y)
                .map(|hit| hit.semantic_id.as_str()),
            Some("window.add")
        );
        assert_ne!(
            map.at(button.x, button.y + button.height)
                .map(|hit| hit.semantic_id.as_str()),
            Some("window.add")
        );
    }

    #[test]
    fn point_inside_the_button_hits_window_add_not_the_root() {
        let map = counter();
        let (x, y) = interior(named(&map, "window.add"));
        let hit = map.at(x, y).expect("button");
        assert_eq!(hit.semantic_id, "window.add");
        assert_ne!(hit.semantic_id, "window.root");
    }

    #[test]
    fn point_inside_the_edit_hits_window_amount() {
        let map = counter();
        let (x, y) = interior(named(&map, "window.amount"));
        assert_eq!(
            map.at(x, y).map(|hit| hit.semantic_id.as_str()),
            Some("window.amount")
        );
    }

    #[test]
    fn point_inside_the_result_hits_state_count() {
        let map = counter();
        let (x, y) = interior(named(&map, "state.count"));
        assert_eq!(
            map.at(x, y).map(|hit| hit.semantic_id.as_str()),
            Some("state.count")
        );
    }

    #[test]
    fn point_in_the_client_outside_the_controls_hits_the_root() {
        let map = counter();
        let root = named(&map, "window.root");
        let edit = named(&map, "window.amount");
        let button = named(&map, "window.add");
        let result = named(&map, "state.count");
        let mut found = None;
        for y in root.y..root.y + root.height {
            for x in root.x..root.x + root.width {
                if !contains(edit, x, y) && !contains(button, x, y) && !contains(result, x, y) {
                    found = Some((x, y));
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
        let (x, y) = found.expect("client has a point outside the edit, button, and result");
        assert_eq!(
            map.at(x, y).map(|hit| hit.semantic_id.as_str()),
            Some("window.root")
        );
    }

    #[test]
    fn one_pixel_left_of_the_client_is_none() {
        let map = counter();
        let root = named(&map, "window.root");
        let x = root
            .x
            .checked_sub(1)
            .expect("client has a pixel to its left");
        assert!(map.at(x, root.y).is_none());
    }

    #[test]
    fn static_fixture_returns_err() {
        match hit_map(STATIC_SOURCE) {
            Err(error) => {
                assert_eq!(error.code, "SASH-CAP");
                assert_eq!(
                    error.message,
                    "this run admits only the interactive counter"
                );
            }
            Ok(_) => panic!("static scene produced a hit map"),
        }
    }
}
