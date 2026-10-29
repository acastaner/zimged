//! The eframe app: state, the per-frame loop, and command execution.
//!
//! Keep this file thin. Chrome lives in `ui/` (one module per bar), commands in `commands.rs`, and
//! domain state in `zimged-core`. `ui()` polls background work, then lays out the panels.

use std::path::{Path, PathBuf};

use zimged_codec::{DisplayImage, Size, fit_within};
use zimged_platform::{Config, Platform};

use crate::Command;
use crate::decode::{DecodeResult, PendingDecode};
use crate::ui::{canvas, status_bar};
use crate::view::{View, ZOOM_STEP, Zoom};

/// The largest Image whose full-resolution pixels are decoded (§4.7). Larger ones are view-only and
/// zoom in on their display-size pixels.
const FULL_RESOLUTION_LIMIT: u64 = 100_000_000;

/// Everything the startup path (or a test) hands the app.
pub struct Launch {
    pub platform: Platform,
    pub config: Config,
    /// The first-image decode started at `main()` (§1.3), if a path was given.
    pub first_image: Option<PendingDecode>,
}

impl Launch {
    /// No image, the config read from the platform's store. This is what the headless tests use.
    pub fn new(platform: Platform) -> Self {
        let config = Config::load(platform.config.as_ref());
        Self {
            platform,
            config,
            first_image: None,
        }
    }
}

/// What the canvas shows (see [`ZimgedApp::canvas`]).
#[derive(Clone, Copy, Debug)]
pub struct CanvasView {
    /// The canvas, in points.
    pub area: egui::Rect,
    /// Where the Image is drawn, in points.
    pub image: egui::Rect,
    /// The size of the pixels drawn: the display-size decode (§5.2), or the full-resolution one
    /// once zoomed past it (§7.2).
    pub decoded_size: Size,
}

/// The Image on screen.
pub(crate) struct ShownImage {
    pub path: PathBuf,
    pub file_name: String,
    pub original_size: Size,
    /// The display-size decode (§5.2), re-decoded larger if the canvas outgrows it (a resize, or a
    /// HiDPI scale factor the startup decode couldn't know).
    display: egui::TextureHandle,
    /// The full-resolution decode, once zoomed past the display-size one (§7.2). Capped at the GPU's
    /// largest texture.
    full: Option<egui::TextureHandle>,
    /// The largest decode asked for so far, so a size that can't be bettered isn't asked for again.
    requested: Size,
}

impl ShownImage {
    /// The texture to draw when the Image covers `on_screen` physical pixels.
    pub fn texture_for(&self, on_screen: Size) -> &egui::TextureHandle {
        let display = texture_size(&self.display);
        match &self.full {
            Some(full) if on_screen.width > display.width || on_screen.height > display.height => {
                full
            }
            _ => &self.display,
        }
    }

    /// The most pixels on hand.
    fn best_size(&self) -> Size {
        texture_size(self.full.as_ref().unwrap_or(&self.display))
    }
}

/// Which pixels a background decode of the shown Image brings.
#[derive(Clone, Copy, PartialEq)]
enum Pixels {
    Display,
    Full,
}

/// A decode of the shown Image that brings more pixels.
struct Upgrade {
    pending: PendingDecode,
    pixels: Pixels,
}

/// Where the app is on the way to its first painted image (§1.3 steps 4 and 5).
#[derive(Clone, Copy, PartialEq)]
enum FirstPaint {
    Waiting,
    /// The frame now being built draws the first image.
    ThisFrame,
    /// The first image has been handed to the compositor and the after-first-paint work started.
    Done,
}

pub struct ZimgedApp {
    #[expect(dead_code, reason = "used by the clipboard, trash and dialog tickets")]
    platform: Platform,
    config: Config,
    pending: Option<PendingDecode>,
    shown: Option<ShownImage>,
    /// "Can't open <name>: <reason>" (§5.2), shown on the canvas instead of an image.
    error: Option<String>,
    upgrade: Option<Upgrade>,
    view: View,
    /// Wanted fullscreen state (F11). Chrome is hidden while it's on.
    fullscreen: bool,
    /// The last fullscreen state sent to the window, and the last one the window reported.
    fullscreen_sent: Option<bool>,
    fullscreen_seen: Option<bool>,
    /// The canvas as laid out in the last frame.
    canvas: Option<canvas::Layout>,
    first_paint: FirstPaint,
}

impl ZimgedApp {
    pub fn new(cc: &eframe::CreationContext<'_>, launch: Launch) -> Self {
        if let Some(pending) = &launch.first_image {
            pending.wake(&cc.egui_ctx);
        }
        // Ctrl+plus / minus / 0 would scale the whole UI; the image zoom is on plain +, - and 0.
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        Self {
            platform: launch.platform,
            config: launch.config,
            pending: launch.first_image,
            shown: None,
            error: None,
            upgrade: None,
            view: View::fit(),
            fullscreen: false,
            fullscreen_sent: None,
            fullscreen_seen: None,
            canvas: None,
            first_paint: FirstPaint::Waiting,
        }
    }

    pub fn execute(&mut self, command: Command) {
        match command {
            Command::Open(path) => self.open(&path),
            Command::ZoomIn => self.zoom(ZOOM_STEP, None),
            Command::ZoomOut => self.zoom(1.0 / ZOOM_STEP, None),
            Command::ZoomAround { factor, around } => self.zoom(factor, Some(around)),
            Command::ZoomFit => self.view = View::fit(),
            Command::ToggleFullscreen => self.fullscreen = !self.fullscreen,
        }
    }

    /// Is background work still pending (a decode, …)? Tests wait for this to clear.
    pub fn is_busy(&self) -> bool {
        self.pending.is_some() || self.upgrade.is_some()
    }

    /// Is the window fullscreen (or about to be)?
    pub fn is_fullscreen(&self) -> bool {
        self.fullscreen
    }

    fn zoom(&mut self, factor: f32, around: Option<egui::Pos2>) {
        let (Some(shown), Some(layout)) = (&self.shown, self.canvas) else {
            return;
        };
        self.view.zoom_by(
            factor,
            around,
            shown.original_size,
            layout.area,
            layout.pixels_per_point,
        );
    }

    fn open(&mut self, path: &Path) {
        let display_box = self
            .canvas
            .map(|c| c.area_px())
            .unwrap_or_else(|| startup_display_box(&self.config));
        // The previous Image stays on screen until the new one is decoded (§3.3).
        self.pending = Some(PendingDecode::spawn(path.to_owned(), move || display_box));
    }

    fn poll_decode(&mut self, ctx: &egui::Context) {
        let Some(pending) = &self.pending else {
            return;
        };
        pending.wake(ctx);
        let Some(result) = pending.take() else {
            return;
        };
        let path = pending.path().to_owned();
        self.pending = None;
        self.show_decoded(ctx, path, result);
    }

    fn show_decoded(&mut self, ctx: &egui::Context, path: PathBuf, result: DecodeResult) {
        let file_name = file_name(&path);
        match result {
            Ok(image) => {
                self.shown = Some(ShownImage {
                    path,
                    file_name,
                    original_size: image.original_size,
                    display: upload(ctx, &image, Pixels::Display),
                    full: None,
                    requested: image.size,
                });
                self.upgrade = None;
                self.error = None;
                self.view = View::fit();
            }
            Err(reason) => {
                self.shown = None;
                self.error = Some(format!("Can't open {file_name}: {reason}"));
            }
        }
    }

    fn poll_upgrade(&mut self, ctx: &egui::Context) {
        let Some(upgrade) = &self.upgrade else {
            return;
        };
        upgrade.pending.wake(ctx);
        let Some(result) = upgrade.pending.take() else {
            return;
        };
        let upgrade = self.upgrade.take().expect("checked above");
        let (Ok(image), Some(shown)) = (result, self.shown.as_mut()) else {
            // A failed re-decode (the file changed or went away) keeps the pixels on screen.
            return;
        };
        if upgrade.pending.path() != shown.path || image.original_size != shown.original_size {
            return;
        }
        let texture = upload(ctx, &image, upgrade.pixels);
        match upgrade.pixels {
            Pixels::Display => shown.display = texture,
            Pixels::Full => shown.full = Some(texture),
        }
    }

    /// Start a decode with more pixels when the Image is drawn larger than the pixels on hand: at
    /// Fit, a display-size re-decode for the canvas's physical size; zoomed, the full resolution
    /// (§7.2). One at a time, never for the Image that's about to be replaced, and not before the
    /// first paint (§1.3).
    fn request_pixels(&mut self, ctx: &egui::Context) {
        if self.upgrade.is_some() || self.pending.is_some() || self.first_paint != FirstPaint::Done
        {
            return;
        }
        let (Some(layout), Some(shown)) = (self.canvas, self.shown.as_mut()) else {
            return;
        };
        let Some(on_screen) = layout.image_px() else {
            return;
        };
        let have = shown.best_size();
        if on_screen.width <= have.width && on_screen.height <= have.height {
            return;
        }
        let original = shown.original_size;
        let (pixels, max) = match self.view.zoom {
            Zoom::Fit => (Pixels::Display, layout.area_px()),
            Zoom::Scale(_)
                if u64::from(original.width) * u64::from(original.height)
                    <= FULL_RESOLUTION_LIMIT =>
            {
                let side = ctx.input(|i| i.max_texture_side) as u32;
                (Pixels::Full, Size::new(side, side))
            }
            Zoom::Scale(_) => return,
        };
        let target = fit_within(original, max);
        if target.width <= shown.requested.width && target.height <= shown.requested.height {
            return;
        }
        shown.requested = target;
        let pending = PendingDecode::spawn(shown.path.clone(), move || max);
        pending.wake(ctx);
        self.upgrade = Some(Upgrade { pending, pixels });
    }

    /// Follow F11, and adopt a fullscreen change made by the window manager.
    fn sync_fullscreen(&mut self, ctx: &egui::Context) {
        let seen = ctx.input(|i| i.viewport().fullscreen);
        if let Some(seen) = seen
            && self.fullscreen_seen != Some(seen)
        {
            self.fullscreen_seen = Some(seen);
            // Only when it isn't the window catching up with an F11 not yet applied.
            if self
                .fullscreen_sent
                .is_none_or(|sent| sent == self.fullscreen)
            {
                self.fullscreen = seen;
                self.fullscreen_sent = Some(seen);
            }
        }
        if self.fullscreen_sent != Some(self.fullscreen) {
            self.fullscreen_sent = Some(self.fullscreen);
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
        }
    }

    /// §1.3 step 5: runs once, on the frame after the first image was painted. The Sibling scan,
    /// directory watcher, prefetch, and UI font loading start from here, never before.
    fn after_first_paint(&mut self, _ctx: &egui::Context) {}

    /// What the canvas shows, as of the last frame: `None` when no Image is drawn.
    pub fn canvas(&self) -> Option<CanvasView> {
        let layout = self.canvas?;
        self.shown.as_ref()?;
        Some(CanvasView {
            area: layout.area,
            image: layout.image?,
            decoded_size: layout.drawn?,
        })
    }

    fn status_bar(&self) -> status_bar::StatusBar<'_> {
        status_bar::StatusBar {
            file_name: self.shown.as_ref().map(|s| s.file_name.as_str()),
            pixel_size: self.shown.as_ref().map(|s| s.original_size),
            zoom: self.shown.as_ref().map(|_| self.view.zoom),
        }
    }
}

impl eframe::App for ZimgedApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        if self.first_paint == FirstPaint::ThisFrame {
            self.first_paint = FirstPaint::Done;
            self.after_first_paint(&ctx);
        }
        self.poll_decode(&ctx);
        self.poll_upgrade(&ctx);
        for command in crate::input::commands(&ctx, self.canvas.map(|c| c.area)) {
            self.execute(command);
        }
        self.sync_fullscreen(&ctx);

        if !self.fullscreen {
            status_bar::show(ui, &self.status_bar());
        }
        self.canvas = Some(canvas::show(
            ui,
            self.shown.as_ref(),
            &mut self.view,
            self.error.as_deref(),
        ));
        self.request_pixels(&ctx);

        if self.first_paint == FirstPaint::Waiting && self.shown.is_some() {
            // Same convention as the spike: the mark is taken while building the frame that draws
            // the image; eframe paints and hands it to the compositor right after this returns.
            crate::bench::mark("first_image_painted");
            self.first_paint = FirstPaint::ThisFrame;
            if crate::bench::exit_after_first_paint() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint();
        }
    }
}

/// The display box to decode into before the window exists: the configured window size, at a
/// scale factor of 1 (the real one is unknown until the window is created).
pub fn startup_display_box(config: &Config) -> Size {
    Size::new(
        config.window.width.round() as u32,
        config.window.height.round() as u32,
    )
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

fn upload(ctx: &egui::Context, image: &DisplayImage, kind: Pixels) -> egui::TextureHandle {
    let pixels = egui::ColorImage::from_rgb(
        [image.size.width as usize, image.size.height as usize],
        &image.rgb,
    );
    let (name, options) = match kind {
        Pixels::Display => ("image", egui::TextureOptions::LINEAR),
        // Drawn anywhere from just above the display size down: mipmaps keep it from aliasing.
        Pixels::Full => (
            "image-full",
            egui::TextureOptions {
                mipmap_mode: Some(egui::TextureFilter::Linear),
                ..egui::TextureOptions::LINEAR
            },
        ),
    };
    ctx.load_texture(name, pixels, options)
}

fn texture_size(texture: &egui::TextureHandle) -> Size {
    let [w, h] = texture.size();
    Size::new(w as u32, h as u32)
}
