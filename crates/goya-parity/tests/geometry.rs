//! Admitted DIP rectangles for the interactive counter.
//!
//! The numbers are the ones `sash::admit::open` returns. This test does not
//! launch a window and does not compare pixels.

#![forbid(unsafe_code)]

const SOURCE: &str = include_str!("../../sash/fixtures/interactive-counter.gscene");

#[test]
fn interactive_counter_admits_dip_rectangles() {
    let opened = sash::admit::open(SOURCE).expect("interactive counter admits");
    let sash::admit::SceneTarget::Interactive(admitted) = opened else {
        panic!("interactive counter must be SceneTarget::Interactive");
    };
    assert_eq!(admitted.profile, sash::interactive::PROFILE_CLIPBOARD);
    assert_eq!((admitted.width_dip, admitted.height_dip), (320, 160));
    assert_eq!(admitted.controls.len(), 4);

    let rect = |id: &str| -> (i32, i32, i32, i32) {
        let control = admitted
            .controls
            .iter()
            .find(|control| control.semantic_id == id)
            .unwrap_or_else(|| panic!("missing control {id}"));
        (
            control.x_dip,
            control.y_dip,
            control.width_dip,
            control.height_dip,
        )
    };

    assert_eq!(rect("window.root"), (0, 0, 320, 160));
    assert_eq!(rect("window.amount"), (8, 64, 120, 32));
    assert_eq!(rect("window.add"), (136, 64, 80, 32));
    assert_eq!(rect("state.count"), (8, 112, 160, 32));
}
