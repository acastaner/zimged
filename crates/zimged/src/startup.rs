//! The startup path (§1.3). The cold-start budget (§1.2) depends on this ordering:
//!
//! 1. `main()` starts the first-image decode and the config read, each on its own thread.
//! 2. Only when *Allow multiple instances* is off: offer the path to a running instance over the
//!    single-instance socket (§8.4) and exit if it answers; else this process becomes the listener.
//! 3. eframe creates the window and the GL context (glow) while the decode runs.
//! 4. The first frame with the image is painted as soon as both are ready.
//! 5. Everything else waits for [`crate::ZimgedApp`]'s after-first-paint hook.
//!
//! Nothing else may run here. Check every addition against the budget with `tools/bench`.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use zimged_codec::Size;
use zimged_platform::{Config, HandOff, Platform};

use crate::app::{Launch, ZimgedApp, startup_display_box};
use crate::decode::PendingDecode;

/// How long the first frame waits for the first-image decode once the window is ready. A 12 MP
/// JPEG decodes in well under this on the reference machine.
const FIRST_FRAME_WAIT: Duration = Duration::from_millis(400);

/// What startup does after [`single_instance`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupStep {
    /// Go on: create the window.
    OpenWindow,
    /// A running instance took the path: exit without a window.
    Exit,
}

/// §1.3 step 2 / §8.4. With *Allow multiple instances* on (the default) the socket isn't touched.
/// Off: hand `path` to a running instance and exit if it answers; if nothing answers, this
/// process becomes the listener (the running app picks up arrivals each frame).
pub fn single_instance(platform: &Platform, config: &Config, path: Option<&Path>) -> StartupStep {
    if config.allow_multiple_instances {
        return StartupStep::OpenWindow;
    }
    match platform.instance.hand_off_or_listen(path) {
        HandOff::Answered => StartupStep::Exit,
        HandOff::Listening | HandOff::Alone => StartupStep::OpenWindow,
    }
}

pub fn run() -> eframe::Result {
    crate::bench::mark("main");
    let arg = std::env::args_os().nth(1);
    // A window opened by Ctrl+Shift+V shows the clipboard image (§3.5, §6.1).
    let from_clipboard = arg.as_deref()
        == Some(std::ffi::OsStr::new(
            zimged_platform::launcher::NEW_FROM_CLIPBOARD_ARG,
        ));
    let path = arg.filter(|_| !from_clipboard).map(PathBuf::from);

    // 1. Both threads start at once. The decode only needs the display box after the JPEG is
    //    decoded, by which time the config has long been read.
    let (box_tx, box_rx) = mpsc::channel::<Size>();
    let first_image = path.clone().map(|path| {
        PendingDecode::spawn(path, move || {
            box_rx
                .recv()
                .unwrap_or_else(|_| startup_display_box(&Config::default()))
        })
    });
    let platform = Platform::os();
    let config_read = {
        let store = platform.config.clone();
        std::thread::Builder::new()
            .name("config".into())
            .spawn(move || Config::load(store.as_ref()))
            .expect("spawn config thread")
    };
    let config = config_read.join().unwrap_or_default();
    let _ = box_tx.send(startup_display_box(&config));

    // 2. Single instance: only when multiple instances aren't allowed. *New from clipboard* asked
    //    for a new window explicitly (§6.1), so it never hands off.
    if !from_clipboard && single_instance(&platform, &config, path.as_deref()) == StartupStep::Exit
    {
        return Ok(());
    }

    // 3. The window and GL context, in parallel with the decode.
    let title = match &path {
        Some(path) => crate::strings::window_title(Some(
            &path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy(),
        )),
        None if from_clipboard => {
            crate::strings::window_title(Some(crate::strings::CLIPBOARD_IMAGE))
        }
        None => crate::strings::window_title(None),
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("zimged")
            .with_title(title)
            .with_inner_size([config.window.width, config.window.height])
            .with_maximized(config.window.maximised),
        renderer: eframe::Renderer::Glow,
        // Ctrl+C / Ctrl+V reach the app as keys: zimged talks to the clipboard itself (§8.1).
        clipboard_shortcuts: false,
        ..Default::default()
    };
    let launch = Launch {
        platform,
        config,
        first_image,
        cache_cap: zimged_siblings::prefetch::CACHE_CAP_BYTES,
        history_pixel_cap: zimged_core::HISTORY_PIXEL_CAP,
        from_clipboard,
    };
    eframe::run_native(
        "zimged",
        options,
        Box::new(move |cc| {
            crate::bench::mark("renderer_ready");
            // 4. The window and GL context are ready. eframe keeps the window hidden until its
            //    first frame, so wait for the decode here: that first frame is the image, not an
            //    empty window followed by the image. A slow decode (a huge file) doesn't hold the
            //    window back for more than FIRST_FRAME_WAIT.
            if let Some(first) = &launch.first_image {
                first.wait(FIRST_FRAME_WAIT);
            }
            Ok(Box::new(ZimgedApp::new(cc, launch)))
        }),
    )
}
