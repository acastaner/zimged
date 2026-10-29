# GUI stack candidates

Research for [#2](https://github.com/acastaner/zimged/issues/2), part of the map [#1](https://github.com/acastaner/zimged/issues/1).
Researched 2026-10-29. Version numbers and dates below are as of that day.

## Question

Which language and GUI toolkit pairings can deliver all of the following:

- a single native binary with no webview
- native Wayland **and** X11, plus Windows 10+ and macOS (arm64 and x64)
- a GPU-accelerated canvas and HiDPI
- clipboard image read and write
- IME-capable text input
- drag-and-drop, trash, and single-instance hooks

Each candidate is judged against the map's hard budgets: **under 150 ms cold start to pixels for a 12 MP JPEG**, and **under 50 ms to switch Sibling**.

## Conclusion (ranked shortlist)

| Rank | Stack | Licence | Verdict |
|---|---|---|---|
| **1** | **C++ / Qt 6 Widgets** (6.12), custom canvas widget (QPainter raster first, QRhi or QOpenGLWidget if needed) | LGPLv3 / GPLv3 / commercial | Meets every requirement out of the box, with the fewest custom parts. The only real risk is the startup budget, which needs a measured spike. |
| **2** | **Rust / winit + wgpu canvas + egui (eframe 0.37)** for the chrome, with `arboard`, `trash` and `directories` | MIT / Apache-2.0 | Viable, and the best fit if Rust is preferred. The OS glue is assembled from several crates, and winit 0.31 is still in beta. GPU-init time is the main startup risk. |
| 3 | C++ / SDL3 + Dear ImGui | zlib / MIT | Strong low-level platform layer (MIME clipboard, Wayland-first, IME). The widgets don't look native, and trash and single-instance are left to us. |
| 4 | Rust / Slint 1.x | GPLv3 / royalty-free / commercial | Good declarative UI. The clipboard API only gained generic (non-text) storage in April 2026, and a free-form annotation canvas works against its model. |
| 5 | Rust / iced 0.14 | MIT | Pre-1.0. IME arrived only in 0.14, and no image clipboard API was found. |
| ✗ | Go / Gio | MIT / Unlicense | Pre-1.0 with no supported releases, and clipboard reads are documented as text. |
| ✗ | Go / Fyne 2.x | BSD-3 | **Fails a hard requirement.** Wayland is a compile-time build tag, so one binary can't do native Wayland and X11. The clipboard is text-only. |

**Recommendation.** Take Qt 6 Widgets into the stack ADR as the default. Rust with winit, wgpu and egui is the fallback. Before locking either, run a **cold-start spike on both**: open a window and draw a 12 MP JPEG, decoded with libjpeg-turbo DCT-scaled to window size, then time it on one Linux Wayland machine and one Windows machine. The budget is decided by decode and GPU or context init, not by the toolkit's widgets (see [Performance](#performance-against-the-budgets)).

## Requirement matrix

Legend: ✅ built in · 🧩 via a mature companion library · ⚠️ partial, or needs our own code · ❌ missing or blocking.
"(T)" marks a claim from my prior knowledge that I didn't re-verify against a primary source in this session.

| Requirement | Qt 6 | Rust winit+wgpu+egui | SDL3+ImGui | Slint | iced | Gio | Fyne |
|---|---|---|---|---|---|---|---|
| Wayland **and** X11 in one binary | ✅ runtime QPA plugin, `wayland` or `xcb` [Q3][Q4] | ✅ runtime (T); actively developed [R1] | ✅ Wayland preferred, X11 fallback [S2] | ✅ through winit (T) | ✅ through winit (T) | ✅ (T) | ❌ build tag `-tags wayland` [F2] |
| Windows 10+, macOS arm64/x64 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ [G2] | ✅ |
| GPU canvas | ✅ QRhi: Vulkan/Metal/D3D11/D3D12/GL, private API since 6.6 [Q5]; or QOpenGLWidget (T) | ✅ wgpu, or Skia through rust-skia (MIT; GL/Vulkan/Metal/D3D) [R6] | ✅ SDL_GPU / SDL_Renderer (T) | ✅ femtovg (GLES2), Skia, software [R4] | ✅ wgpu, tiny-skia [R5] | ✅ GPU (T) | ✅ OpenGL (T) |
| HiDPI, per-monitor | ✅ (T) | ✅ winit scale factor (T) | ✅ (T) | ✅ (T) | ✅ (T) | ✅ (T) | ✅ (T) |
| Clipboard image **read and write** | ✅ `QClipboard::setImage()` / `image()` [Q2] | 🧩 `arboard` text+image; Wayland needs the `wayland-data-control` feature and a compositor with ext/wlr data-control, otherwise XWayland [R3]. winit 0.31 beta adds a clipboard API [R1]. egui 0.37 adds `Event::PasteImage` [R2] | ✅ `SDL_SetClipboardData`/`GetClipboardData` by MIME type, lazily encoded [S1] | ⚠️ "Generic clipboard storage" merged April 2026 (PR #11407); general clipboard API issue #2570 still open [R7] | ⚠️ no image clipboard in release notes [R5]; arboard on the side | ⚠️ `ReadCmd` documented as text; `WriteCmd` has a `Type` field [G1] | ❌ `Content() string` only [F1] |
| IME text input | ✅ mature (T) | ✅ winit `Ime` events, many fixes in 0.30.13 and 0.31 betas [R1]; egui reworked composition in 0.35 [R2] | ✅ `SDL_StartTextInput` + text-input rect (T); ImGui `PlatformSetImeDataFn` (T) | ✅ (T) | ✅ new in 0.14 [R5] | ✅ (T) | ⚠️ (T) |
| Drag-and-drop files in | ✅ (T) | ✅ winit; 0.31 reworks DnD into `DragEntered/Moved/Dropped/Left` [R1] | ✅ `SDL_EVENT_DROP_*` (T) | ✅ (T) | ✅ (T) | ⚠️ (T) | ✅ (T) |
| Trash | ✅ `QFile::moveToTrash()` since 5.15 (T) | 🧩 `trash` 5.2.9, MIT, freedesktop Trash spec on Linux [R8] | ⚠️ none, needs per-OS code | 🧩 `trash` | 🧩 `trash` | ⚠️ own code | ⚠️ own code |
| Single instance plus forwarding the open | 🧩 KDSingleApplication, MIT [Q6] | ⚠️ own code over a local socket or named pipe (crates exist) | ⚠️ own code | ⚠️ own code | ⚠️ own code | ⚠️ own code | ✅ (T) |
| Config dir helper | ✅ `QStandardPaths` (T) | 🧩 `directories`/`dirs` crates (T) | ✅ `SDL_GetPrefPath` (T) | 🧩 | 🧩 | ⚠️ `os.UserConfigDir` | ✅ |
| Image decode (map formats) | ✅ QImageReader plugins: PNG/JPEG/GIF/BMP/ICO; TIFF/WebP via qtimageformats; AVIF/HEIC via KImageFormats (T) | 🧩 `image` crate, plus `zune-jpeg` or turbojpeg bindings (T) | 🧩 SDL3_image (T) | 🧩 `image` crate | 🧩 `image` crate | 🧩 Go `image` + x/image | 🧩 |
| Native file dialogs | ✅ | 🧩 `rfd` (T) | ✅ SDL3 dialog API (T) | 🧩 `rfd` | 🧩 `rfd` | ⚠️ | ✅ |
| Accessibility (map: not yet specified) | ✅ | ✅ AccessKit is required in egui since 0.34 [R2] | ❌ | ✅ (T) | ⚠️ (T) | ⚠️ | ⚠️ |

## Candidates in detail

### 1. Qt 6 Widgets (C++)

- **Maturity:** Qt 6.12.0 shipped on 30 September 2026 [Q7]. It is the most mature cross-platform desktop toolkit on this list, and long-lived image viewers use it (nomacs, qView, gwenview's Qt base, and so on (T)).
- **Licence:** the core modules we'd need (Core, Gui, Widgets, Wayland client, imageformats) are LGPLv3/GPLv3. The GPL-only list covers Quick 3D, Graphs, Virtual Keyboard, Wayland *Compositor* and similar, none of which we'd need [Q1]. A GPL licence for zimged also lets us **link Qt statically** into one binary with no LGPL relinking burden.
- **Fit:** everything in the matrix is built in except single-instance, and KDAB's MIT KDSingleApplication covers that [Q6]. Wayland versus X11 is chosen at runtime through the QPA plugin (`-platform wayland|xcb`, or `QT_QPA_PLATFORM`) [Q3][Q4], so one Linux binary serves both. `QClipboard::setImage/image` handles images; note that the X11 clipboard is event-loop driven [Q2].
- **Canvas:** start with a custom `QWidget` drawn with QPainter, which is CPU-raster. That is enough for annotations, and the zoomed bitmap can be cached. If smooth zoom or pan of 12 MP+ images needs the GPU, use QOpenGLWidget or QRhi. QRhi is Qt's own Vulkan/Metal/D3D/GL abstraction, but it sits in `QtGuiPrivate` with limited compatibility guarantees across Qt versions [Q5]. That is acceptable if Qt is pinned.
- **Trade-offs:**
  - The C++ toolchain and CMake are needed.
  - A static Qt build is a CI chore on three OSes.
  - Wayland client-side decorations come from Qt's own plugin, so they look non-native on GNOME (T).
  - The binary is larger, roughly 15–30 MB static (T, to measure).

### 2. Rust: winit + wgpu (+ egui/eframe)

- **Maturity:**
  - winit 0.30.13 is stable. 0.31.0-beta.4 (24 October) reworks DnD, adds a cross-platform clipboard and data-transfer API, and fixes many IME issues on Wayland, Windows and macOS [R1]. A pre-1.0 crate with active breaking churn.
  - egui/eframe 0.37.0 (15 October) is MIT/Apache, depends on winit ^0.30.13, and uses wgpu by default with glow optional [R2][R9]. It is widely used for tools.
- **Licence:** MIT/Apache-2.0 throughout. rust-skia is MIT if Skia is preferred over wgpu for 2D vector annotation [R6].
- **Fit:** one Linux binary handles both Wayland and X11. Clipboard images come from `arboard` [R3]; on Wayland it needs a compositor with the data-control protocol (GNOME supports ext-data-control only in recent versions (T)), otherwise it falls back to XWayland. That fallback is a real-world gap to test. egui 0.37 can now receive pasted images [R2]. Trash comes from the `trash` crate [R8], and single-instance is ours to write.
- **Canvas:** draw the image as a wgpu texture with our own pipeline. egui draws tool chrome and annotation handles; annotation strokes go either through egui's tessellator or through Skia or vello. HiDPI and IME are via winit.
- **Trade-offs:**
  - The look is immediate-mode and non-native.
  - It is assembled from 6–10 crates, which goes against the "prefer mature libraries" rule on maturity, though not on popularity.
  - wgpu adapter and device creation can be slow on some drivers, a startup risk to measure (T).
  - The text box annotation needs rich font handling. egui's text is adequate; cosmic-text/parley is an option (T).

### 3. SDL3 + Dear ImGui (C++)

- **Licence:** SDL3 is zlib and Dear ImGui is MIT (T).
- **Fit:** the platform layer is excellent:
  - SDL3 prefers Wayland and falls back to X11 at runtime. On GNOME it needs libdecor for decorations [S2].
  - The clipboard is MIME-typed, with lazy encoding through callbacks: it advertises `image/png` and encodes only when asked [S1].
  - It has IME text-input and DnD events (T).

  The gaps are trash, single-instance and native-looking widgets. ImGui is a tool or game UI, and its IME needs per-backend wiring (T). This is a good choice if we want the thinnest stack and accept writing glue.

### 4. Slint (Rust)

- **Licence:** GPLv3, royalty-free or commercial [R4]. A stable 1.x API with femtovg, Skia or software renderers [R4].
- **Concern:** clipboard beyond text only arrived recently ("Generic clipboard storage", merged 21 April 2026), and the broader clipboard API issue #2570 is still open [R7]. Image clipboard is a v1 requirement (Ctrl+C, Ctrl+V). A free-form annotation canvas would be our own rendering, put into the scene as an `Image` or through a renderer underlay. That is more friction than in Qt or egui.

### 5. iced (Rust)

- 0.14.0, MIT, wgpu or tiny-skia renderers. Input-method support appeared only in 0.14 [R5]. No image clipboard API appears in the release notes [R5]. Pre-1.0, with breaking changes every release. Not recommended for a project that wants mature libraries.

### Rejected: Gio and Fyne (Go)

- **Gio:** "Pre-1.0 tags are provided for reference only, and do not designate releases with ongoing support" [G2]. The clipboard read is documented as text [G1].
- **Fyne:** native Wayland is the build tag `-tags wayland`, "only relevant on Linux and BSD" [F2]. That means separate Wayland and X11 binaries, which breaks the platform rule. `fyne.Clipboard` is `Content() string` / `SetContent(string)` only [F1].

## Performance against the budgets

These are reasoned estimates, not measurements. The spike should confirm them.

- **Cold start under 150 ms.** No toolkit's widgets are the bottleneck. The budget goes to:
  1. Process start and dynamic linking. Static linking helps, especially for Qt.
  2. Window plus graphics context creation. GL is usually fast. Vulkan/wgpu adapter enumeration can take tens of milliseconds or more on some drivers (T).
  3. **Decoding 12 MP JPEG.** A full-resolution libjpeg-turbo decode is roughly 50–100 ms single-threaded on a mid-range CPU (T). Decoding with **DCT scaling** to 1/2 or 1/4 for fit-to-window, then the full image in the background, is the standard trick and roughly quarters the decode time (T).
  4. Texture upload.

  Implication: whichever stack is chosen, **show the first frame from a DCT-scaled decode and overlap decode with window creation on a second thread**. Qt can paint with QPainter before any GPU init, which de-risks step 2. With wgpu, consider the GL backend, or a software first frame, if Vulkan init is slow.
- **Sibling switch under 50 ms.** Prefetch makes this decode-independent in every stack. The remaining cost is texture upload of a ~48 MB RGBA buffer, a few milliseconds on GPU, or a QImage→QPixmap conversion in Qt (T). It is achievable everywhere.
- **10,000-file directory.** Enumerate on a worker thread. This is independent of the toolkit.

## Open questions for the stack ADR

- C++ (Qt) or Rust? This is mostly a team-preference call, since both pass. Qt wins on completeness and maturity, Rust on memory safety and licence simplicity.
- Is Qt's non-native Wayland decoration on GNOME acceptable? Is arboard's XWayland fallback on GNOME versions without data-control acceptable?
- Spike results for cold start on Wayland and Windows.

## Sources

- [Q1] Qt licensing, Qt 6.12 docs: https://doc.qt.io/qt-6/licensing.html
- [Q2] QClipboard: https://doc.qt.io/qt-6/qclipboard.html
- [Q3] Wayland and Qt: https://doc.qt.io/qt-6/wayland-and-qt.html
- [Q4] QGuiApplication (`-platform`, `QT_QPA_PLATFORM`): https://doc.qt.io/qt-6/qguiapplication.html
- [Q5] QRhi: https://doc.qt.io/qt-6/qrhi.html
- [Q6] KDSingleApplication: https://github.com/KDAB/KDSingleApplication
- [Q7] Qt 6.12 release: https://wiki.qt.io/Qt_6.12_Release
- [R1] winit releases: https://github.com/rust-windowing/winit/releases
- [R2] egui releases: https://github.com/emilk/egui/releases
- [R3] arboard: https://github.com/1Password/arboard
- [R4] Slint: https://github.com/slint-ui/slint
- [R5] iced releases: https://github.com/iced-rs/iced/releases
- [R6] rust-skia: https://github.com/rust-skia/rust-skia
- [R7] Slint clipboard issues and PRs: https://github.com/slint-ui/slint/issues?q=clipboard+image (#2570, #11399, PR #11407)
- [R8] trash crate: https://docs.rs/trash/latest/trash/
- [R9] eframe: https://docs.rs/eframe/latest/eframe/
- [S1] SDL3 clipboard: https://wiki.libsdl.org/SDL3/CategoryClipboard
- [S2] SDL3 Wayland: https://wiki.libsdl.org/SDL3/README-wayland
- [G1] Gio clipboard: https://pkg.go.dev/gioui.org/io/clipboard
- [G2] Gio repository: https://github.com/gioui/gio
- [F1] Fyne Clipboard: https://pkg.go.dev/fyne.io/fyne/v2#Clipboard
- [F2] Fyne compile options: https://docs.fyne.io/explore/compiling/
