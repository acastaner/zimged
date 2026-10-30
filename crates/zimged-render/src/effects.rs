//! Effects shared by Annotation types. Highlight and obfuscate (#30, #31) go here too.

use skia_safe as sk;

/// The drop shadow of the "Shadow" property, in Image pixels (so Flatten matches the screen).
const SHADOW_OFFSET: (f32, f32) = (3.0, 3.0);
const SHADOW_SIGMA: f32 = 2.0;
const SHADOW_ALPHA: u8 = 110;

/// Draw `draw` with a drop shadow under it when `shadow` is on.
pub fn with_shadow(canvas: &sk::Canvas, shadow: bool, draw: impl FnOnce(&sk::Canvas)) {
    if !shadow {
        draw(canvas);
        return;
    }
    let filter = sk::image_filters::drop_shadow(
        SHADOW_OFFSET,
        (SHADOW_SIGMA, SHADOW_SIGMA),
        sk::Color::from_argb(SHADOW_ALPHA, 0, 0, 0),
        None,
        None,
        None,
    );
    let mut paint = sk::Paint::default();
    paint.set_image_filter(filter);
    canvas.save_layer(&sk::canvas::SaveLayerRec::default().paint(&paint));
    draw(canvas);
    canvas.restore();
}
