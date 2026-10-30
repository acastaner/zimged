//! Crop (#32): the crop tool (§4.1 C), the Image operation and its effect on Annotations (§4.7,
//! ADR 0001), and Image-operation History steps with their pixel cap (§4.8).

mod support;

use std::sync::Arc;

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;
use support::TestApp;
use zimged::tools::ToolId;
use zimged_core::{PixelData, Pixels};

/// A 320×240 Image, shown at 100%.
fn app_with_image() -> TestApp {
    TestApp::open_fixture("small-320x240.jpg")
}

fn rounded(r: zimged_core::Rect) -> [i32; 4] {
    [
        r.min.x.round() as i32,
        r.min.y.round() as i32,
        r.max.x.round() as i32,
        r.max.y.round() as i32,
    ]
}

fn crop_region(app: &TestApp) -> Option<[i32; 4]> {
    app.harness.state().crop_region().map(rounded)
}

fn annotation_rects(app: &TestApp) -> Vec<[i32; 4]> {
    app.doc()
        .annotations()
        .iter()
        .map(|a| rounded(a.bounds()))
        .collect()
}

/// The text of a property-bar read-out (a label with a fixed accessible name).
fn readout(app: &TestApp, label: &str) -> Option<String> {
    let wanted = label.to_owned();
    app.harness
        .query_by(move |node| {
            node.role() == Role::Label && node.label().as_deref() == Some(wanted.as_str())
        })
        .map(|node| node.value().unwrap_or_default())
}

fn full_resolution(app: &TestApp) -> Arc<Pixels> {
    app.doc()
        .full_resolution()
        .expect("full-resolution pixels")
        .clone()
}

/// The RGB of pixel (x, y).
fn rgb_at(pixels: &Pixels, x: u32, y: u32) -> [u8; 3] {
    let i = (y * pixels.width + x) as usize;
    match &pixels.data {
        PixelData::Rgb(d) => [d[i * 3], d[i * 3 + 1], d[i * 3 + 2]],
        PixelData::Rgba(d) => [d[i * 4], d[i * 4 + 1], d[i * 4 + 2]],
    }
}

/// Every pixel of `cropped` equals the pixel of `original` at the same place offset by `origin`.
fn assert_is_crop_of(cropped: &Pixels, original: &Pixels, origin: (u32, u32)) {
    for y in 0..cropped.height {
        for x in 0..cropped.width {
            assert_eq!(
                rgb_at(cropped, x, y),
                rgb_at(original, x + origin.0, y + origin.1),
                "pixel ({x}, {y})"
            );
        }
    }
}

fn draw_rectangle(app: &mut TestApp, from: (f32, f32), to: (f32, f32)) {
    app.press(Key::R);
    app.drag_px(from, to);
}

// ---- The crop tool ----

#[test]
fn c_then_a_drag_selects_a_region_and_the_property_bar_offers_its_size_apply_and_cancel() {
    let mut app = app_with_image();
    app.press(Key::C);
    assert_eq!(app.harness.state().tool(), ToolId::Crop);
    assert_eq!(crop_region(&app), None);

    app.drag_px((40.0, 30.0), (240.0, 180.0));

    assert_eq!(crop_region(&app), Some([40, 30, 240, 180]));
    assert_eq!(readout(&app, "Crop size").as_deref(), Some("200×150"));
    app.control("Apply");
    app.control("Cancel");
    // Selecting a region changes nothing yet.
    assert_eq!((app.doc().width(), app.doc().height()), (320, 240));
    assert!(!app.doc().can_undo());
}

#[test]
fn the_crop_region_stays_inside_the_image() {
    let mut app = app_with_image();
    app.press(Key::C);
    app.drag_px((200.0, 150.0), (400.0, 300.0));

    assert_eq!(crop_region(&app), Some([200, 150, 320, 240]));
}

#[test]
fn the_handles_adjust_the_region_and_a_drag_inside_moves_it() {
    let mut app = app_with_image();
    app.press(Key::C);
    app.drag_px((40.0, 30.0), (240.0, 180.0));

    app.drag_px((240.0, 180.0), (260.0, 200.0));
    assert_eq!(crop_region(&app), Some([40, 30, 260, 200]));

    app.drag_px((40.0, 115.0), (20.0, 115.0));
    assert_eq!(crop_region(&app), Some([20, 30, 260, 200]));

    app.drag_px((100.0, 100.0), (110.0, 90.0));
    assert_eq!(crop_region(&app), Some([30, 20, 270, 190]));
    assert_eq!(readout(&app, "Crop size").as_deref(), Some("240×170"));
}

#[test]
fn enter_crops_the_image_to_the_region() {
    let mut app = app_with_image();
    app.press(Key::C);
    let original = full_resolution(&app);
    app.drag_px((40.0, 30.0), (240.0, 180.0));

    app.press(Key::Enter);

    assert_eq!((app.doc().width(), app.doc().height()), (200, 150));
    let cropped = full_resolution(&app);
    assert_eq!((cropped.width, cropped.height), (200, 150));
    assert_is_crop_of(&cropped, &original, (40, 30));
    assert_eq!(app.status("Pixel size"), "200×150");
    assert_eq!(app.status("File name"), "small-320x240.jpg •");
    assert_eq!(crop_region(&app), None);
    assert_eq!(app.harness.state().tool(), ToolId::Select);
    // The whole cropped Image is on the canvas, from the cropped pixels.
    let composite = app.harness.state().composite().expect("a composite");
    assert_eq!((composite.width, composite.height), (200, 150));
}

#[test]
fn the_apply_button_crops_and_cancel_leaves_the_image_alone() {
    let mut app = app_with_image();
    app.press(Key::C);
    app.drag_px((40.0, 30.0), (240.0, 180.0));
    app.control("Cancel").click();
    app.settle();
    assert_eq!(crop_region(&app), None);
    assert_eq!((app.doc().width(), app.doc().height()), (320, 240));

    app.press(Key::C);
    app.drag_px((10.0, 20.0), (110.0, 70.0));
    app.control("Apply").click();
    app.settle();
    assert_eq!((app.doc().width(), app.doc().height()), (100, 50));
}

#[test]
fn esc_cancels_the_crop() {
    let mut app = app_with_image();
    app.press(Key::C);
    app.drag_px((40.0, 30.0), (240.0, 180.0));

    app.press(Key::Escape);

    assert_eq!(crop_region(&app), None);
    assert_eq!(app.harness.state().tool(), ToolId::Select);
    assert_eq!((app.doc().width(), app.doc().height()), (320, 240));
    assert!(!app.doc().can_undo());
}

// ---- Annotations through a crop (ADR 0001) ----

/// Three rectangles against the crop region (40, 30)–(240, 180): one inside, one across its
/// bottom-right corner, one entirely outside it.
fn app_with_three_rectangles() -> TestApp {
    let mut app = app_with_image();
    draw_rectangle(&mut app, (60.0, 50.0), (100.0, 80.0));
    draw_rectangle(&mut app, (200.0, 100.0), (300.0, 220.0));
    draw_rectangle(&mut app, (260.0, 195.0), (310.0, 230.0));
    app
}

fn crop_to_region(app: &mut TestApp) {
    app.press(Key::C);
    app.drag_px((40.0, 30.0), (240.0, 180.0));
    app.press(Key::Enter);
}

#[test]
fn annotations_keep_their_image_position_and_those_entirely_outside_are_removed() {
    let mut app = app_with_three_rectangles();
    let ids: Vec<_> = app.doc().annotations().iter().map(|a| a.id).collect();

    crop_to_region(&mut app);

    // Moved with the Image: (−40, −30). The one across the edge keeps its whole geometry, so it
    // stays editable; the canvas clips it at the Image's edge.
    assert_eq!(
        annotation_rects(&app),
        vec![[20, 20, 60, 50], [160, 70, 260, 190]]
    );
    let kept: Vec<_> = app.doc().annotations().iter().map(|a| a.id).collect();
    assert_eq!(kept, ids[..2]);
    app.assert_golden("crop-annotations");
}

#[test]
fn an_annotation_across_the_edge_stays_editable_after_the_crop() {
    let mut app = app_with_three_rectangles();
    crop_to_region(&mut app);

    // Grab the clipped rectangle by its left edge and move it back inside.
    app.drag_px((160.0, 100.0), (100.0, 40.0));

    assert_eq!(annotation_rects(&app)[1], [100, 10, 200, 130]);
}

#[test]
fn undo_restores_the_pixels_and_every_annotation_and_redo_crops_again() {
    let mut app = app_with_three_rectangles();
    let before = app.doc().annotations().to_vec();
    app.press(Key::C);
    let original = full_resolution(&app);
    app.drag_px((40.0, 30.0), (240.0, 180.0));
    app.press(Key::Enter);
    let cropped_annotations = app.doc().annotations().to_vec();

    app.key_with(Modifiers::COMMAND, Key::Z);

    assert_eq!((app.doc().width(), app.doc().height()), (320, 240));
    assert_eq!(app.doc().annotations(), &before[..]);
    let restored = full_resolution(&app);
    assert_is_crop_of(&restored, &original, (0, 0));
    assert_eq!(app.status("Pixel size"), "320×240");
    let composite = app.harness.state().composite().expect("a composite");
    assert_eq!((composite.width, composite.height), (320, 240));

    app.key_with(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);

    assert_eq!((app.doc().width(), app.doc().height()), (200, 150));
    assert_eq!(app.doc().annotations(), &cropped_annotations[..]);
    assert_is_crop_of(&full_resolution(&app), &original, (40, 30));
}

#[test]
fn undoing_every_step_after_a_crop_leaves_the_image_unmodified() {
    let mut app = app_with_image();
    draw_rectangle(&mut app, (60.0, 50.0), (100.0, 80.0));
    crop_to_region(&mut app);
    assert_eq!(app.doc().undo_len(), 2);

    app.key_with(Modifiers::COMMAND, Key::Z);
    app.key_with(Modifiers::COMMAND, Key::Z);

    assert!(!app.doc().is_modified());
    assert!(app.doc().annotations().is_empty());
    assert_eq!(app.status("File name"), "small-320x240.jpg");
    // Back to the plain Image texture.
    assert!(app.harness.state().composite().is_none());
}

// ---- The History's pixel cap (§4.8) ----

#[test]
fn image_operation_steps_beyond_the_pixel_cap_drop_the_oldest_steps() {
    // The 320×240 RGB Image is 230 400 bytes; the cap holds one pre-crop copy but not two.
    let mut app = TestApp::with_launch(|l| l.history_pixel_cap = 300_000);
    let path = app.add_fixtures(&["small-320x240.jpg"]);
    app.open(path);
    draw_rectangle(&mut app, (60.0, 50.0), (100.0, 80.0));

    app.press(Key::C);
    app.drag_px((0.0, 0.0), (300.0, 220.0));
    app.press(Key::Enter);
    assert_eq!(app.doc().undo_len(), 2);

    app.press(Key::C);
    app.drag_px((0.0, 0.0), (280.0, 200.0));
    app.press(Key::Enter);

    // 230 400 + 198 000 bytes is over the cap: the rectangle step and the first crop are gone.
    assert_eq!(app.doc().undo_len(), 1);
    app.key_with(Modifiers::COMMAND, Key::Z);
    assert_eq!((app.doc().width(), app.doc().height()), (300, 220));
    assert!(!app.doc().can_undo());
    assert_eq!(app.doc().annotations().len(), 1);
    // The opened state can't be reached again: the Image stays modified.
    assert!(app.doc().is_modified());
}
