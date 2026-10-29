// PROTOTYPE, throwaway. Rust/egui half of "Cold-start spike for the top stacks" (acastaner/zimged#9).
// Decode (image crate -> zune-jpeg, then fast_image_resize to display size) starts on a thread before the
// window exists; eframe renders with glow (OpenGL) or wgpu, picked by SPIKE_RENDERER.
//
// Env: SPIKE_T0_NS, SPIKE_EXIT=1, SPIKE_NAV=ms,n (same meaning as the Qt spike), SPIKE_RENDERER=glow|wgpu
use eframe::egui;
use fast_image_resize as fr;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

static mut T0: u128 = 0;
fn now_ns() -> u128 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() }
fn ms() -> f64 { (now_ns() - unsafe { T0 }) as f64 / 1e6 }
fn mark(what: &str) { println!("{what} {:.2}", ms()); }

const BOX: (u32, u32) = (1600, 1000); // = window size

fn decode_for_display(p: &Path) -> egui::ColorImage {
    let t = Instant::now();
    let img = image::ImageReader::open(p).unwrap().with_guessed_format().unwrap().decode().unwrap();
    let td = t.elapsed();
    let img = img.into_rgb8();
    let (w, h) = img.dimensions();
    let s = (BOX.0 as f64 / w as f64).min(BOX.1 as f64 / h as f64).min(1.0);
    let (nw, nh) = (((w as f64) * s).round() as u32, ((h as f64) * s).round() as u32);
    let rgb = if s < 1.0 {
        let src = fr::images::Image::from_vec_u8(w, h, img.into_raw(), fr::PixelType::U8x3).unwrap();
        let mut dst = fr::images::Image::new(nw, nh, fr::PixelType::U8x3);
        fr::Resizer::new()
            .resize(&src, &mut dst, &fr::ResizeOptions::new().resize_alg(fr::ResizeAlg::Convolution(fr::FilterType::Bilinear)))
            .unwrap();
        dst.into_vec()
    } else {
        img.into_raw()
    };
    let out = egui::ColorImage::from_rgb([nw as usize, nh as usize], &rgb);
    if std::env::var("SPIKE_DECODE_ONLY").is_ok() { eprintln!("decode {:.1} ms, total {:.1} ms", td.as_secs_f64() * 1e3, t.elapsed().as_secs_f64() * 1e3); }
    out
}

type Slot = Arc<Mutex<Option<egui::ColorImage>>>;
fn spawn_decode(p: PathBuf, ctx: Option<egui::Context>) -> Slot {
    let slot: Slot = Arc::new(Mutex::new(None));
    let s2 = slot.clone();
    std::thread::spawn(move || {
        let img = decode_for_display(&p);
        *s2.lock().unwrap() = Some(img);
        if let Some(c) = ctx { c.request_repaint(); }
    });
    slot
}

struct App {
    first_path: PathBuf,
    siblings: Vec<PathBuf>,
    idx: usize,
    cache: HashMap<usize, Slot>,
    tex: Option<egui::TextureHandle>,
    shown_idx: Option<usize>,
    first_painted: bool,
    exit_after_first: bool,
    nav: Option<(u64, u32)>,
    nav_start: Option<Instant>,
    last_switch_done: Option<Instant>,
}

impl App {
    fn scan(&mut self) {
        let dir = self.first_path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let mut v: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path())
            .filter(|p| matches!(p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
                Some("jpg" | "jpeg" | "png" | "webp" | "gif"))).collect();
        v.sort_by_key(|p| p.file_name().unwrap().to_string_lossy().to_lowercase()); // spike: plain sort
        self.idx = v.iter().position(|p| p.file_name() == self.first_path.file_name()).unwrap_or(0);
        self.siblings = v;
        println!("siblings_scanned {:.2} {}", ms(), self.siblings.len());
    }
    fn prefetch_around(&mut self, ctx: &egui::Context) {
        let i = self.idx as isize;
        for j in [i + 1, i - 1, i + 2] {
            if j >= 0 && (j as usize) < self.siblings.len() && !self.cache.contains_key(&(j as usize)) {
                self.cache.insert(j as usize, spawn_decode(self.siblings[j as usize].clone(), Some(ctx.clone())));
            }
        }
        self.cache.retain(|k, _| (*k as isize - i).abs() <= 2);
    }
    fn go(&mut self, d: isize, ctx: &egui::Context) {
        let n = self.idx as isize + d;
        if n < 0 || n as usize >= self.siblings.len() { return; }
        self.idx = n as usize;
        self.nav_start = Some(Instant::now());
        if !self.cache.contains_key(&self.idx) {
            self.cache.insert(self.idx, spawn_decode(self.siblings[self.idx].clone(), Some(ctx.clone())));
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &ui.ctx().clone();
        // pick up a finished decode for the current index
        if self.shown_idx != Some(self.idx) {
            if let Some(slot) = self.cache.get(&self.idx) {
                if let Some(img) = slot.lock().unwrap().clone() {
                    match &mut self.tex {
                        Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                        None => self.tex = Some(ctx.load_texture("img", img, egui::TextureOptions::LINEAR)),
                    }
                    self.shown_idx = Some(self.idx);
                }
            }
        }
        if self.tex.is_none() { ctx.request_repaint_after(std::time::Duration::from_millis(2)); }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) { self.go(1, ctx); }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) { self.go(-1, ctx); }
        let scroll: f32 = ctx.input(|i| i.events.iter().filter_map(|e| if let egui::Event::MouseWheel { delta, .. } = e { Some(delta.y) } else { None }).sum());
        if scroll != 0.0 { self.go(if scroll < 0.0 { 1 } else { -1 }, ctx); }

        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(17, 18, 20))).show(ui, |ui| {
            if let Some(t) = &self.tex {
                let r = ui.max_rect();
                let sz = t.size_vec2();
                let s = (r.width() / sz.x).min(r.height() / sz.y);
                let rect = egui::Rect::from_center_size(r.center(), sz * s);
                ui.painter().image(t.id(), rect, egui::Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(1., 1.)), egui::Color32::WHITE);
            }
        });

        if self.tex.is_some() && !self.first_painted {
            self.first_painted = true;
            mark("first_image_painted");
            if self.exit_after_first { ctx.send_viewport_cmd(egui::ViewportCommand::Close); return; }
            self.scan();
            let cur = self.idx;
            self.cache.insert(cur, Arc::new(Mutex::new(None))); // already on screen
            self.shown_idx = Some(cur);
            self.prefetch_around(ctx);
            self.last_switch_done = Some(Instant::now() + std::time::Duration::from_millis(300));
        } else if let (Some(st), true) = (self.nav_start, self.shown_idx == Some(self.idx)) {
            println!("switch_ms {:.2} {}", st.elapsed().as_secs_f64() * 1e3, self.idx);
            self.nav_start = None;
            self.last_switch_done = Some(Instant::now());
            self.prefetch_around(ctx);
        }
        if let Some((interval, left)) = self.nav {
            if self.first_painted && self.nav_start.is_none() {
                let due = self.last_switch_done.unwrap() + std::time::Duration::from_millis(interval);
                if Instant::now() >= due {
                    if left == 0 || self.idx + 1 >= self.siblings.len() { ctx.send_viewport_cmd(egui::ViewportCommand::Close); return; }
                    self.nav = Some((interval, left - 1));
                    self.go(1, ctx);
                }
                ctx.request_repaint_after(due.saturating_duration_since(Instant::now()));
            }
            if self.nav_start.is_some() { ctx.request_repaint(); }
        }
    }
}

fn main() -> eframe::Result {
    unsafe { T0 = std::env::var("SPIKE_T0_NS").ok().and_then(|v| v.parse().ok()).unwrap_or_else(now_ns); }
    mark("main");
    if std::env::var("SPIKE_DECODE_ONLY").is_ok() {
        if let Ok(ms) = std::env::var("SPIKE_SPIN") { let t = Instant::now(); let mut x = 0u64; while t.elapsed().as_millis() < ms.parse::<u128>().unwrap() { x = x.wrapping_mul(31).wrapping_add(1); } std::hint::black_box(x); }
        for a in std::env::args().skip(1) { decode_for_display(Path::new(&a)); }
        return Ok(());
    }
    let path = PathBuf::from(std::env::args().nth(1).expect("usage: zimged-spike-rust <image>"));
    let first = {
        let p = path.clone();
        let slot: Slot = Arc::new(Mutex::new(None));
        let s2 = slot.clone();
        std::thread::spawn(move || { let i = decode_for_display(&p); mark("decoded"); *s2.lock().unwrap() = Some(i); });
        slot
    };
    let renderer = match std::env::var("SPIKE_RENDERER").as_deref() { Ok("wgpu") => eframe::Renderer::Wgpu, _ => eframe::Renderer::Glow };
    let nav = std::env::var("SPIKE_NAV").ok().map(|v| { let mut it = v.split(','); (it.next().unwrap().parse().unwrap(), it.next().unwrap().parse().unwrap()) });
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1600.0, 1000.0])
            .with_title(format!("{} - zimged spike (Rust/egui)", path.file_name().unwrap().to_string_lossy())),
        renderer,
        ..Default::default()
    };
    let exit_after_first = std::env::var("SPIKE_EXIT").is_ok();
    eframe::run_native("zimged-spike", opts, Box::new(move |cc| {
        mark("renderer_ready");
        let mut cache = HashMap::new();
        cache.insert(0, first);
        Ok(Box::new(App { first_path: path, siblings: vec![], idx: 0, cache, tex: None, shown_idx: None, first_painted: false,
            exit_after_first, nav, nav_start: None, last_switch_done: None }))
    }))
}
