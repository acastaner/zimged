//! The eframe app: state, the per-frame loop, and command execution.
//!
//! Keep this file thin. Chrome lives in `ui/` (one module per bar), commands in `commands.rs`,
//! input mapping in `input.rs`, the editor (Document, tools, composite) in `editor.rs`, and domain
//! state in `zimged-core`. `ui()` polls background work, then lays out the panels.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use zimged_codec::{DisplayImage, Size, fit_within};
use zimged_core::Document;
use zimged_platform::{Config, Platform};
use zimged_render::Composite;
use zimged_siblings::{Decoded, Direction};

use crate::Command;
use crate::animation::Animation;
use crate::config_save::ConfigSaver;
use crate::decode::PendingDecode;
use crate::editor::{DisplayPixels, Editor, MAX_EDIT_PIXELS, SaveStamp, VIEW_ONLY_MESSAGE};
use crate::pan::PanGesture;
use crate::save::{PendingSave, SaveJob};
use crate::siblings::{Navigation, Step};
use crate::tools::ToolId;
use crate::ui::{canvas, menu_bar, property_bar, status_bar, tool_strip, unsaved_dialog};
use crate::view::{View, ZOOM_STEP, Zoom};

/// The largest Image whose full-resolution pixels are decoded (§4.7). Larger ones are view-only and
/// zoom in on their display-size pixels.
const FULL_RESOLUTION_LIMIT: u64 = MAX_EDIT_PIXELS;

/// The status-bar message after Delete trashed the file (§2, §3.1).
pub const MOVED_TO_TRASH: &str = "Moved to trash";

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
    /// The cap on pixels the History holds, in bytes (§4.8). Tests lower it.
    pub history_pixel_cap: usize,
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
            history_pixel_cap: zimged_core::HISTORY_PIXEL_CAP,
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
    /// `display`'s pixels, shared with egui (no copy). Tests read them; so will the on-screen
    /// composite (§7.2).
    display_pixels: Arc<egui::ColorImage>,
    /// Identifies `display_pixels` for the Skia renderer's cache (a new key per decode).
    display_key: u64,
    /// The full-resolution decode, once zoomed past the display-size one (§7.2). Capped at the GPU's
    /// largest texture.
    full: Option<egui::TextureHandle>,
    /// The largest decode asked for so far, so a size that can't be bettered isn't asked for again.
    requested: Size,
    /// A GIF or WebP whose frames haven't been looked at yet (§3.4): they are, after the first paint.
    may_animate: bool,
    /// Its frames, playing (or still decoding), until it turns out to be a still image.
    animation: Option<Animation>,
    /// The file was deleted from outside zimged while on screen (§3.2): the Image stays open and is
    /// treated as unsaved, since its pixels now exist only here.
    deleted_from_disk: bool,
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

/// A file Delete moved to the trash, until the Sibling replacing it is on screen.
#[derive(Clone)]
struct Trashed {
    path: PathBuf,
    /// The Sibling scan hadn't finished: the move happens when it does.
    waits_for_scan: bool,
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

/// A save running on the worker thread (#34).
struct Saving {
    pending: PendingSave,
    stamp: SaveStamp,
}

/// What waits on the unsaved-changes prompt (#35, §6.4): done right away without unsaved changes,
/// else after Save (once the file is written) or Discard.
#[derive(Clone, Debug)]
enum Proceed {
    Open(PathBuf),
    Go(Direction),
    Close,
}

/// The unsaved-changes prompt while it is up.
struct UnsavedPrompt {
    file_name: String,
    then: Proceed,
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

/// The canvas text when there is no Image (§2).
pub const EMPTY_WINDOW_HINT: &str = "Open (Ctrl+O), drop or paste (Ctrl+V) an image";

pub struct ZimgedApp {
    /// To wake the UI from background work.
    ctx: egui::Context,
    platform: Platform,
    config: Config,
    config_saver: ConfigSaver,
    pending: Option<PendingDecode>,
    /// The file on screen (or that failed to open, when `error` is set).
    path: Option<PathBuf>,
    shown: Option<ShownImage>,
    /// "Can't open <name>: <reason>" (§5.2), shown on the canvas instead of an image.
    error: Option<String>,
    upgrade: Option<Upgrade>,
    view: View,
    /// The pan drag in progress (Space+left or middle button).
    pan: PanGesture,
    /// Wanted fullscreen state (F11). Chrome is hidden while it's on.
    fullscreen: bool,
    /// The last fullscreen state sent to the window, and the last one the window reported.
    fullscreen_sent: Option<bool>,
    fullscreen_seen: Option<bool>,
    /// The canvas as laid out in the last frame.
    canvas: Option<canvas::Layout>,
    first_paint: FirstPaint,
    /// The open dialog while it is up: receives the chosen path, or `None` on Cancel.
    open_dialog: Option<mpsc::Receiver<Option<PathBuf>>>,
    navigation: Navigation,
    /// The transient status-bar message and when it was posted.
    message: Option<(String, Instant)>,
    /// The file just trashed, while the Sibling replacing it isn't on screen yet.
    trashed: Option<Trashed>,
    /// Benchmark driver for Sibling switching (`tools/bench switch`).
    bench_nav: Option<crate::bench::NavBench>,
    editor: Editor,
    /// The Save As dialog while it is up (#34).
    save_dialog: Option<mpsc::Receiver<Option<PathBuf>>>,
    saving: Option<Saving>,
    /// A save asked for while the Document waits for its pixels (a queued crop): started then.
    queued_save: Option<(PathBuf, zimged_codec::SaveFormat)>,
    /// "Save changes?" while it is up (#35).
    unsaved_prompt: Option<UnsavedPrompt>,
    /// The action the prompt's Save is holding until the save (and any Save As dialog) is done.
    after_save: Option<Proceed>,
    /// The window is closing: a close request was let through, or sent after the prompt.
    closing: bool,
}

impl ZimgedApp {
    pub fn new(cc: &eframe::CreationContext<'_>, launch: Launch) -> Self {
        if let Some(pending) = &launch.first_image {
            pending.wake(&cc.egui_ctx);
        }
        // Ctrl+plus / minus / 0 would scale the whole UI; the image zoom is on plain +, - and 0.
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let mut editor = Editor::with_tool_styles(&launch.config.tools);
        editor.set_history_pixel_cap(launch.history_pixel_cap);
        Self {
            ctx: cc.egui_ctx.clone(),
            config_saver: ConfigSaver::new(launch.platform.config.clone()),
            platform: launch.platform,
            config: launch.config,
            pending: launch.first_image,
            path: None,
            shown: None,
            error: None,
            upgrade: None,
            view: View::fit(),
            pan: PanGesture::default(),
            fullscreen: false,
            fullscreen_sent: None,
            fullscreen_seen: None,
            canvas: None,
            first_paint: FirstPaint::Waiting,
            open_dialog: None,
            navigation: Navigation::new(launch.cache_cap),
            message: None,
            trashed: None,
            bench_nav: crate::bench::NavBench::from_env(),
            editor,
            save_dialog: None,
            saving: None,
            queued_save: None,
            unsaved_prompt: None,
            after_save: None,
            closing: false,
        }
    }

    pub fn execute(&mut self, command: Command) {
        match command {
            // With unsaved changes, opening (the dialog or a drop) and moving to a Sibling first ask
            // Save / Discard / Cancel (§6.4). Every open comes through here.
            Command::Open(path) => self.proceed_or_ask(Proceed::Open(path)),
            Command::OpenDialog => self.show_open_dialog(),
            Command::NextSibling => self.proceed_or_ask(Proceed::Go(Direction::Forward)),
            Command::PreviousSibling => self.proceed_or_ask(Proceed::Go(Direction::Backward)),
            Command::ZoomIn => self.zoom(ZOOM_STEP, None),
            Command::ZoomOut => self.zoom(1.0 / ZOOM_STEP, None),
            Command::ZoomAround { factor, around } => self.zoom(factor, Some(around)),
            Command::Pan(delta) => self.pan_by(delta),
            Command::ZoomFit => self.view = View::fit(),
            Command::ToggleFullscreen => self.fullscreen = !self.fullscreen,
            Command::SetAllowMultipleInstances(allow) => {
                self.update_config(|config| config.allow_multiple_instances = allow);
            }
            Command::SelectTool(tool) => self.editor.select_tool(tool),
            Command::Undo => self.editor.undo(),
            Command::Redo => self.editor.redo(),
            Command::Delete => self.delete(),
            Command::SetProperty { property, merge } => {
                self.editor.set_property(property, merge);
                // §4.3, §8.3: the tools' defaults persist between runs.
                let styles = self.editor.tool_styles_config();
                self.update_config(|config| config.tools.extend(styles));
            }
            Command::Save => self.save(),
            Command::SaveAs => self.show_save_dialog(),
            Command::ApplyCrop => self.editor.apply_crop(),
            Command::CancelCrop => self.editor.cancel_crop(),
            Command::Duplicate => self.editor.duplicate_selection(),
            Command::Restack(restack) => self.editor.restack(restack),
        }
    }

    // ---- The editor, as tests (and later the menus) see it ----

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

    /// The crop tool's region, in Image pixels (§4.1 C).
    pub fn crop_region(&self) -> Option<zimged_core::Rect> {
        self.editor.crop_region()
    }

    /// The Image's size in pixels: the Document's (it changes with Image operations, §4.7), else
    /// the decoded file's.
    fn image_size(&self) -> Option<Size> {
        let shown = self.shown.as_ref()?;
        Some(
            self.document()
                .map(|d| Size::new(d.width(), d.height()))
                .unwrap_or(shown.original_size),
        )
    }

    /// The selection box (dashed, with 8 handles) as painted last frame, in points.
    pub fn selection_box(&self) -> Option<egui::Rect> {
        self.editor.selection_box()
    }

    /// The select tool's rubber band (dashed) as painted last frame, in points.
    pub fn rubber_band(&self) -> Option<egui::Rect> {
        self.editor.rubber_band()
    }

    /// Over 100 MP: the tools are disabled (§4.7).
    pub fn is_view_only(&self) -> bool {
        self.editor.is_view_only()
    }

    /// The settings in effect (§8.3), including changes not yet written to `config.toml`.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Is background work still pending (a decode, an answer from the open dialog, a config write,
    /// the Sibling scan, prefetch, …)? Tests wait for this to clear.
    pub fn is_busy(&self) -> bool {
        self.pending.is_some()
            || self.upgrade.is_some()
            || self.open_dialog.is_some()
            || self.config_saver.is_busy()
            || self
                .shown
                .as_ref()
                .is_some_and(|s| s.animation.as_ref().is_some_and(Animation::is_decoding))
            || self.editor.is_busy()
            || self.navigation.is_busy()
            || self.save_dialog.is_some()
            || self.saving.is_some()
            || self.queued_save.is_some()
            || self.trashed.as_ref().is_some_and(|t| t.waits_for_scan)
            || self.after_save.is_some()
    }

    /// Does the Image have unsaved changes (` •` after the file name)? Edits to the Document, or a
    /// file deleted from outside zimged (§3.2). The Save / Discard / Cancel prompt asks whenever
    /// this is true, and saving writes the file back.
    pub fn is_modified(&self) -> bool {
        self.document().is_some_and(|d| d.is_modified())
            || self.shown.as_ref().is_some_and(|s| s.deleted_from_disk)
    }

    /// Is the window closing (a close let through, or confirmed in the unsaved-changes prompt)?
    pub fn is_closing(&self) -> bool {
        self.closing
    }

    /// Is a modal dialog up? Window input (shortcuts, wheel, drops, pan) waits meanwhile.
    fn dialog_up(&self) -> bool {
        self.unsaved_prompt.is_some()
    }

    // ---- The unsaved-changes flow (#35, §6.4) ----

    /// Do `then` now, or, with unsaved changes, ask Save / Discard / Cancel first. Asked again while
    /// the prompt is up (or its save runs), nothing happens.
    fn proceed_or_ask(&mut self, then: Proceed) {
        if self.unsaved_prompt.is_some() || self.after_save.is_some() {
            return;
        }
        let asks = self.is_modified()
            && match &then {
                // Only when the move would leave this Image (not at an end of the list).
                Proceed::Go(direction) => self
                    .path
                    .as_deref()
                    .is_some_and(|from| self.navigation.would_move(from, *direction)),
                Proceed::Open(_) | Proceed::Close => true,
            };
        if !asks {
            return self.proceed(then);
        }
        let file_name = self
            .shown
            .as_ref()
            .map(|s| s.file_name.clone())
            .or_else(|| self.path.as_deref().map(file_name))
            .unwrap_or_default();
        self.unsaved_prompt = Some(UnsavedPrompt { file_name, then });
    }

    fn proceed(&mut self, then: Proceed) {
        match then {
            Proceed::Open(path) => self.open(&path),
            Proceed::Go(direction) => self.go(direction),
            Proceed::Close => {
                self.closing = true;
                self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// The window manager asks to close the window: hold it while there are unsaved changes.
    fn on_close_requested(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) || self.closing {
            return;
        }
        if self.is_modified() || self.unsaved_prompt.is_some() || self.after_save.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.proceed_or_ask(Proceed::Close);
        } else {
            self.closing = true;
        }
    }

    /// The prompt, while it is up, and the user's answer.
    fn unsaved_prompt(&mut self, ctx: &egui::Context) {
        let Some(prompt) = &self.unsaved_prompt else {
            return;
        };
        let Some(answer) = unsaved_dialog::show(ctx, &prompt.file_name) else {
            return;
        };
        let prompt = self.unsaved_prompt.take().expect("checked above");
        match answer {
            unsaved_dialog::Answer::Cancel => {}
            unsaved_dialog::Answer::Discard => self.proceed(prompt.then),
            unsaved_dialog::Answer::Save => {
                self.after_save = Some(prompt.then);
                self.save();
            }
        }
    }

    /// Once the prompt's save is over: proceed if it left nothing unsaved; else (it failed, or
    /// Save As was cancelled) stay put.
    fn poll_after_save(&mut self) {
        if self.after_save.is_none()
            || self.saving.is_some()
            || self.save_dialog.is_some()
            || self.queued_save.is_some()
        {
            return;
        }
        // After Save As the window shows the new file: move on from it once its folder is known.
        if matches!(self.after_save, Some(Proceed::Go(_))) && !self.navigation.is_scanned() {
            return;
        }
        let then = self.after_save.take().expect("checked above");
        if !self.is_modified() {
            self.proceed(then);
        }
    }

    /// Is the Image's directory being watched for changes (§3.2)?
    pub fn is_watching_directory(&self) -> bool {
        self.navigation.is_watching()
    }

    /// The files in the display-size cache (§3.3) and their total size in bytes.
    pub fn cached_siblings(&self) -> (Vec<PathBuf>, usize) {
        self.navigation.cached()
    }

    /// The box display-size decodes fit into: the canvas in physical pixels.
    fn display_box(&self) -> Size {
        self.canvas
            .map(|c| c.area_px())
            .unwrap_or_else(|| startup_display_box(&self.config))
    }

    /// Is the window fullscreen (or about to be)?
    pub fn is_fullscreen(&self) -> bool {
        self.fullscreen
    }

    fn zoom(&mut self, factor: f32, around: Option<egui::Pos2>) {
        let (Some(size), Some(layout)) = (self.image_size(), self.canvas) else {
            return;
        };
        self.view
            .zoom_by(factor, around, size, layout.area, layout.pixels_per_point);
    }

    fn pan_by(&mut self, delta: egui::Vec2) {
        let (Some(size), Some(layout)) = (self.image_size(), self.canvas) else {
            return;
        };
        self.view
            .pan_by(delta, size, layout.area, layout.pixels_per_point);
    }

    /// Does the pointer belong to panning this frame? The active tool leaves it alone while it does
    /// (Space and the middle button take precedence over tools).
    pub fn pan_owns_pointer(&self) -> bool {
        self.pan.owns_pointer(&self.ctx)
    }

    /// Change the settings and schedule a (debounced) write of `config.toml`.
    fn update_config(&mut self, change: impl FnOnce(&mut Config)) {
        let before = self.config.clone();
        change(&mut self.config);
        if self.config != before {
            self.config_saver.changed();
        }
    }

    /// Remember the window's restored size and maximised state (§2, §8.3). A fullscreen or
    /// minimised window isn't recorded.
    fn track_window(&mut self, ctx: &egui::Context) {
        let (inner, maximised, fullscreen, minimised) = ctx.input(|i| {
            let viewport = i.viewport();
            (
                viewport.inner_rect,
                viewport.maximized,
                viewport.fullscreen,
                viewport.minimized,
            )
        });
        // `self.fullscreen` covers the frames before the window reports entering fullscreen; the
        // viewport flag covers those before it reports leaving it.
        if self.fullscreen || fullscreen == Some(true) || minimised == Some(true) {
            return;
        }
        self.update_config(|config| {
            if let Some(maximised) = maximised {
                config.window.maximised = maximised;
            }
            if !config.window.maximised
                && let Some(inner) = inner
            {
                config.window.width = inner.width().round();
                config.window.height = inner.height().round();
            }
        });
    }

    /// Ctrl+O. Only one open dialog at a time: asking again while it is up does nothing.
    fn show_open_dialog(&mut self) {
        if self.open_dialog.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.open_dialog = Some(rx);
        let ctx = self.ctx.clone();
        self.platform.dialogs.pick_image(Box::new(move |chosen| {
            let _ = tx.send(chosen);
            ctx.request_repaint();
        }));
    }

    fn poll_open_dialog(&mut self) {
        let Some(rx) = &self.open_dialog else {
            return;
        };
        let chosen = match rx.try_recv() {
            Ok(chosen) => chosen,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => None,
        };
        self.open_dialog = None;
        if let Some(path) = chosen {
            self.execute(Command::Open(path));
        }
    }

    fn open(&mut self, path: &Path) {
        let display_box = self.display_box();
        self.navigation.cancel();
        // The file opened replaces a trashed one; no move off it any more.
        self.trashed = None;
        self.message = None;
        // The previous Image stays on screen until the new one is decoded (§3.3).
        self.pending = Some(PendingDecode::spawn(path.to_owned(), move || display_box));
    }

    /// Move to the next or previous Sibling (§3.1). Callers go through
    /// [`Self::proceed_or_ask`], which asks Save / Discard / Cancel first with unsaved changes (§6.4).
    fn go(&mut self, direction: Direction) {
        let Some(from) = self.path.clone() else {
            return;
        };
        self.message = None;
        match self.navigation.go(&from, direction, self.display_box()) {
            Step::End(message) => self.post_message(message),
            step => self.take_step(step),
        }
    }

    /// Show the Sibling a move landed on (now, or once its decode arrives).
    fn take_step(&mut self, step: Step) {
        match step {
            Step::Show(path, decoded) => {
                self.pending = None;
                let ctx = self.ctx.clone();
                self.show_decoded(&ctx, path, decoded);
            }
            Step::Pending => self.pending = None,
            Step::End(_) | Step::NotReady => {}
        }
    }

    /// Follow changes to the Image's folder made from outside zimged (§3.2).
    fn follow_directory(&mut self, ctx: &egui::Context) {
        let display_box = self.display_box();
        let deleted = self
            .navigation
            .follow_changes(ctx, self.path.as_deref(), display_box);
        if deleted && let Some(shown) = &mut self.shown {
            shown.deleted_from_disk = true;
        }
    }

    fn post_message(&mut self, message: impl Into<String>) {
        self.message = Some((message.into(), Instant::now()));
    }

    // ---- Save (#34, §5.1, §5.3) ----

    /// Ctrl+S: write the Image back in its own format; Save As (defaulting to PNG) for a read-only
    /// one (§5.1). A clipboard Image (no file) will go to Save As too (§6.1).
    fn save(&mut self) {
        let Some(path) = self.path.clone().filter(|_| self.shown.is_some()) else {
            return;
        };
        match crate::save::in_place_format(&path) {
            Some(format) => self.save_to(path, format),
            None => self.show_save_dialog(),
        }
    }

    /// Ctrl+Shift+S, or Ctrl+S on a read-only format. One save (or dialog) at a time.
    fn show_save_dialog(&mut self) {
        if self.save_dialog.is_some() || self.saving.is_some() || self.shown.is_none() {
            return;
        }
        let Some(path) = &self.path else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        self.save_dialog = Some(rx);
        let ctx = self.ctx.clone();
        self.platform.dialogs.save_image(
            &crate::save::suggestion(path),
            Box::new(move |chosen| {
                let _ = tx.send(chosen);
                ctx.request_repaint();
            }),
        );
    }

    fn poll_save_dialog(&mut self) {
        let Some(rx) = &self.save_dialog else {
            return;
        };
        let chosen = match rx.try_recv() {
            Ok(chosen) => chosen,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => None,
        };
        self.save_dialog = None;
        if let Some(chosen) = chosen {
            let (target, format) = crate::save::chosen_target(chosen);
            self.save_to(target, format);
        }
    }

    /// Flatten and write the Image to `target` on the worker thread (§7.3).
    fn save_to(&mut self, target: PathBuf, format: zimged_codec::SaveFormat) {
        if self.saving.is_some() {
            return;
        }
        if self.editor.save_waits() {
            self.queued_save = Some((target, format));
            return;
        }
        // #38: when the Image is an edited animation and `target` is its own file, ask "Overwrite
        // animated image?" (Cancel, Save As..., Overwrite; §6.4) here, before anything is written.
        let Some(source) = self.editor.save_source() else {
            return;
        };
        let job = SaveJob {
            drawing: source.drawing,
            source: Some(source.path),
            fallback: self
                .shown
                .as_ref()
                .filter(|s| {
                    let [w, h] = s.display_pixels.size;
                    Size::new(w as u32, h as u32) == s.original_size
                })
                .map(|s| s.display_pixels.clone()),
            target,
            format,
            jpeg_quality: self.config.jpeg_quality,
        };
        let pending = PendingSave::spawn_save(job);
        pending.wake(&self.ctx);
        self.saving = Some(Saving {
            pending,
            stamp: source.stamp,
        });
    }

    fn poll_save(&mut self, ctx: &egui::Context) {
        if self.queued_save.is_some() && !self.editor.save_waits() {
            let (target, format) = self.queued_save.take().expect("checked above");
            self.save_to(target, format);
        }
        let Some(saving) = &self.saving else {
            return;
        };
        saving.pending.wake(ctx);
        let Some(result) = saving.pending.take() else {
            return;
        };
        let saving = self.saving.take().expect("checked above");
        let target = saving.pending.path().to_owned();
        match result {
            Ok(saved) => {
                self.editor.saved(saving.stamp, &target, saved.decoded);
                self.saved_as(ctx, &target);
            }
            Err(reason) => {
                self.post_message(format!("Can't save {}: {reason}", file_name(&target)));
            }
        }
    }

    /// Delete (§3.1, §4.2): the selected Annotations go and the file stays; with nothing selected,
    /// the file goes to the trash. Never in the middle of a drawing gesture.
    fn delete(&mut self) {
        if self.editor.in_gesture() || self.editor.delete_selection() {
            return;
        }
        self.trash_file();
    }

    /// Move the file on screen (or that failed to open) to the system trash and show the next
    /// Sibling, or the previous one at the end of the list (§3.1). It is taken out of the Sibling
    /// list right away, so #21's watcher seeing it vanish has nothing left to do for it.
    /// No unsaved-changes prompt (#35): §6.4 asks before moving to a Sibling, opening, dropping or
    /// closing; trashing is an explicit "throw this file away", and saving it first is pointless.
    fn trash_file(&mut self) {
        let Some(path) = self.path.clone() else {
            return;
        };
        // Already trashed: the Sibling replacing it is on its way.
        if self.trashed.as_ref().is_some_and(|t| t.path == path) {
            return;
        }
        if let Err(reason) = self.platform.trash.trash(&path) {
            let name = file_name(&path);
            self.post_message(format!("Can't move {name} to trash: {reason}"));
            return;
        }
        self.navigation.forget(&path);
        self.trashed = Some(Trashed {
            path,
            waits_for_scan: false,
        });
        self.leave_trashed();
        self.post_message(MOVED_TO_TRASH);
    }

    /// Move off the trashed file: to the Sibling after it, else the one before it, else to the
    /// empty window. Waits for the scan when the Siblings aren't known yet.
    fn leave_trashed(&mut self) {
        let Some(trashed) = self.trashed.clone() else {
            return;
        };
        let display_box = self.display_box();
        let step = match self
            .navigation
            .go(&trashed.path, Direction::Forward, display_box)
        {
            Step::End(_) => self
                .navigation
                .go(&trashed.path, Direction::Backward, display_box),
            step => step,
        };
        match step {
            Step::NotReady if !self.navigation.is_scanned() => {
                let ctx = self.ctx.clone();
                self.navigation.ensure_started(&ctx, &trashed.path);
                self.trashed = Some(Trashed {
                    waits_for_scan: true,
                    ..trashed
                });
            }
            Step::End(_) | Step::NotReady => self.show_nothing(),
            step => {
                if let Some(trashed) = &mut self.trashed {
                    trashed.waits_for_scan = false;
                }
                self.take_step(step);
            }
        }
    }

    /// After a save. Save As to another file: the window now shows that file (its name, its
    /// folder's Siblings).
    fn saved_as(&mut self, ctx: &egui::Context, target: &Path) {
        let Some(shown) = self.shown.as_mut() else {
            return;
        };
        // The file is back on disk (#21: it may have been deleted from outside).
        shown.deleted_from_disk = false;
        if shown.path == target {
            return;
        }
        shown.path = target.to_owned();
        shown.file_name = file_name(target);
        // A zoom upgrade in flight belongs to the old path: drop it.
        self.upgrade = None;
        self.path = Some(target.to_owned());
        self.navigation.start(ctx, target);
    }

    /// Back to the empty window (the folder's last file was trashed).
    fn show_nothing(&mut self) {
        self.path = None;
        self.shown = None;
        self.error = None;
        self.upgrade = None;
        self.trashed = None;
        self.view = View::fit();
    }

    fn poll_decode(&mut self, ctx: &egui::Context) {
        if let Some((path, decoded)) = self
            .navigation
            .poll(self.path.as_deref(), self.display_box())
        {
            self.show_decoded(ctx, path, decoded);
        }
        if self.trashed.as_ref().is_some_and(|t| t.waits_for_scan) && self.navigation.is_scanned() {
            self.leave_trashed();
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
        self.path = Some(path.clone());
        self.trashed = None;
        match decoded {
            Ok(image) => {
                self.editor.open(&path, image.original_size);
                let (display, display_pixels) = upload(ctx, &image, Pixels::Display);
                self.shown = Some(ShownImage {
                    path,
                    file_name,
                    original_size: image.original_size,
                    display,
                    display_pixels,
                    display_key: next_display_key(),
                    full: None,
                    requested: image.size,
                    may_animate: image.may_animate,
                    animation: None,
                    deleted_from_disk: false,
                });
                self.upgrade = None;
                self.error = None;
                self.view = View::fit();
            }
            Err(reason) => {
                self.shown = None;
                self.editor.close();
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
        if shown.animation.is_some() {
            // The animation's frames are drawn instead.
            return;
        }
        let (texture, pixels) = upload(ctx, &image, upgrade.pixels);
        match upgrade.pixels {
            Pixels::Display => {
                shown.display = texture;
                shown.display_pixels = pixels;
                shown.display_key = next_display_key();
            }
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
        // After an Image operation the canvas draws the Document's own pixels, not the file's.
        if self.document().is_some_and(|d| !d.is_original_image()) {
            return;
        }
        let (Some(layout), Some(shown)) = (self.canvas, self.shown.as_mut()) else {
            return;
        };
        if shown.may_animate || shown.animation.is_some() {
            // An animation's frames are decoded at the display size only.
            return;
        }
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

    /// Start decoding the frames of a GIF or WebP, after the first paint (§1.3).
    fn start_animation(&mut self, ctx: &egui::Context) {
        if self.first_paint != FirstPaint::Done {
            return;
        }
        let (Some(layout), Some(shown)) = (self.canvas, self.shown.as_mut()) else {
            return;
        };
        if !shown.may_animate {
            return;
        }
        shown.may_animate = false;
        shown.full = None;
        shown.animation = Some(Animation::spawn(shown.path.clone(), layout.area_px(), ctx));
    }

    /// How many frames of an animation are held (decoded, CPU pixels): 0 without one (§3.4).
    pub fn animation_frames(&self) -> usize {
        self.shown
            .as_ref()
            .and_then(|s| s.animation.as_ref())
            .map_or(0, Animation::frame_count)
    }

    /// Move the animation on to the frame due now (§3.4); forget it once it turns out to be a still.
    fn play_animation(&mut self, ctx: &egui::Context) {
        let Some(shown) = self.shown.as_mut() else {
            return;
        };
        let Some(animation) = shown.animation.as_mut() else {
            return;
        };
        let now = ctx.input(|i| i.time);
        if let Some(frame) = animation.tick(now, ctx) {
            shown
                .display
                .set(frame.clone(), egui::TextureOptions::LINEAR);
            shown.display_pixels = frame;
            shown.display_key = next_display_key();
        }
        if animation.is_still() {
            shown.animation = None;
        }
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

    /// §1.3 step 5: runs once, on the frame after the first image was painted. The Sibling scan and
    /// directory watcher (both in `navigation.start`), prefetch, and UI font loading start from here, never before.
    fn after_first_paint(&mut self, ctx: &egui::Context) {
        if let Some(path) = self.path.clone() {
            self.navigation.start(ctx, &path);
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

    /// The text the canvas shows instead of an Image: the decode error, or the empty-window hint.
    fn canvas_message(&self) -> Option<&str> {
        if self.shown.is_some() {
            return None;
        }
        if let Some(error) = &self.error {
            return Some(error);
        }
        // While the first Image decodes, the canvas stays blank rather than flash the hint.
        self.pending.is_none().then_some(EMPTY_WINDOW_HINT)
    }

    /// A pixel of the display-size decode on screen (after orientation and colour conversion):
    /// `None` when no Image is shown or `(x, y)` is outside it.
    pub fn shown_pixel(&self, x: u32, y: u32) -> Option<egui::Color32> {
        let pixels = &self.shown.as_ref()?.display_pixels;
        let [w, h] = pixels.size;
        let (x, y) = (x as usize, y as usize);
        (x < w && y < h).then(|| pixels.pixels[y * w + x])
    }

    fn status_bar(&self) -> status_bar::StatusBar<'_> {
        status_bar::StatusBar {
            file_name: self.shown.as_ref().map(|s| s.file_name.as_str()),
            modified: self.is_modified(),
            position: self.path.as_deref().map(|p| self.navigation.position(p)),
            pixel_size: self.image_size(),
            zoom: self.shown.as_ref().map(|_| self.view.zoom),
            // #38 adds the frozen state.
            animation: self
                .shown
                .as_ref()
                .and_then(|s| s.animation.as_ref())
                .filter(|a| a.is_animated())
                .map(|_| status_bar::AnimationState::Playing),
            // The view-only notice stays until another Image; transient messages win.
            message: self
                .message
                .as_ref()
                .map(|(text, _)| text.as_str())
                .or(self.is_view_only().then_some(VIEW_ONLY_MESSAGE)),
        }
    }

    fn property_bar(&mut self, ui: &mut egui::Ui) {
        let (style, properties, selection) = self.editor.property_target();
        let crop = self
            .editor
            .crop_region()
            .map(|r| Size::new(r.width().round() as u32, r.height().round() as u32));
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
                crop,
                hint: self.editor.tool().hint(),
            },
        );
        if out.apply_crop {
            self.execute(Command::ApplyCrop);
        }
        if out.cancel_crop {
            self.execute(Command::CancelCrop);
        }
        for property in out.changes {
            let command = self.editor.property_change(property);
            self.execute(command);
        }
        self.editor.property_interaction(out.active);
        if out.delete {
            self.execute(Command::Delete);
        }
        if let Some(restack) = out.restack {
            self.execute(Command::Restack(restack));
        }
    }
}

impl eframe::App for ZimgedApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // A later frame, not a second pass of the same one: that frame has been painted.
        if let FirstPaint::Drawn(frame) = self.first_paint
            && ctx.cumulative_frame_nr() > frame
        {
            self.first_paint = FirstPaint::Done;
            self.after_first_paint(&ctx);
        }
        self.on_close_requested(&ctx);
        // A modal dialog takes the keys (Esc is its Cancel); the window behind it gets no input.
        if !self.dialog_up() {
            let canvas_area = self.canvas.map(|c| c.area);
            let mut commands = crate::input::commands(&ctx, canvas_area);
            commands.extend(self.pan.commands(&ctx, canvas_area));
            for command in commands {
                self.execute(command);
            }
        }
        self.unsaved_prompt(&ctx);
        self.poll_open_dialog();
        self.poll_save_dialog();
        self.poll_save(&ctx);
        self.poll_after_save();
        self.follow_directory(&ctx);
        self.poll_decode(&ctx);
        self.poll_upgrade(&ctx);
        self.play_animation(&ctx);
        self.editor.poll(&ctx);
        self.expire_message(&ctx);
        self.drive_bench(&ctx);
        self.sync_fullscreen(&ctx);
        self.track_window(&ctx);
        self.config_saver.poll(&ctx, &self.config);

        // Fullscreen hides all chrome (§2): the menu bar, property bar, tool strip and status bar.
        if !self.fullscreen {
            let menu = menu_bar::MenuBar {
                allow_multiple_instances: self.config.allow_multiple_instances,
            };
            if let Some(command) = menu_bar::show(ui, &menu) {
                self.execute(command);
            }
            self.property_bar(ui);
            if let Some(command) = tool_strip::show(ui, self.editor.tool(), self.editor.can_edit())
            {
                self.execute(command);
            }
            status_bar::show(ui, &self.status_bar());
        }
        // Laying out keeps the zoomed Image in bounds, so the view comes back updated.
        let mut view = self.view;
        let message = self.canvas_message().map(str::to_owned);
        // Space and the middle button pan; the tools leave the pointer alone meanwhile (#43).
        let tools_get_pointer = !self.pan_owns_pointer();
        let image_size = self.image_size();
        let editor = &mut self.editor;
        let display = self.shown.as_ref().map(|s| DisplayPixels {
            key: s.display_key,
            image: &s.display_pixels,
        });
        self.canvas = Some(canvas::show(
            ui,
            self.shown.as_ref(),
            image_size,
            &mut view,
            message.as_deref(),
            |ui, layout, response| {
                editor.canvas(ui.ctx(), layout, response, display, tools_get_pointer)
            },
        ));
        self.view = view;
        self.start_animation(&ctx);
        self.request_pixels(&ctx);

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

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.config_saver.flush(&self.config);
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

fn upload(
    ctx: &egui::Context,
    image: &DisplayImage,
    kind: Pixels,
) -> (egui::TextureHandle, Arc<egui::ColorImage>) {
    let pixels = Arc::new(color_image(image));
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
    (ctx.load_texture(name, pixels.clone(), options), pixels)
}

pub(crate) fn color_image(image: &DisplayImage) -> egui::ColorImage {
    let size = [image.size.width as usize, image.size.height as usize];
    match &image.pixels {
        zimged_codec::Pixels::Rgb(rgb) => egui::ColorImage::from_rgb(size, rgb),
        zimged_codec::Pixels::Rgba(rgba) => egui::ColorImage::from_rgba_unmultiplied(size, rgba),
    }
}

/// A fresh cache key for a display-size decode's pixels (see [`DisplayPixels`]).
fn next_display_key() -> u64 {
    static NEXT_KEY: AtomicU64 = AtomicU64::new(1);
    NEXT_KEY.fetch_add(1, Ordering::Relaxed)
}

fn texture_size(texture: &egui::TextureHandle) -> Size {
    let [w, h] = texture.size();
    Size::new(w as u32, h as u32)
}
