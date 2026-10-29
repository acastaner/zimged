//! Native file dialogs (§8.2): `rfd`, using the XDG desktop portal on Linux.
//!
//! The app never blocks on a dialog: it asks for one and gets the answer through a callback, called
//! from whatever thread the dialog finished on. The app polls for it each frame.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;

/// Receives what the user chose: `None` when the dialog was cancelled.
pub type DialogReply = Box<dyn FnOnce(Option<PathBuf>) + Send>;

/// The native file dialogs.
pub trait FileDialogs: Send + Sync {
    /// Show the open dialog for an image file (Ctrl+O, §3.5). `reply` is called exactly once.
    fn pick_image(&self, reply: DialogReply);
}

/// The extensions of the formats zimged reads (§5.1).
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "jpe", "jfif", "png", "webp", "gif", "bmp", "ico", "tif", "tiff", "avif",
];

/// `rfd`'s native dialogs.
#[derive(Default)]
pub struct OsFileDialogs;

impl FileDialogs for OsFileDialogs {
    fn pick_image(&self, reply: DialogReply) {
        // Build the dialog on the calling (UI) thread, as rfd wants on macOS, and wait for it on a
        // thread of its own so the window keeps painting while it is up.
        let dialog = rfd::AsyncFileDialog::new()
            .set_title("Open")
            .add_filter("Images", IMAGE_EXTENSIONS)
            .add_filter("All files", &["*"])
            .pick_file();
        let spawned = std::thread::Builder::new()
            .name("open-dialog".into())
            .spawn(move || {
                let chosen = pollster::block_on(dialog);
                reply(chosen.map(|file| file.path().to_owned()));
            });
        if let Err(e) = spawned {
            eprintln!("zimged: can't show the open dialog: {e}");
        }
    }
}

/// Scripted dialogs for tests: each dialog shown takes the next queued answer (cancel when none).
#[derive(Default)]
pub struct MemoryFileDialogs {
    state: Mutex<MemoryDialogsState>,
}

#[derive(Default)]
struct MemoryDialogsState {
    answers: VecDeque<Option<PathBuf>>,
    opened: usize,
    /// When set, dialogs stay up: their replies are kept, never called.
    hold: bool,
    held: Vec<DialogReply>,
}

impl MemoryFileDialogs {
    /// What the next dialog shown answers: a chosen path, or `None` for Cancel.
    pub fn answer_next(&self, answer: Option<PathBuf>) {
        self.state.lock().unwrap().answers.push_back(answer);
    }

    /// Keep every dialog from now on up, as if the user never answered it.
    pub fn hold_answers(&self) {
        self.state.lock().unwrap().hold = true;
    }

    /// How many dialogs have been shown.
    pub fn opened(&self) -> usize {
        self.state.lock().unwrap().opened
    }
}

impl FileDialogs for MemoryFileDialogs {
    fn pick_image(&self, reply: DialogReply) {
        let mut state = self.state.lock().unwrap();
        state.opened += 1;
        if state.hold {
            state.held.push(reply);
            return;
        }
        let answer = state.answers.pop_front().flatten();
        drop(state);
        reply(answer);
    }
}
