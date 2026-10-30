//! The editor (§4): the open Document, the active tool and the tools' styles, the full-resolution
//! decode, and the Skia composite of the canvas.
//!
//! The app owns one [`Editor`]. Per Image state ([`Edited`]) is replaced on every open, which is
//! what clears the History when moving to another Image (§4.8). The active tool and each tool's
//! style outlive it.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use zimged_codec::Size;
use zimged_core::{
    Document, HISTORY_PIXEL_CAP, Inset, PixelData, Pixels, Point, Property, PropertyKind, Rect,
    Restack, Shape, Style, Vector,
};
use zimged_render::{Composite, Renderer, Source, SourcePixels, View};

use crate::decode::PendingFullDecode;
use crate::tools::{Pointer, Tool, ToolContext, ToolId};
use crate::ui::canvas::{EditorPaint, Layout};

/// Editing needs the full-resolution decode, allowed up to 100 MP (§4.7).
pub const MAX_EDIT_PIXELS: u64 = 100_000_000;

pub const VIEW_ONLY_MESSAGE: &str = "View only: image too large to edit";

/// How far Ctrl+D moves the copies, right and down, in Image pixels (§4.2).
pub const DUPLICATE_OFFSET: f32 = 10.0;

/// The display-size pixels of the shown Image (shared with its texture), which the composite draws
/// from unless zoomed past them.
#[derive(Clone, Copy)]
pub struct DisplayPixels<'a> {
    /// Unique per decode: the renderer's cache key.
    pub key: u64,
    pub image: &'a egui::ColorImage,
}

/// The editor's state for the open Image.
struct Edited {
    doc: Document,
    /// The file the Image came from; `None` for a clipboard image not saved yet (§3.5).
    path: Option<PathBuf>,
    view_only: bool,
    full_decode: Option<PendingFullDecode>,
    /// Which open this is: a save finishing after another Image was opened leaves it alone.
    generation: u64,
    /// Image operations chosen before the full-resolution pixels arrived: done when they do.
    queued: Vec<ImageOp>,
    /// The last composite, its texture, and what it was rendered for.
    composite: Option<CompositeTexture>,
}

struct CompositeTexture {
    key: (u64, u64, View),
    texture: egui::TextureHandle,
    dest: egui::Rect,
    pixels: Composite,
}

/// A property-bar interaction in progress: changes to `kind` merge into one step once the first
/// one has made a step.
#[derive(Clone, Copy, PartialEq)]
struct PropertySession {
    kind: PropertyKind,
    stepped: bool,
}

pub struct Editor {
    tool: ToolId,
    behaviour: Box<dyn Tool>,
    /// Each tool's current style (its defaults, §4.3), saved as `tools.<tool>` in the config.
    styles: HashMap<ToolId, Style>,
    edited: Option<Edited>,
    /// Created on the first Annotation (§1.3, §7.2): creating it initialises Skia.
    renderer: Option<Renderer>,
    /// Is a pointer gesture on the canvas in progress?
    pointer_down: bool,
    property_session: Option<PropertySession>,
    /// The selection box as painted last frame, in points.
    selection_box: Option<egui::Rect>,
    /// The crop tool's region, in Image pixels (§4.1 C).
    crop: Option<Rect>,
    /// The cap on pixels each Document's History holds (§4.8).
    history_pixel_cap: usize,
    /// The rubber band as painted last frame, in points.
    rubber_band: Option<egui::Rect>,
    /// How many Images have been opened (each one's [`Edited::generation`]).
    opened: u64,
}

/// What a save (#34) needs from the editor when it starts.
pub struct SaveSource {
    /// What is drawn, for the worker to Flatten.
    pub drawing: Document,
    /// The file the Document came from (`None`: a clipboard image).
    pub path: Option<PathBuf>,
    /// Identifies the Document and its state, for [`Editor::saved`].
    pub stamp: SaveStamp,
}

/// Which Document, in which state, a save wrote.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SaveStamp {
    generation: u64,
    revision: u64,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

impl Editor {
    pub fn new() -> Self {
        Self {
            tool: ToolId::Select,
            behaviour: ToolId::Select.create().expect("the select tool"),
            styles: HashMap::new(),
            edited: None,
            renderer: None,
            pointer_down: false,
            property_session: None,
            selection_box: None,
            opened: 0,
            crop: None,
            history_pixel_cap: HISTORY_PIXEL_CAP,
            rubber_band: None,
        }
    }

    /// Change the History's pixel cap for the Documents opened from now on (tests lower it).
    pub fn set_history_pixel_cap(&mut self, bytes: usize) {
        self.history_pixel_cap = bytes;
    }

    /// An editor whose tools start with the styles saved in `config.toml` (`tools.<tool>`, §8.3).
    /// A table that isn't a valid style leaves that tool on its defaults.
    pub fn with_tool_styles(saved: &BTreeMap<String, toml::Table>) -> Self {
        let mut editor = Self::new();
        for tool in ToolId::STYLED {
            let Some(table) = saved.get(tool.config_key()) else {
                continue;
            };
            match toml::Value::Table(table.clone()).try_into::<Style>() {
                Ok(style) => {
                    editor.styles.insert(tool, style.sanitised());
                }
                Err(e) => eprintln!(
                    "zimged: tools.{} in config.toml is not a valid style, using the defaults: {e}",
                    tool.config_key()
                ),
            }
        }
        editor
    }

    /// A new Image is on screen: start a fresh Document (and History). `path`: the file it came
    /// from; `None` for a clipboard image, whose pixels come with [`Self::open_pixels`].
    pub fn open(&mut self, path: Option<&Path>, size: Size) {
        let view_only = u64::from(size.width) * u64::from(size.height) > MAX_EDIT_PIXELS;
        // Drop any half-done gesture: it belonged to the previous Document.
        self.behaviour = self.tool.create().expect("an available tool");
        self.pointer_down = false;
        self.property_session = None;
        self.opened += 1;
        self.crop = None;
        let mut doc = Document::new(size.width, size.height);
        doc.set_history_pixel_cap(self.history_pixel_cap);
        self.edited = Some(Edited {
            doc,
            path: path.map(Path::to_owned),
            view_only,
            full_decode: None,
            generation: self.opened,
            queued: Vec::new(),
            composite: None,
        });
        if view_only {
            self.set_tool(ToolId::Select);
        }
    }

    /// A clipboard image is on screen (§3.5): a fresh Document that already has its pixels.
    pub fn open_pixels(&mut self, pixels: Arc<Pixels>) {
        self.open(None, Size::new(pixels.width, pixels.height));
        if let Some(e) = &mut self.edited {
            e.doc.set_full_resolution(pixels);
        }
    }

    /// No Image any more (a failed open).
    pub fn close(&mut self) {
        self.edited = None;
        self.behaviour = self.tool.create().expect("an available tool");
        self.pointer_down = false;
        self.crop = None;
    }

    pub fn document(&self) -> Option<&Document> {
        self.edited.as_ref().map(|e| &e.doc)
    }

    pub fn tool(&self) -> ToolId {
        self.tool
    }

    pub fn is_view_only(&self) -> bool {
        self.edited.as_ref().is_some_and(|e| e.view_only)
    }

    /// Can the tools be used? Not without an Image, nor on a view-only one.
    pub fn can_edit(&self) -> bool {
        self.edited.as_ref().is_some_and(|e| !e.view_only)
    }

    /// The crop tool's region, in Image pixels.
    pub fn crop_region(&self) -> Option<Rect> {
        self.crop
    }

    /// The selection box (dashed, with 8 handles) as painted last frame, in points.
    pub fn selection_box(&self) -> Option<egui::Rect> {
        self.selection_box
    }

    /// The select tool's rubber band as painted last frame, in points.
    pub fn rubber_band(&self) -> Option<egui::Rect> {
        self.rubber_band
    }

    pub fn skia_initialised(&self) -> bool {
        self.renderer.is_some()
    }

    pub fn composite(&self) -> Option<&Composite> {
        let edited = self.edited.as_ref()?;
        edited.composite.as_ref().map(|c| &c.pixels)
    }

    /// The style the property bar shows when nothing is selected: the active tool's.
    pub fn tool_style(&self, tool: ToolId) -> Style {
        self.styles.get(&tool).cloned().unwrap_or_default()
    }

    /// Is the full-resolution decode still running?
    pub fn is_busy(&self) -> bool {
        self.edited
            .as_ref()
            .is_some_and(|e| e.full_decode.is_some())
    }

    // ---- Saving (#34) ----

    /// The Document to save, as it is now. `None` without an Image, or mid-gesture.
    pub fn save_source(&self) -> Option<SaveSource> {
        let e = self.edited.as_ref().filter(|e| !e.doc.in_gesture())?;
        Some(SaveSource {
            drawing: e.doc.drawing(),
            path: e.path.clone(),
            stamp: SaveStamp {
                generation: e.generation,
                revision: e.doc.revision(),
            },
        })
    }

    /// A save of `stamp` was written to `target`. The Document now belongs to `target`; it is
    /// marked saved (§4.8: the History stays) unless it changed while the save ran. `decoded`: the
    /// full-resolution pixels the save had to decode, kept so later saves and zooms don't decode
    /// the Flattened file.
    pub fn saved(&mut self, stamp: SaveStamp, target: &Path, decoded: Option<Arc<Pixels>>) {
        let Some(e) = self
            .edited
            .as_mut()
            .filter(|e| e.generation == stamp.generation)
        else {
            return;
        };
        e.path = Some(target.to_owned());
        if e.doc.revision() == stamp.revision {
            e.doc.mark_saved();
        }
        self.keep_decoded(stamp, decoded);
    }

    /// The full-resolution pixels a save or a copy (on the worker) had to decode for `stamp`'s
    /// Document: kept, so later saves and zooms don't decode again.
    pub fn keep_decoded(&mut self, stamp: SaveStamp, decoded: Option<Arc<Pixels>>) {
        let Some(e) = self
            .edited
            .as_mut()
            .filter(|e| e.generation == stamp.generation)
        else {
            return;
        };
        if let Some(pixels) = decoded
            && e.doc.full_resolution().is_none()
        {
            // A full decode still running could read the Flattened file: these pixels win, and a
            // crop waiting for that decode is done now.
            e.full_decode = None;
            e.doc.set_full_resolution(pixels);
            for op in std::mem::take(&mut e.queued) {
                op.apply(&mut e.doc);
            }
        }
    }

    /// Is the Document waiting for its full-resolution pixels before it is what the user sees (an
    /// Image operation queued until they arrive)? A save waits too.
    pub fn save_waits(&self) -> bool {
        self.edited.as_ref().is_some_and(|e| !e.queued.is_empty())
    }

    // ---- Commands ----

    pub fn select_tool(&mut self, tool: ToolId) {
        if !tool.is_available() || (tool != ToolId::Select && !self.can_edit()) {
            return;
        }
        if tool != ToolId::Select {
            self.start_full_decode();
            if let Some(e) = &mut self.edited
                && tool != self.tool
            {
                e.doc.clear_selection();
            }
        }
        self.set_tool(tool);
    }

    fn set_tool(&mut self, tool: ToolId) {
        if let Some(e) = &mut self.edited {
            let style = self.styles.get(&self.tool).cloned().unwrap_or_default();
            let mut cx = ToolContext {
                doc: &mut e.doc,
                style: &style,
                image_per_point: 1.0,
                crop: &mut self.crop,
            };
            self.behaviour.cancel(&mut cx);
        }
        // Leaving the crop tool drops its region (Esc cancels the crop, §4.1).
        self.crop = None;
        self.pointer_down = false;
        self.tool = tool;
        self.behaviour = tool.create().expect("an available tool");
    }

    pub fn undo(&mut self) {
        if let Some(e) = self.edited_idle() {
            e.doc.undo();
        }
    }

    pub fn redo(&mut self) {
        if let Some(e) = self.edited_idle() {
            e.doc.redo();
        }
    }

    /// Enter or Apply with the crop tool: crop to its region (§4.7), then back to the select tool.
    pub fn apply_crop(&mut self) {
        let Some(region) = self.crop.filter(|_| self.tool == ToolId::Crop) else {
            return;
        };
        self.image_operation(ImageOp::Crop(region));
        self.set_tool(ToolId::Select);
    }

    /// An Image operation is about to be chosen (the Resize dialog is up): start the
    /// full-resolution decode now (§4.7).
    pub fn prepare_image_operation(&mut self) {
        if self.can_edit() {
            self.start_full_decode();
        }
    }

    /// Carry out an Image operation (§4.7) as one History step. Before the full-resolution pixels
    /// have arrived (this starts their decode), it waits for them. Ignored mid-gesture and on a
    /// view-only Image. Any other operation ends the crop tool (its region would no longer fit).
    pub fn image_operation(&mut self, op: ImageOp) {
        if !self.can_edit() || self.edited_idle().is_none() {
            return;
        }
        if self.tool == ToolId::Crop && !matches!(op, ImageOp::Crop(_)) {
            self.set_tool(ToolId::Select);
        }
        self.start_full_decode();
        let e = self.edited.as_mut().expect("checked above");
        if !e.queued.is_empty() || e.doc.full_resolution().is_none() {
            e.queued.push(op);
        } else {
            op.apply(&mut e.doc);
        }
    }

    /// Cancel (Esc) with the crop tool: drop the region and go back to the select tool.
    pub fn cancel_crop(&mut self) {
        if self.tool == ToolId::Crop && !self.pointer_down {
            self.set_tool(ToolId::Select);
        }
    }

    /// Delete the selected Annotations. Returns false when nothing was selected.
    pub fn delete_selection(&mut self) -> bool {
        self.edited_idle().is_some_and(|e| e.doc.delete_selection())
    }

    /// Paste or Ctrl+drop an image (§4.6, §6.3): insert it as an Inset centred on `centre`
    /// (Image pixels) as one step, switch to the select tool and select it. Returns false when the
    /// Image can't be edited (view-only, mid-gesture).
    pub fn insert_inset(&mut self, pixels: Arc<Pixels>, centre: Option<Point>) -> bool {
        if !self.can_edit() || self.edited_idle().is_none() {
            return false;
        }
        self.set_tool(ToolId::Select);
        let style = self.tool_style(ToolId::Inset);
        let e = self.edited.as_mut().expect("checked above");
        let image = e.doc.bounds();
        let inset = Inset::placed(pixels, centre.unwrap_or(image.center()), image);
        e.doc.step(|doc| {
            let id = doc.add(inset, style);
            doc.select([id]);
        });
        true
    }

    /// Ctrl+D: duplicate the selection, offset by [`DUPLICATE_OFFSET`] Image pixels.
    pub fn duplicate_selection(&mut self) {
        if let Some(e) = self.edited_idle() {
            e.doc
                .duplicate_selection(Vector::new(DUPLICATE_OFFSET, DUPLICATE_OFFSET));
        }
    }

    pub fn restack(&mut self, restack: Restack) {
        if let Some(e) = self.edited_idle() {
            e.doc.restack(restack);
        }
    }

    /// Restyle the selection, and make the change the default of the tools that create the
    /// selected Annotations (§4.3). With nothing selected, it is the active tool's default.
    pub fn set_property(&mut self, property: Property, merge: bool) {
        let mut tools = vec![self.tool];
        if let Some(e) = self.edited.as_mut().filter(|e| !e.doc.in_gesture())
            && e.doc.set_property(property.clone(), merge)
        {
            tools = e
                .doc
                .selected()
                .map(|a| ToolId::for_shape(&a.shape))
                .collect();
        }
        let kind = property.kind();
        for tool in tools {
            if tool.properties().contains(&kind) {
                self.styles.entry(tool).or_default().set(property.clone());
            }
        }
    }

    /// The tools' styles as `tools.<tool>` tables for `config.toml` (§8.3): the ones changed (or
    /// read from the config) this run.
    pub fn tool_styles_config(&self) -> BTreeMap<String, toml::Table> {
        self.styles
            .iter()
            .filter_map(|(tool, style)| {
                let table = toml::Table::try_from(style).ok()?;
                Some((tool.config_key().to_owned(), table))
            })
            .collect()
    }

    /// The edited Image, when no pointer gesture is in progress.
    /// Is a tool gesture (a drag) under way?
    pub fn in_gesture(&self) -> bool {
        self.edited.as_ref().is_some_and(|e| e.doc.in_gesture())
    }

    fn edited_idle(&mut self) -> Option<&mut Edited> {
        self.edited.as_mut().filter(|e| !e.doc.in_gesture())
    }

    /// §4.7: the first tool use starts the full-resolution decode, in the background.
    fn start_full_decode(&mut self) {
        let Some(e) = &mut self.edited else {
            return;
        };
        if e.view_only || e.full_decode.is_some() || e.doc.full_resolution().is_some() {
            return;
        }
        if let Some(path) = &e.path {
            e.full_decode = Some(PendingFullDecode::spawn_full(path.clone()));
        }
    }

    /// Collect background results.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let Some(e) = &mut self.edited else {
            return;
        };
        let Some(pending) = &e.full_decode else {
            return;
        };
        pending.wake(ctx);
        if let Some(result) = pending.take() {
            e.full_decode = None;
            // A failed full decode leaves editing on the display pixels; Flatten (#34) reports it.
            if let Ok(pixels) = result {
                e.doc.set_full_resolution(Arc::new(pixels));
            }
            for op in std::mem::take(&mut e.queued) {
                op.apply(&mut e.doc);
            }
        }
    }

    // ---- Property bar ----

    /// What the property bar edits: the selection's style and properties, or the active tool's.
    pub fn property_target(&self) -> (Style, Vec<PropertyKind>, bool) {
        if let Some(doc) = self.document() {
            let mut selected = doc.selected();
            if let Some(first) = selected.next() {
                let mut kinds = first.shape.properties().to_vec();
                for other in selected {
                    let theirs = other.shape.properties();
                    kinds.retain(|k| theirs.contains(k));
                }
                return (first.style.clone(), kinds, true);
            }
        }
        (
            self.tool_style(self.tool),
            self.tool.properties().to_vec(),
            false,
        )
    }

    /// Turn a property-bar change into a command, merging the changes of one widget interaction.
    pub fn property_change(&mut self, property: Property) -> crate::Command {
        let kind = property.kind();
        let merge = self.property_session
            == Some(PropertySession {
                kind,
                stepped: true,
            });
        self.property_session = Some(PropertySession {
            kind,
            stepped: true,
        });
        crate::Command::SetProperty { property, merge }
    }

    /// After the property bar: `active` is the property whose widget is still being interacted
    /// with (a colour picker open, a value being dragged), if any.
    pub fn property_interaction(&mut self, active: Option<PropertyKind>) {
        self.property_session = match (active, self.property_session) {
            (None, _) => None,
            (Some(kind), Some(s)) if s.kind == kind => Some(s),
            (Some(kind), _) => Some(PropertySession {
                kind,
                stepped: false,
            }),
        };
    }

    // ---- Canvas ----

    /// Handle the pointer on the canvas and say what to paint (§7.2).
    pub fn canvas(
        &mut self,
        ctx: &egui::Context,
        layout: &Layout,
        response: &egui::Response,
        display: Option<DisplayPixels<'_>>,
        pointer_is_ours: bool,
    ) -> EditorPaint {
        let revision = self.document().map(|d| d.revision());
        if self.can_edit() && (pointer_is_ours || self.pointer_down) {
            self.pointer(ctx, layout, response);
        }
        if self.document().map(|d| d.revision()) != revision {
            ctx.request_repaint();
        }
        let composite = display.and_then(|d| self.update_composite(ctx, layout, d));
        let selection = self
            .document()
            .and_then(|d| d.selection_bounds())
            .map(|r| layout.rect_to_screen(r));
        self.selection_box = selection;
        let rubber_band = self
            .behaviour
            .rubber_band()
            .map(|r| layout.rect_to_screen(r));
        self.rubber_band = rubber_band;
        EditorPaint {
            composite,
            selection,
            crop: self.crop.map(|r| layout.rect_to_screen(r)),
            rubber_band,
        }
    }

    fn pointer(&mut self, ctx: &egui::Context, layout: &Layout, response: &egui::Response) {
        let modifiers = ctx.input(|i| i.modifiers);
        let at = |pos: egui::Pos2| Pointer {
            pos: layout.image_point(pos),
            modifiers,
        };
        let press_origin = ctx.input(|i| i.pointer.press_origin());
        let current = response.interact_pointer_pos();

        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(origin) = press_origin
        {
            self.pointer_down = true;
            self.with_tool(layout, |tool, cx| tool.press(cx, at(origin)));
        }
        if self.pointer_down
            && response.dragged_by(egui::PointerButton::Primary)
            && let Some(pos) = current
        {
            self.with_tool(layout, |tool, cx| tool.drag(cx, at(pos)));
        }
        if self.pointer_down && response.drag_stopped_by(egui::PointerButton::Primary) {
            self.pointer_down = false;
            let pos = current
                .or_else(|| ctx.input(|i| i.pointer.latest_pos()))
                .unwrap_or_default();
            self.with_tool(layout, |tool, cx| tool.release(cx, at(pos)));
        }
        if response.clicked()
            && let Some(pos) = current.or(press_origin)
        {
            self.with_tool(layout, |tool, cx| {
                tool.press(cx, at(pos));
                tool.release(cx, at(pos));
            });
        }
    }

    fn with_tool(&mut self, layout: &Layout, f: impl FnOnce(&mut dyn Tool, &mut ToolContext<'_>)) {
        if self.tool != ToolId::Select {
            self.start_full_decode();
        }
        let Some(e) = &mut self.edited else {
            return;
        };
        let style = self.styles.get(&self.tool).cloned().unwrap_or_default();
        let mut cx = ToolContext {
            doc: &mut e.doc,
            style: &style,
            image_per_point: layout.image_per_point(),
            crop: &mut self.crop,
        };
        f(self.behaviour.as_mut(), &mut cx);
    }

    /// Re-render the composite if the Document or the view changed. `None` while there are no
    /// Annotations and the Image is the decoded file: the plain Image texture is drawn then (§7.2).
    /// After an Image operation the composite always draws, from the Document's own pixels.
    fn update_composite(
        &mut self,
        ctx: &egui::Context,
        layout: &Layout,
        display: DisplayPixels<'_>,
    ) -> Option<(egui::TextureId, egui::Rect)> {
        let e = self.edited.as_mut()?;
        if e.doc.annotations().is_empty() && e.doc.is_original_image() {
            e.composite = None;
            return None;
        }
        let (dest, region, px) = layout.visible()?;
        let view = View {
            region,
            width: px.width,
            height: px.height,
        };
        // Zoomed in past the display-size decode, draw from the full-resolution pixels (§7.2).
        let display_density = display.image.width() as f32 / e.doc.width() as f32;
        let wanted_density = px.width as f32 / region.width().max(1.0);
        let source = match e.doc.full_resolution() {
            Some(full) if wanted_density > display_density || !e.doc.is_original_image() => {
                Source {
                    key: source_key(display.key, e.doc.image_generation()),
                    width: full.width,
                    height: full.height,
                    pixels: match &full.data {
                        PixelData::Rgb(rgb) => SourcePixels::Rgb(rgb),
                        PixelData::Rgba(rgba) => SourcePixels::Rgba(rgba),
                    },
                }
            }
            _ => Source {
                key: display.key * 2,
                width: display.image.width() as u32,
                height: display.image.height() as u32,
                pixels: SourcePixels::RgbaPremultiplied(display.image.as_raw()),
            },
        };
        let key = (e.doc.revision(), source.key, view);
        if e.composite.as_ref().is_none_or(|c| c.key != key) {
            let renderer = self.renderer.get_or_insert_with(Renderer::new);
            // Re-rendering only the dirty region is a later optimisation (§7.2).
            let pixels = renderer.composite(&e.doc, &source, view);
            let image = egui::ColorImage::from_rgba_premultiplied(
                [pixels.width as usize, pixels.height as usize],
                &pixels.rgba_premultiplied,
            );
            match &mut e.composite {
                Some(c) => {
                    c.texture.set(image, egui::TextureOptions::LINEAR);
                    c.key = key;
                    c.dest = dest;
                    c.pixels = pixels;
                }
                None => {
                    let texture =
                        ctx.load_texture("composite", image, egui::TextureOptions::LINEAR);
                    e.composite = Some(CompositeTexture {
                        key,
                        texture,
                        dest,
                        pixels,
                    });
                }
            }
        }
        let c = e.composite.as_ref()?;
        Some((c.texture.id(), c.dest))
    }
}

/// An Image operation (§4.7), as the editor queues it until the full-resolution pixels arrive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImageOp {
    /// Crop to this region (Image pixels).
    Crop(Rect),
    Rotate {
        clockwise: bool,
    },
    Flip {
        horizontal: bool,
    },
    /// Resize to this many pixels (Mitchell cubic up, mipmap + cubic down).
    Resize {
        width: u32,
        height: u32,
    },
}

impl ImageOp {
    fn apply(self, doc: &mut Document) -> bool {
        match self {
            ImageOp::Crop(region) => doc.crop(region),
            ImageOp::Rotate { clockwise } => doc.rotate(clockwise),
            ImageOp::Flip { horizontal } => doc.flip(horizontal),
            ImageOp::Resize { width, height } => doc.resize(width, height, zimged_render::resample),
        }
    }
}

/// The renderer's cache key for the full-resolution pixels: unique per decode and per Image
/// operation result (display-size decodes use even keys).
fn source_key(display_key: u64, generation: u64) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (display_key, generation).hash(&mut h);
    h.finish() | 1
}
