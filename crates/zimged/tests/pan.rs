//! Panning a zoomed Image (#43, follow-up to spec §3.1).

mod support;

use egui::{Key, PointerButton, pos2, vec2};
use support::TestApp;

/// The large fixture, zoomed in three steps (about 82%): larger than the canvas on both axes.
fn zoomed() -> TestApp {
    let mut app = TestApp::open_fixture("large-2400x1600.jpg");
    for _ in 0..3 {
        app.press(Key::Plus);
    }
    let canvas = app.harness.state().canvas().unwrap();
    assert!(canvas.image.width() > canvas.area.width() + 200.0);
    assert!(canvas.image.height() > canvas.area.height() + 200.0);
    app
}

fn image(app: &TestApp) -> egui::Rect {
    app.harness.state().canvas().unwrap().image
}

fn close(a: egui::Vec2, b: egui::Vec2) -> bool {
    (a - b).length() < 1.01
}

#[test]
fn space_and_left_drag_pans_a_zoomed_image() {
    let mut app = zoomed();
    let before = image(&app);

    app.drag(
        PointerButton::Primary,
        pos2(500.0, 300.0),
        pos2(400.0, 250.0),
        true,
    );

    let moved = image(&app).min - before.min;
    assert!(close(moved, vec2(-100.0, -50.0)), "moved by {moved:?}");
}

#[test]
fn middle_drag_pans_a_zoomed_image() {
    let mut app = zoomed();
    let before = image(&app);

    app.drag(
        PointerButton::Middle,
        pos2(500.0, 300.0),
        pos2(560.0, 340.0),
        false,
    );

    let moved = image(&app).min - before.min;
    assert!(close(moved, vec2(60.0, 40.0)), "moved by {moved:?}");
}

#[test]
fn a_left_drag_without_space_does_not_pan() {
    let mut app = zoomed();
    let before = image(&app);

    app.drag(
        PointerButton::Primary,
        pos2(500.0, 300.0),
        pos2(400.0, 250.0),
        false,
    );

    assert_eq!(image(&app), before);
}

#[test]
fn the_pan_stops_where_the_image_edge_meets_the_canvas_edge() {
    let mut app = zoomed();

    // Further than the Image overhangs: its top-left corner stops at the canvas's.
    for _ in 0..3 {
        app.drag(
            PointerButton::Middle,
            pos2(100.0, 100.0),
            pos2(900.0, 650.0),
            false,
        );
    }
    let canvas = app.harness.state().canvas().unwrap();
    assert!(close(canvas.image.min.to_vec2(), canvas.area.min.to_vec2()));

    // And the other way, the bottom-right corner.
    for _ in 0..3 {
        app.drag(
            PointerButton::Middle,
            pos2(900.0, 650.0),
            pos2(100.0, 100.0),
            false,
        );
    }
    let canvas = app.harness.state().canvas().unwrap();
    assert!(close(canvas.image.max.to_vec2(), canvas.area.max.to_vec2()));
}

#[test]
fn an_image_narrower_than_the_canvas_stays_centred_on_that_axis() {
    // Five steps (305%) make 320×240 about 977×732: narrower than the canvas, but taller.
    let mut app = TestApp::open_fixture("small-320x240.jpg");
    for _ in 0..5 {
        app.press(Key::Plus);
    }
    let before = app.harness.state().canvas().unwrap();
    assert!(before.image.width() < before.area.width());
    assert!(before.image.height() > before.area.height());

    app.drag(
        PointerButton::Middle,
        pos2(500.0, 300.0),
        pos2(300.0, 290.0),
        false,
    );

    let after = image(&app);
    assert!((after.center().x - before.area.center().x).abs() < 1.0);
    assert!(close(after.min - before.image.min, vec2(0.0, -10.0)));
}

#[test]
fn panning_at_fit_does_nothing() {
    let mut app = TestApp::open_fixture("large-2400x1600.jpg");
    let before = image(&app);

    app.drag(
        PointerButton::Middle,
        pos2(500.0, 300.0),
        pos2(300.0, 200.0),
        false,
    );

    assert_eq!(image(&app), before);
    assert_eq!(app.status("Zoom"), "Fit");
}

#[test]
fn shift_wheel_pans_horizontally() {
    let mut app = zoomed();
    let before = image(&app);

    // One notch down scrolls the view right: the Image moves left by one line step (40 points).
    app.wheel_at(pos2(500.0, 300.0), -1.0, egui::Modifiers::SHIFT);

    let moved = image(&app).min - before.min;
    assert!(close(moved, vec2(-40.0, 0.0)), "moved by {moved:?}");
}

#[test]
fn the_plain_wheel_does_not_pan() {
    let mut app = zoomed();
    let before = image(&app);

    app.wheel_at(pos2(500.0, 300.0), -1.0, egui::Modifiers::NONE);

    assert_eq!(image(&app), before);
}

#[test]
fn zero_resets_the_pan() {
    let mut app = zoomed();
    app.drag(
        PointerButton::Middle,
        pos2(500.0, 300.0),
        pos2(300.0, 200.0),
        false,
    );

    app.press(Key::Num0);
    app.press(Key::Plus);

    let canvas = app.harness.state().canvas().unwrap();
    assert!((canvas.image.center() - canvas.area.center()).length() < 1.0);
}

#[test]
fn opening_an_image_resets_the_pan() {
    let mut app = zoomed();
    app.drag(
        PointerButton::Middle,
        pos2(500.0, 300.0),
        pos2(300.0, 200.0),
        false,
    );

    let path = app.add_fixtures(&["large-2400x1600.jpg"]);
    app.open(path);
    app.press(Key::Plus);

    let canvas = app.harness.state().canvas().unwrap();
    assert!((canvas.image.center() - canvas.area.center()).length() < 1.0);
}
