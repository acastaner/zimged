//! The eframe app: state, the per-frame loop, and command execution.
//!
//! Keep this file thin. Chrome lives in `ui/` (one module per bar), commands in `commands.rs`, and
//! domain state in `zimged-core`. `ui()` polls background work, then lays out the panels.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use zimged_codec::{DisplayImage, Size};
use zimged_platform::{Config, Platform};
use zimged_siblings::{Decoded, Direction};

use crate::Command;
use crate::decode::PendingDecode;
use crate::siblings::{Navigation, Step};
use crate::ui::{canvas, status_bar};

/// How long a transient status-bar message stays up.
const MESSAGE_DURATION: Duration = Duration::from_secs(3);

/// Everything the startup path (or a test) hands the app.
pub struct Launch {
    pub platform: Platform,
    pub config: Config,
    /// The first-image decode started at `main()` (§1.3), if a path was given.
    pub first_image: Option<PendingDecode>,
    /// The display-size cache cap in bytes (§3.3). Tests lower it to watch eviction.
    pub cache_cap: usize,
}

impl Launch {
    /// No image, the config read from the platform's store. This is what the headless tests use.
    pub fn new(platform: Platform) -> Self {
        let config = Config::load(platform.config.as_ref());
        Self {
            platform,
            config,
            first_image: None,
            cache_cap: zimged_siblings::prefetch::CACHE_CAP_BYTES,
        }
    }
}

/// How the Image is scaled on the canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zoom {
    /// Fit to the canvas, never above 100% (§3.1 `0`, the default for every new Image).
    Fit,
}

impl Zoom {
    pub fn label(self) -> String {
        match self {
            Zoom::Fit => "Fit".to_owned(),
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
    /// The size of the pixels on screen: the display-size decode (§5.2).
    pub decoded_size: Size,
}

/// The Image on screen.
pub(crate) struct ShownImage {
    pub file_name: String,
    pub original_size: Size,
    pub texture: egui::TextureHandle,
}

/// Where the app is on the way to its first painted image (§1.3 steps 4 and 5).
#[derive(Clone, Copy, PartialEq)]
enum FirstPaint {
    Waiting,
    /// This frame (by `cumulative_frame_nr`) drew the first image.
    Drawn(u64),
    /// The first image has been handed to the compositor and the after-first-paint work started.
    Done,
}

pub struct ZimgedApp {
    /// For uploading textures and waking from background threads outside `ui()`.
    ctx: egui::Context,
    #[expect(dead_code, reason = "used by the clipboard, trash and dialog tickets")]
    platform: Platform,
    config: Config,
    pending: Option<PendingDecode>,
    /// The file on screen (or failed to open, when `error` is set).
    path: Option<PathBuf>,
    shown: Option<ShownImage>,
    /// "Can't open <name>: <reason>" (§5.2), shown on the canvas instead of an image.
    error: Option<String>,
    zoom: Zoom,
    /// The canvas as laid out in the last frame.
    canvas: Option<canvas::Layout>,
    first_paint: FirstPaint,
    navigation: Navigation,
    /// The transient status-bar message and when it was posted.
    message: Option<(&'static str, Instant)>,
    /// Benchmark driver for Sibling switching (`tools/bench switch`).
    bench_nav: Option<crate::bench::NavBench>,
}

impl ZimgedApp {
    pub fn new(cc: &eframe::CreationContext<'_>, launch: Launch) -> Self {
        if let Some(pending) = &launch.first_image {
            pending.wake(&cc.egui_ctx);
        }
        Self {
            ctx: cc.egui_ctx.clone(),
            platform: launch.platform,
            config: launch.config,
            pending: launch.first_image,
            path: None,
            shown: None,
            error: None,
            zoom: Zoom::Fit,
            canvas: None,
            first_paint: FirstPaint::Waiting,
            navigation: Navigation::new(launch.cache_cap),
            message: None,
            bench_nav: crate::bench::NavBench::from_env(),
        }
    }

    pub fn execute(&mut self, command: Command) {
        match command {
            Command::Open(path) => self.open(&path),
            Command::NextSibling => self.go(Direction::Forward),
            Command::PreviousSibling => self.go(Direction::Backward),
        }
    }

    /// Is background work still pending (a decode, the Sibling scan, prefetch)? Tests wait for this
    /// to clear.
    pub fn is_busy(&self) -> bool {
        self.pending.is_some() || self.navigation.is_busy()
    }

    /// The files in the display-size cache (§3.3) and their total size in bytes.
    pub fn cached_siblings(&self) -> (Vec<PathBuf>, usize) {
        self.navigation.cached()
    }

    fn display_box(&self) -> Size {
        self.canvas
            .map(|c| c.area_px())
            .unwrap_or_else(|| startup_display_box(&self.config))
    }

    fn open(&mut self, path: &Path) {
        let display_box = self.display_box();
        self.navigation.cancel();
        self.message = None;
        // The previous Image stays on screen until the new one is decoded (§3.3).
        self.pending = Some(PendingDecode::spawn(path.to_owned(), move || display_box));
    }

    fn go(&mut self, direction: Direction) {
        let Some(from) = self.path.clone() else {
            return;
        };
        self.message = None;
        match self.navigation.go(&from, direction, self.display_box()) {
            Step::Show(path, decoded) => {
                self.pending = None;
                let ctx = self.ctx.clone();
                self.show_decoded(&ctx, path, decoded);
            }
            Step::Pending => self.pending = None,
            Step::End(message) => self.message = Some((message, Instant::now())),
            Step::NotReady => {}
        }
    }

    fn poll_decode(&mut self, ctx: &egui::Context) {
        if let Some((path, decoded)) = self
            .navigation
            .poll(self.path.as_deref(), self.display_box())
        {
            self.show_decoded(ctx, path, decoded);
        }
        let Some(pending) = &self.pending else {
            return;
        };
        pending.wake(ctx);
        let Some(result) = pending.take() else {
            return;
        };
        let path = pending.path().to_owned();
        self.pending = None;
        self.show_decoded(ctx, path.clone(), result.map(Arc::new).map_err(Arc::new));
        if self.first_paint == FirstPaint::Done {
            // A newly opened file: its folder is the new Sibling list.
            self.navigation.start(ctx, &path);
        }
    }

    fn show_decoded(&mut self, ctx: &egui::Context, path: PathBuf, decoded: Decoded) {
        let file_name = file_name(&path);
        self.path = Some(path);
        match decoded {
            Ok(image) => {
                let texture = upload(ctx, &image);
                self.shown = Some(ShownImage {
                    file_name,
                    original_size: image.original_size,
                    texture,
                });
                self.error = None;
                self.zoom = Zoom::Fit;
            }
            Err(reason) => {
                self.shown = None;
                self.error = Some(format!("Can't open {file_name}: {reason}"));
            }
        }
    }

    /// §1.3 step 5: runs once, on the frame after the first image was painted. The Sibling scan,
    /// directory watcher, prefetch, and UI font loading start from here, never before.
    fn after_first_paint(&mut self, ctx: &egui::Context) {
        if let Some(path) = self.path.clone() {
            self.navigation.start(ctx, &path);
        }
    }

    /// What the canvas shows, as of the last frame: `None` when no Image is drawn.
    pub fn canvas(&self) -> Option<CanvasView> {
        let layout = self.canvas?;
        let shown = self.shown.as_ref()?;
        Some(CanvasView {
            area: layout.area,
            image: layout.image?,
            decoded_size: Size::new(
                shown.texture.size()[0] as u32,
                shown.texture.size()[1] as u32,
            ),
        })
    }

    fn status_bar(&self) -> status_bar::StatusBar<'_> {
        status_bar::StatusBar {
            file_name: self.shown.as_ref().map(|s| s.file_name.as_str()),
            position: self.path.as_deref().map(|p| self.navigation.position(p)),
            pixel_size: self.shown.as_ref().map(|s| s.original_size),
            zoom: self.shown.as_ref().map(|_| self.zoom),
            message: self.message.map(|(text, _)| text),
        }
    }

    /// `tools/bench switch`: flip through the Siblings and time each switch.
    fn drive_bench(&mut self, ctx: &egui::Context) {
        let Some(bench) = &mut self.bench_nav else {
            return;
        };
        let scanned = self.navigation.is_scanned();
        let Some(command) = bench.frame(ctx, scanned, self.path.as_deref()) else {
            return;
        };
        self.execute(command);
        let bench = self.bench_nav.as_mut().expect("checked above");
        if self.message.is_some() {
            bench.at_end(ctx);
        }
        bench.check(self.path.as_deref());
    }

    /// Drop the transient message once it has been up long enough.
    fn expire_message(&mut self, ctx: &egui::Context) {
        if let Some((_, posted)) = self.message {
            let left = MESSAGE_DURATION.saturating_sub(posted.elapsed());
            if left.is_zero() {
                self.message = None;
            } else {
                ctx.request_repaint_after(left);
            }
        }
    }
}

impl eframe::App for ZimgedApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        for command in crate::input::commands(&ctx) {
            self.execute(command);
        }
        // A later frame, not a second pass of the same one: that frame has been painted.
        if let FirstPaint::Drawn(frame) = self.first_paint
            && ctx.cumulative_frame_nr() > frame
        {
            self.first_paint = FirstPaint::Done;
            self.after_first_paint(&ctx);
        }
        self.poll_decode(&ctx);
        self.expire_message(&ctx);
        self.drive_bench(&ctx);

        status_bar::show(ui, &self.status_bar());
        self.canvas = Some(canvas::show(ui, self.shown.as_ref(), self.error.as_deref()));

        if self.first_paint == FirstPaint::Waiting && self.shown.is_some() {
            // Same convention as the spike: the mark is taken while building the frame that draws
            // the image; eframe paints and hands it to the compositor right after this returns.
            crate::bench::mark("first_image_painted");
            self.first_paint = FirstPaint::Drawn(ctx.cumulative_frame_nr());
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

fn upload(ctx: &egui::Context, image: &DisplayImage) -> egui::TextureHandle {
    let pixels = egui::ColorImage::from_rgb(
        [image.size.width as usize, image.size.height as usize],
        &image.rgb,
    );
    ctx.load_texture("image", pixels, egui::TextureOptions::LINEAR)
}
