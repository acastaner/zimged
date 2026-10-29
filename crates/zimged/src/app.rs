//! The eframe app: state, the per-frame loop, and command execution.
//!
//! Keep this file thin. Chrome lives in `ui/` (one module per bar), commands in `commands.rs`,
//! keyboard mapping in `input.rs`, the editor (Document, tools, composite) in `editor.rs`, and
//! domain state in `zimged-core`. `ui()` polls background work, maps keys to commands, then lays
//! out the panels.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use zimged_codec::{DisplayImage, Size};
use zimged_core::Document;
use zimged_platform::{Config, Platform};
use zimged_render::Composite;

use crate::Command;
use crate::decode::{DecodeResult, PendingDecode};
use crate::editor::{DisplayPixels, Editor, VIEW_ONLY_MESSAGE};
use crate::tools::ToolId;
use crate::ui::{canvas, property_bar, status_bar, tool_strip};

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
    /// The display-size pixels, which the Skia composite draws from.
    pub display: DisplayPixels,
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
    zoom: Zoom,
    /// The canvas as laid out in the last frame.
    canvas: Option<canvas::Layout>,
    first_paint: FirstPaint,
    editor: Editor,
}

impl ZimgedApp {
    pub fn new(cc: &eframe::CreationContext<'_>, launch: Launch) -> Self {
        if let Some(pending) = &launch.first_image {
            pending.wake(&cc.egui_ctx);
        }
        Self {
            platform: launch.platform,
            config: launch.config,
            pending: launch.first_image,
            shown: None,
            error: None,
            zoom: Zoom::Fit,
            canvas: None,
            first_paint: FirstPaint::Waiting,
            editor: Editor::new(),
        }
    }

    pub fn execute(&mut self, command: Command) {
        match command {
            Command::Open(path) => self.open(&path),
            Command::SelectTool(tool) => self.editor.select_tool(tool),
            Command::Undo => self.editor.undo(),
            Command::Redo => self.editor.redo(),
            Command::Delete => {
                // With nothing selected, Delete trashes the file (#22).
                self.editor.delete_selection();
            }
            Command::SetProperty { property, merge } => self.editor.set_property(property, merge),
        }
    }

    /// Is background work still pending (a decode, …)? Tests wait for this to clear.
    pub fn is_busy(&self) -> bool {
        self.pending.is_some() || self.editor.is_busy()
    }

    // ---- What tests (and later the menus) read ----

    /// The open Document: the Annotations, the selection and the History.
    pub fn document(&self) -> Option<&Document> {
        self.editor.document()
    }

    pub fn tool(&self) -> ToolId {
        self.editor.tool()
    }

    /// Has Skia been initialised (§1.3: only on the first Annotation)?
    pub fn skia_initialised(&self) -> bool {
        self.editor.skia_initialised()
    }

    /// The last Skia composite of the canvas, while there are Annotations (§7.2).
    pub fn composite(&self) -> Option<&Composite> {
        self.editor.composite()
    }

    /// The selection box (dashed, with 8 handles) as painted last frame, in points.
    pub fn selection_box(&self) -> Option<egui::Rect> {
        self.editor.selection_box()
    }

    pub fn is_view_only(&self) -> bool {
        self.editor.is_view_only()
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
        self.show_decoded(ctx, &path, result);
    }

    fn show_decoded(&mut self, ctx: &egui::Context, path: &Path, result: DecodeResult) {
        let file_name = file_name(path);
        match result {
            Ok(image) => {
                let texture = upload(ctx, &image);
                self.editor.open(path, image.original_size);
                self.shown = Some(ShownImage {
                    file_name,
                    original_size: image.original_size,
                    texture,
                    display: DisplayPixels {
                        key: next_display_key(),
                        size: image.size,
                        rgb: Arc::new(image.rgb),
                    },
                });
                self.error = None;
                self.zoom = Zoom::Fit;
            }
            Err(reason) => {
                self.shown = None;
                self.editor.close();
                self.error = Some(format!("Can't open {file_name}: {reason}"));
            }
        }
    }

    /// §1.3 step 5: runs once, on the frame after the first image was painted. The Sibling scan,
    /// directory watcher, prefetch, and UI font loading start from here, never before.
    fn after_first_paint(&mut self, _ctx: &egui::Context) {}

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
            modified: self.document().is_some_and(|d| d.is_modified()),
            pixel_size: self.shown.as_ref().map(|s| s.original_size),
            zoom: self.shown.as_ref().map(|_| self.zoom),
            message: self.is_view_only().then_some(VIEW_ONLY_MESSAGE),
        }
    }

    fn property_bar(&mut self, ui: &mut egui::Ui) {
        let (style, properties, selection) = self.editor.property_target();
        let tool = self.editor.tool();
        let out = property_bar::show(
            ui,
            &property_bar::PropertyBar {
                style: &style,
                properties: if self.editor.can_edit() {
                    &properties
                } else {
                    &[]
                },
                selection,
                hint: tool.hint(),
            },
        );
        for property in out.changes {
            let command = self.editor.property_change(property);
            self.execute(command);
        }
        self.editor.property_interaction(out.active);
        if out.delete {
            self.execute(Command::Delete);
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
        self.editor.poll(&ctx);
        for command in crate::input::keyboard_commands(&ctx) {
            self.execute(command);
        }

        // The menu bar (#23) goes above the property bar.
        self.property_bar(ui);
        if let Some(command) = tool_strip::show(ui, self.editor.tool(), self.editor.can_edit()) {
            self.execute(command);
        }
        status_bar::show(ui, &self.status_bar());
        let editor = &mut self.editor;
        let display = self.shown.as_ref().map(|s| &s.display);
        self.canvas = Some(canvas::show(
            ui,
            self.shown.as_ref(),
            self.error.as_deref(),
            |ui, layout, response| editor.canvas(ui.ctx(), layout, response, display),
        ));

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

fn next_display_key() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
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
