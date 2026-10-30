//! The editor (§4): the open Document, the active tool and the tools' styles, the full-resolution
//! decode, and the Skia composite of the canvas.
//!
//! The app owns one [`Editor`]. Per Image state ([`Edited`]) is replaced on every open, which is
//! what clears the History when moving to another Image (§4.8). The active tool and each tool's
//! style outlive it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use zimged_codec::Size;
use zimged_core::{Document, PixelData, Property, PropertyKind, Shape, Style};
use zimged_render::{Composite, Renderer, Source, SourcePixels, View};

use crate::decode::PendingFullDecode;
use crate::tools::{Pointer, Tool, ToolContext, ToolId};
use crate::ui::canvas::{EditorPaint, Layout};

/// Editing needs the full-resolution decode, allowed up to 100 MP (§4.7).
pub const MAX_EDIT_PIXELS: u64 = 100_000_000;

pub const VIEW_ONLY_MESSAGE: &str = "View only: image too large to edit";

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
    path: PathBuf,
    view_only: bool,
    full_decode: Option<PendingFullDecode>,
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
    /// Each tool's current style (its defaults, §4.3). Persisting them is #27/#24.
    styles: HashMap<ToolId, Style>,
    edited: Option<Edited>,
    /// Created on the first Annotation (§1.3, §7.2): creating it initialises Skia.
    renderer: Option<Renderer>,
    /// Is a pointer gesture on the canvas in progress?
    pointer_down: bool,
    property_session: Option<PropertySession>,
    /// The selection box as painted last frame, in points.
    selection_box: Option<egui::Rect>,
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
        }
    }

    /// A new Image is on screen: start a fresh Document (and History).
    pub fn open(&mut self, path: &Path, size: Size) {
        let view_only = u64::from(size.width) * u64::from(size.height) > MAX_EDIT_PIXELS;
        // Drop any half-done gesture: it belonged to the previous Document.
        self.behaviour = self.tool.create().expect("an available tool");
        self.pointer_down = false;
        self.property_session = None;
        self.edited = Some(Edited {
            doc: Document::new(size.width, size.height),
            path: path.to_owned(),
            view_only,
            full_decode: None,
            composite: None,
        });
        if view_only {
            self.set_tool(ToolId::Select);
        }
    }

    /// No Image any more (a failed open).
    pub fn close(&mut self) {
        self.edited = None;
        self.behaviour = self.tool.create().expect("an available tool");
        self.pointer_down = false;
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

    /// The selection box (dashed, with 8 handles) as painted last frame, in points.
    pub fn selection_box(&self) -> Option<egui::Rect> {
        self.selection_box
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
            };
            self.behaviour.cancel(&mut cx);
        }
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

    /// Delete the selected Annotations. Returns false when nothing was selected.
    pub fn delete_selection(&mut self) -> bool {
        self.edited_idle().is_some_and(|e| e.doc.delete_selection())
    }

    pub fn set_property(&mut self, property: Property, merge: bool) {
        let tool = self.tool;
        if let Some(e) = self.edited.as_mut().filter(|e| !e.doc.in_gesture())
            && e.doc.set_property(property.clone(), merge)
        {
            return;
        }
        // Nothing selected: the active tool's default for its next Annotation.
        self.styles.entry(tool).or_default().set(property);
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
        e.full_decode = Some(PendingFullDecode::spawn_full(e.path.clone()));
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
        EditorPaint {
            composite,
            selection,
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
        };
        f(self.behaviour.as_mut(), &mut cx);
    }

    /// Re-render the composite if the Document or the view changed. `None` while there are no
    /// Annotations: the plain Image texture is drawn then (§7.2).
    fn update_composite(
        &mut self,
        ctx: &egui::Context,
        layout: &Layout,
        display: DisplayPixels<'_>,
    ) -> Option<(egui::TextureId, egui::Rect)> {
        let e = self.edited.as_mut()?;
        if e.doc.annotations().is_empty() {
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
            Some(full) if wanted_density > display_density => Source {
                key: display.key * 2 + 1,
                width: full.width,
                height: full.height,
                pixels: match &full.data {
                    PixelData::Rgb(rgb) => SourcePixels::Rgb(rgb),
                    PixelData::Rgba(rgba) => SourcePixels::Rgba(rgba),
                },
            },
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
