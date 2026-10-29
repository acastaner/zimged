//! A display-size decode running on its own thread, polled by the UI.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use zimged_codec::{DecodeError, DisplayImage, Size};

pub type DecodeResult = Result<DisplayImage, DecodeError>;

#[derive(Default)]
struct Slot {
    result: Option<DecodeResult>,
    /// Woken when the result arrives, once the UI has handed over its context.
    waker: Option<egui::Context>,
}

#[derive(Default)]
struct Shared {
    slot: Mutex<Slot>,
    done: Condvar,
}

/// A decode in flight. The UI polls it with [`PendingDecode::take`] each frame.
pub struct PendingDecode {
    path: PathBuf,
    shared: Arc<Shared>,
}

impl PendingDecode {
    /// Start decoding `path` on a new thread. `display_box` is called only after the full decode, so
    /// the startup path can start decoding before it knows the window size (§1.3).
    pub fn spawn(path: PathBuf, display_box: impl FnOnce() -> Size + Send + 'static) -> Self {
        let shared = Arc::new(Shared::default());
        let thread_shared = Arc::clone(&shared);
        let thread_path = path.clone();
        std::thread::Builder::new()
            .name("decode".into())
            .spawn(move || {
                let result =
                    zimged_codec::decode(&thread_path).map(|d| d.into_display(display_box()));
                static FIRST: AtomicBool = AtomicBool::new(false);
                crate::bench::mark_once("decoded", &FIRST);
                let mut slot = thread_shared.slot.lock().unwrap();
                slot.result = Some(result);
                if let Some(ctx) = &slot.waker {
                    ctx.request_repaint();
                }
                thread_shared.done.notify_all();
            })
            .expect("spawn decode thread");
        Self { path, shared }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Ask for a repaint of `ctx` when the result arrives (immediately, if it already has).
    pub fn wake(&self, ctx: &egui::Context) {
        let mut slot = self.shared.slot.lock().unwrap();
        if slot.result.is_some() {
            ctx.request_repaint();
        } else if slot.waker.is_none() {
            slot.waker = Some(ctx.clone());
        }
    }

    /// Block until the decode has finished or `timeout` has passed. Returns whether it finished.
    pub fn wait(&self, timeout: Duration) -> bool {
        let slot = self.shared.slot.lock().unwrap();
        let (slot, _) = self
            .shared
            .done
            .wait_timeout_while(slot, timeout, |slot| slot.result.is_none())
            .unwrap();
        slot.result.is_some()
    }

    /// The result, once the decode has finished.
    pub fn take(&self) -> Option<DecodeResult> {
        self.shared.slot.lock().unwrap().result.take()
    }
}
