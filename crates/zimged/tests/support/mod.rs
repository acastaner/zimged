//! The headless App harness (spec §9): the real zimged app under `egui_kittest`, with the platform
//! adapters replaced by in-memory fakes and fixtures copied into a temporary directory.
//!
//! Each integration test file does `mod support;` and drives the app through [`TestApp`].
#![allow(dead_code)] // Not every test file uses every helper.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use zimged::decode::PendingDecode;
use zimged::{Command, Launch, ZimgedApp};
use zimged_platform::Platform;

/// Logical window size of the headless App, in points (1 point = 1 pixel).
pub const WINDOW: egui::Vec2 = egui::vec2(1000.0, 700.0);

pub struct TestApp {
    pub harness: Harness<'static, ZimgedApp>,
    /// Holds the copied fixtures; dropped (deleted) with the TestApp.
    pub dir: tempfile::TempDir,
}

/// Path of a fixture in the repository's `tests/fixtures/`.
pub fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

/// Copy fixtures into `dir`; returns the path of the first copy.
fn copy_fixtures(dir: &Path, names: &[&str]) -> PathBuf {
    let copies: Vec<PathBuf> = names
        .iter()
        .map(|name| {
            let to = dir.join(name);
            std::fs::copy(fixture_path(name), &to).expect("copy fixture");
            to
        })
        .collect();
    copies[0].clone()
}

impl TestApp {
    /// A headless App with no Image open, in-memory platform fakes and an empty temporary directory.
    pub fn new() -> Self {
        Self::with_platform(Platform::in_memory())
    }

    pub fn with_platform(platform: Platform) -> Self {
        Self::with_launch(|launch| launch.platform = platform)
    }

    /// A headless App with no Image open, its [`Launch`] (in-memory platform) adjusted by `adjust`.
    pub fn with_launch(adjust: impl FnOnce(&mut Launch)) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut launch = Launch::new(Platform::in_memory());
        adjust(&mut launch);
        let harness = Harness::builder()
            .with_size(WINDOW)
            .build_eframe(move |cc| ZimgedApp::new(cc, launch));
        Self { harness, dir }
    }

    /// Start a headless App the way `zimged <path>` starts: the first-image decode is already
    /// running when the app is created (§1.3). Copies the fixtures first; the first one is launched.
    /// Doesn't wait: call [`Self::settle`] (or step the harness) yourself.
    pub fn launch(names: &[&str]) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = copy_fixtures(dir.path(), names);
        let mut launch = Launch::new(Platform::in_memory());
        let display_box = zimged::app::startup_display_box(&launch.config);
        launch.first_image = Some(PendingDecode::spawn(path, move || display_box));
        let harness = Harness::builder()
            .with_size(WINDOW)
            .build_eframe(move |cc| ZimgedApp::new(cc, launch));
        Self { harness, dir }
    }

    /// Like [`Self::launch`], but the first-image decode doesn't finish until the returned sender
    /// sends (or is dropped), so a test can step frame by frame from the first image on. (Building
    /// the harness runs frames until the app is idle, which would otherwise race the decode.)
    pub fn launch_held(names: &[&str]) -> (Self, std::sync::mpsc::Sender<()>) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = copy_fixtures(dir.path(), names);
        let mut launch = Launch::new(Platform::in_memory());
        let display_box = zimged::app::startup_display_box(&launch.config);
        let (release, held) = std::sync::mpsc::channel::<()>();
        launch.first_image = Some(PendingDecode::spawn(path, move || {
            let _ = held.recv();
            display_box
        }));
        let harness = Harness::builder()
            .with_size(WINDOW)
            .build_eframe(move |cc| ZimgedApp::new(cc, launch));
        (Self { harness, dir }, release)
    }

    /// Copy the named fixtures into the temporary directory and return the path of the first copy.
    pub fn add_fixtures(&self, names: &[&str]) -> PathBuf {
        copy_fixtures(self.dir.path(), names)
    }

    /// Copy one fixture into the temporary directory under another name (to build a folder of
    /// Siblings); returns the copy's path.
    pub fn add_fixture_as(&self, fixture: &str, name: &str) -> PathBuf {
        let to = self.dir.path().join(name);
        std::fs::copy(fixture_path(fixture), &to).expect("copy fixture");
        to
    }

    /// A headless App whose temporary directory holds a copy of `fixture` under each of `names`,
    /// with `open` opened and the Sibling scan finished.
    pub fn open_in_folder(fixture: &str, names: &[&str], open: &str) -> Self {
        let mut app = Self::new();
        for name in names {
            app.add_fixture_as(fixture, name);
        }
        app.open(app.dir.path().join(open));
        app
    }

    /// Press a key (no modifiers) and wait until idle.
    pub fn press(&mut self, key: egui::Key) {
        self.harness.key_press(key);
        self.settle();
    }

    /// Turn the mouse wheel by `lines` notches over the canvas (negative: down) and wait until idle.
    pub fn wheel(&mut self, lines: f32) {
        self.harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, lines),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::NONE,
        });
        self.settle();
    }

    /// Start a headless App, copy the fixture into its temporary directory, open it (the same
    /// command Ctrl+O and a drop end in), and wait until it is on screen.
    pub fn open_fixture(name: &str) -> Self {
        let mut app = Self::new();
        let path = app.add_fixtures(&[name]);
        app.open(path);
        app
    }

    /// Open a path (the same command a launch argument, Ctrl+O or a drop end in) and wait until idle.
    pub fn open(&mut self, path: impl Into<PathBuf>) {
        self.harness.state_mut().execute(Command::Open(path.into()));
        self.settle();
    }

    /// Run frames until the app has no background work pending (decodes, …) and egui stops asking for
    /// repaints. Panics after 10 s.
    pub fn settle(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            // Background work (prefetch decodes) keeps asking for repaints while busy, so more
            // than `max_steps` frames are fine here; the final `run` checks the app settles.
            let _ = self.harness.try_run();
            if !self.harness.state().is_busy() {
                self.harness.run();
                return;
            }
            assert!(Instant::now() < deadline, "app still busy after 10 s");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// The text of a status-bar field, found by its accessibility label (e.g. "File name").
    ///
    /// Status-bar fields are `Role::Label` nodes whose accessible name is the field's label and
    /// whose value is the text shown. (kittest's `get_by_label` reads a `Role::Label` node's value
    /// as its label, hence the predicate.)
    pub fn status(&self, label: &str) -> String {
        self.status_field(label)
            .unwrap_or_else(|| panic!("no status-bar field labelled {label:?}"))
    }

    /// Like [`Self::status`], but `None` when the field isn't shown.
    pub fn status_field(&self, label: &str) -> Option<String> {
        let wanted = label.to_owned();
        self.harness
            .query_by(move |node| {
                node.role() == Role::Label && node.label().as_deref() == Some(wanted.as_str())
            })
            .map(|node| node.value().unwrap_or_default())
    }
}
