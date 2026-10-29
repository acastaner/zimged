//! Animated GIF and WebP play in the viewer (spec §3.4).
//!
//! The fixtures (`tools/fixtures/make_fixtures.py`) show COLOUR for 100 ms, then for 300 ms a frame
//! whose left half is OTHER. The GIF stores that second frame as the left half only, drawn over the
//! first: its right half on screen shows the decoder composited it.

mod support;

use egui::Color32;
use support::TestApp;

const COLOUR: Color32 = Color32::from_rgb(40, 120, 200);
const OTHER: Color32 = Color32::from_rgb(200, 40, 40);

const ANIMATED: &str = "▶ animated";

/// WebP's encoder shifts a flat colour by a step or two, even lossless.
fn close(a: Color32, b: Color32) -> bool {
    a.to_array()
        .iter()
        .zip(b.to_array())
        .all(|(x, y)| x.abs_diff(y) <= 4)
}

#[test]
fn an_animated_gif_says_so_in_the_status_bar() {
    let app = TestApp::open_fixture("anim-partial-64x48.gif");
    assert_eq!(app.status_field("Animation").as_deref(), Some(ANIMATED));
}

/// Step the app frame by frame for `seconds` of egui time, 10 ms apart, and return the times at
/// which the on-screen pixel at `watched` changed, with its new colour. Also checks that the pixel
/// at `still` keeps `still_colour` throughout.
fn watch(
    app: &mut TestApp,
    seconds: f64,
    watched: (u32, u32),
    still: (u32, u32),
    still_colour: Color32,
) -> Vec<(f64, Color32)> {
    let start = app.harness.ctx.input(|i| i.time);
    let mut changes = Vec::new();
    let mut last = app.pixel(watched.0, watched.1);
    let mut t = start;
    while t < start + seconds {
        t += 0.01;
        app.harness.input_mut().time = Some(t);
        app.harness.step();
        let now = app.pixel(watched.0, watched.1);
        if now != last {
            changes.push((t, now.expect("a pixel")));
            last = now;
        }
        let kept = app.pixel(still.0, still.1).expect("a pixel");
        assert!(close(kept, still_colour), "{kept:?} at {t}");
    }
    changes
}

/// The animation shows COLOUR for 100 ms and OTHER for 300 ms, over and over.
fn assert_plays_its_own_timing(changes: &[(f64, Color32)]) {
    assert!(
        changes.len() >= 4,
        "expected a few frame changes, got {changes:?}"
    );
    for pair in changes.windows(2) {
        let ((t0, shown), (t1, _)) = (pair[0], pair[1]);
        assert!(
            close(shown, OTHER) || close(shown, COLOUR),
            "unexpected colour {shown:?}"
        );
        let expected = if close(shown, OTHER) { 0.3 } else { 0.1 };
        assert!(
            (t1 - t0 - expected).abs() < 0.015,
            "{shown:?} shown for {:.3} s, expected {expected} s: {changes:?}",
            t1 - t0
        );
    }
}

#[test]
fn an_animated_gif_plays_composited_frames_at_their_own_timing() {
    let mut app = TestApp::open_fixture("anim-partial-64x48.gif");
    // The right half is only in frame 1: the decoder draws frame 2 over it.
    let changes = watch(&mut app, 2.0, (8, 24), (56, 24), COLOUR);
    assert_plays_its_own_timing(&changes);
}

#[test]
fn an_animated_webp_plays_at_its_own_timing() {
    let mut app = TestApp::open_fixture("anim-64x48.webp");
    assert_eq!(app.status_field("Animation").as_deref(), Some(ANIMATED));
    let changes = watch(&mut app, 2.0, (8, 24), (56, 24), COLOUR);
    assert_plays_its_own_timing(&changes);
}

#[test]
fn a_still_webp_has_no_animation_state_and_stays_still() {
    let mut app = TestApp::open_fixture("webp-64x48.webp");
    assert_eq!(app.status_field("Animation"), None);
    let right = app.pixel(56, 24).expect("a pixel");
    let changes = watch(&mut app, 0.5, (8, 24), (56, 24), right);
    assert!(changes.is_empty(), "{changes:?}");
}
