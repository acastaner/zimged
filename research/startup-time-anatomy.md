# Startup-time anatomy of fast image viewers

Research for [#5](https://github.com/acastaner/zimged/issues/5), under the map [#1](https://github.com/acastaner/zimged/issues/1).
The budget being tested is a cold start under 150 ms from double-click to pixels, for a 12 MP JPEG on a mid-range machine.

## Answer in one paragraph

On every OS, the slowest thing between `exec` and first pixels is **GPU device creation**, not decoding.
On the reference Linux machine it took 90–300 ms for Vulkan and 60–135 ms for EGL, against 50 ms for a full 12 MP baseline JPEG decode.
Defaults in the GPU stack that probe every backend or every adapter can push that to whole seconds.
That is 1.5 s with wgpu on Wayland and NVIDIA, and 4.5 s with wgpu's DX12 backend on a machine with three adapters.
The CPU-only path is quick by comparison: a minimal Wayland `wl_shm` window had its first frame on screen 18–49 ms after `main()`.
A 12 MP JPEG decoded at half scale takes about 30 ms.
150 ms is therefore **realistic on Linux, and likely on Windows and macOS**, under one condition: GPU initialisation is either off the critical path or narrowed to one backend and one adapter.
Decoding must also start at t=0, in parallel with window creation, and must decode at display size.
The budget **cannot be met on the first launch after installation on macOS**, where Gatekeeper takes seconds.
It is also at risk for **progressive JPEGs**, which decode about 2.2× slower.

## Measurement setup (local, this research)

None of the six viewers named in the ticket is installed locally, and no packages were installed.
Instead, the phases were timed directly with small probes. They are standalone C and Python programs, kept out of the repo, on:

- **Machine:** Intel i5-12600KF (desktop), NVIDIA GeForce RTX 3070 Ti, driver 580.178.04.
- **Software:** Linux 7.0, KDE Plasma on Wayland (KWin), with Mesa ICDs also installed.
- **Decoder:** Pillow 12.1.1 on libjpeg-turbo 2.1.5, so the numbers include a little Python overhead.
- **Runs:** each figure is the best of 5–7 runs, warm page cache.

This is a fast desktop CPU, so a "mid-range" laptop CPU will decode about 1.5–2× slower (an estimate, not measured).

## Phase-by-phase cost breakdown

### 1. Process start and runtime initialisation

| Evidence | Number | Source |
|---|---|---|
| `fork`+`exec` of `/bin/true` (Linux, measured from Python `subprocess`) | 1.7 ms | local |
| nomacs (Qt, Windows): "Initialization takes" (app start to active window, theme CSS, geometry restore) | 78 ms and 82 ms | [nomacs#1231](https://github.com/nomacs/nomacs/issues/1231) logs |
| qView (Qt): occasional 5 s+ starts after reboot. A maintainer suspected the *Open Recent* menu checking whether the files still exist, including on network shares | 5 s | [qView#513](https://github.com/jurplel/qView/issues/513) |
| qView: removing the update checker and QtNetwork to speed up startup "makes no difference at all" | — | [qView#151](https://github.com/jurplel/qView/issues/151) |
| ImageGlass 9 (.NET, JIT): a user reports about a minute to start. The maintainer's answer is ImageGlass 10, which uses Native AOT ("startup time is much faster now") | — | [ImageGlass#2188](https://github.com/d2phap/ImageGlass/issues/2188), [IG10 beta 1 notes](https://imageglass.org/news/imageglass-10-beta-1-is-here-99) |
| macOS, notarised app, **first** run after download (Tahoe): 10.4 s to the approval prompt. Later runs take 0.64–0.66 s for a 1 GB Python app (Calibre) | 10.4 s / 0.65 s | [Eclectic Light, 2025-09-30](https://eclecticlight.co/2025/09/30/do-apps-launch-faster-in-macos-tahoe/) |
| Apple's own launch goal is "rendering our first frame within 400 milliseconds" | 400 ms | [WWDC19 session 423](https://developer.apple.com/videos/play/wwdc2019/423/) |

**Reading:**

- For a single native binary, exec and dynamic linking cost single-digit milliseconds on Linux.
- Toolkit initialisation is the first real cost. For Qt on Windows it is about 80 ms, nomacs's whole measured initialisation.
- Managed runtimes with a JIT are far worse, which is why ImageGlass moved to Native AOT.
- Apple's 400 ms target is 2.7× zimged's budget, so 150 ms on macOS is aggressive but not unreasonable for a small native binary.
- Windows Defender real-time scanning of a freshly installed executable adds an unquantified delay. Only anecdotal sources were found, for example [DBeaver's troubleshooting page](https://dbeaver.com/docs/team-edition/desktop/Troubleshooting-Windows-Defender-issues/). Measure it on the first Windows build.

### 2. Display connection and window creation

| Evidence | Number | Source |
|---|---|---|
| Wayland: `wl_display_connect` + registry roundtrip | 2–14 ms | local (`shmwin` probe) |
| Wayland: `xdg_toplevel` creation to first `configure` | 0.2–2.6 ms | local |
| Wayland: fill a 1600×1200 `wl_shm` buffer | 3–19 ms | local |
| Wayland: commit to `wl_surface.frame` done (compositor has used the buffer; includes waiting for vsync) | 8–35 ms | local |
| **Wayland CPU path, `main()` to first presented frame** | **18–49 ms** | local |
| X11/GLX: alacritty maintainer, "window builds in 3.9ms" | 3.9 ms | [alacritty#7472](https://github.com/alacritty/alacritty/issues/7472) |

**Reading:**

- Window creation itself is cheap on both Wayland and X11.
- The xdg-shell protocol forbids attaching a buffer before the first `configure`, so on Wayland one roundtrip must finish before any pixel can be committed ([xdg-shell spec clarification MR](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/merge_requests/374)).
- Waiting for vsync adds up to one refresh interval (16.7 ms at 60 Hz), and the measured variance in the "frame done" step is that wait.

### 3. GPU instance and device creation (the dominant phase)

| Evidence | Number | Source |
|---|---|---|
| Vulkan, NVIDIA, all 9 installed ICDs: `vkCreateInstance` | 45–157 ms | local (`vkprobe`) |
| Vulkan, NVIDIA: `vkEnumeratePhysicalDevices` | 5–15 ms | local |
| Vulkan, NVIDIA: `vkCreateDevice` | 88–135 ms | local |
| **Vulkan total, NVIDIA (all ICDs)** | **186–307 ms** | local |
| Vulkan total, NVIDIA ICD only (`VK_DRIVER_FILES`) | 92–211 ms | local |
| Vulkan total, Mesa lavapipe (software) | 79–82 ms | local |
| **EGL (GLES2), NVIDIA: `eglInitialize` + config + context** | **63–134 ms** | local (`eglprobe`) |
| EGL, Mesa (falls back to software on this box) | 125–160 ms | local |
| wgpu (via iced) on Wayland + NVIDIA: about 1.5 s to start, against "instantly" with the tiny-skia CPU backend. The log shows **both the Vulkan and GL backends** being initialised | 1.5 s | [wgpu#9912](https://github.com/gfx-rs/wgpu/issues/9912) |
| Same issue, another user: most of the time is in `vkEnumerate*ExtensionProperties`, which wakes a sleeping dGPU even when a low-power adapter was requested | — | [wgpu#9912](https://github.com/gfx-rs/wgpu/issues/9912) (comment by edwloef) |
| wgpu DX12, Windows 11, dGPU + iGPU + WARP: `request_adapter` took about 4.5 s because it created a D3D12 device on *every* adapter. With lazy selection of only the chosen one: 356 ms warm, 1.7 s cold. The iGPU's driver initialisation alone was about 2.7 s per process start. (The PR was closed unmerged, pending an extension API, but the measurements stand) | 4.5 s → 356 ms | [wgpu PR #10011](https://github.com/gfx-rs/wgpu/pull/10011) |
| alacritty (GL): a dGPU-capable driver installed makes startup take 2–5 s instead of about 130 ms, because libglvnd opens and wakes the NVIDIA GPU | 1.3–5 s | [alacritty#6366](https://github.com/alacritty/alacritty/issues/6366), [#6359](https://github.com/alacritty/alacritty/issues/6359) |
| Electron on Linux: GDK's OpenGL probe during `gtk_init()` cost 29 ms to `ready` (44 ms to first paint). The fix is `GDK_GL=disable` around `gtk_init` | 29 ms | [electron PR #53070](https://releases.electronjs.org/pr/53070) |
| macOS: `MTLCreateSystemDefaultDevice()` returns the *discrete* GPU on dual-GPU Macs, and waking it made launch slow. No figure is given | — | [Apple Developer Forums 692769](https://developer.apple.com/forums/thread/692769) |
| Firefox (old, D3D10): creating devices at two feature levels was "5-10% of startup time or ~100ms". *Seen only as a search snippet of a mirrored commit, not verified* | ~100 ms | [tor-browser mirror commit](https://gitlab.torproject.org/t-m-w/tor-browser/-/commit/ed784525f3e56e326c6a17363ca737ea1303a997) |

**Reading:**

- This phase alone can exceed the whole 150 ms budget, even on a fast machine with the GPU already awake.
- **Multi-backend and multi-adapter probing is the real killer.** wgpu's default `Backends::all()` initialises Vulkan *and* GL on Linux ([`Backends` docs](https://docs.rs/wgpu/latest/wgpu/struct.Backends.html)), and its DX12 path initialises every adapter.
- There is a second, separate cost: the **first pipeline creation**, meaning shader compilation. wgpu exposes a `PipelineCache` (Vulkan only) to amortise it across runs ([docs](https://docs.rs/wgpu/latest/wgpu/struct.PipelineCache.html)), and pipeline creation is synchronous ([wgpu#3794](https://github.com/gfx-rs/wgpu/issues/3794) asks for async variants).
- Not measured here: Mesa hardware drivers (Intel/AMD), D3D11 or D3D12 on a single adapter, and Metal on Apple Silicon. Those need measuring on real hardware before the stack ADR.

### 4. Fonts

| Evidence | Number | Source |
|---|---|---|
| fontconfig `FcInit`, warm cache, on a helper thread (Electron) | 3.8 ms median | [electron PR #53314](https://releases.electronjs.org/pr/53314) |
| `FcInit` with a big fontconfig configuration (Matthias Clasen). Pango now calls it on a thread. *Commit message seen only via search snippet* | 60–100 ms | [pango commit (commits-list)](https://lists.gnome.org/archives/commits-list/2020-August/msg07345.html) |
| Stale fontconfig cache after an upgrade | ~20 s | [freedesktop bug 98338](https://bugs.freedesktop.org/show_bug.cgi?id=98338) |

**Reading:**

- Fonts are not on the critical path for pixels, since the first frame of a viewer needs no text.
- They are worth deferring all the same: initialise them on a thread, and render the first text-bearing UI (hints, tool labels) after the image.

### 5. Read and decode (12 MP JPEG)

Measured locally with libjpeg-turbo 2.1.5 through Pillow, from bytes already in memory (`draft()` selects libjpeg's DCT-domain scaling):

| File | Full (1/1) | 1/2 | 1/4 | 1/8 |
|---|---|---|---|---|
| 3968×2976, 3.6 MB, baseline (phone camera) | 47.5–52.7 ms | 30.5–31.4 ms | 26.2–27.3 ms | 22.1–22.7 ms |
| 4000×3000, 2.1 MB, baseline (compact camera) | 31.8 ms | 22.3 ms | 18.7 ms | 15.8 ms |
| Same phone image re-encoded q92 **baseline**, 3.0 MB | 49.8 ms | — | — | 20.0 ms |
| Same phone image re-encoded q92 **progressive**, 2.8 MB | **112.0 ms** | — | — | **82.2 ms** |

Corroboration:

- mozjpeg's issue tracker reports progressive decoding at 72 Mpx/s against 97 Mpx/s for baseline, from one user. The libjpeg-turbo maintainer puts progressive with multi-pass optimisation at "~65%" lower decompression performance ([mozjpeg#176](https://github.com/mozilla/mozjpeg/issues/176)).
- The decoder's own logs in nomacs show a JPEG "loaded in 85 ms / 89 ms" on Windows, file size unknown ([nomacs#1231](https://github.com/nomacs/nomacs/issues/1231)).
- Google's Rust decoder benchmark finds `jpeg-decoder` about 30% slower than libjpeg-turbo on large images ([google/decoder-benchmarks-for-rust](https://github.com/google/decoder-benchmarks-for-rust)).

**Reading:**

- A 12 MP baseline JPEG costs about 50 ms at full size on a fast desktop core, probably 75–100 ms on a mid-range laptop (estimated).
- DCT scaling saves 40% at 1/2, but no more than about 55% even at 1/8, because entropy (Huffman) decoding cannot be scaled. Half and quarter scale hold almost all of the gain.
- A fit-to-window display on a 1080p or 1440p screen needs at most 1/2 scale (2000×1500), so the first frame should be decoded at the largest scale that still covers the window.
- **Progressive files break the budget even when scaled.** The remedy is libjpeg's *buffered-image mode*: display the first scan, then refine ([libjpeg.txt, "Buffered-image mode"](https://github.com/libjpeg-turbo/libjpeg-turbo/blob/main/libjpeg.txt)).
- Disk reads were excluded here (warm cache). A cold 3–5 MB read is under 5 ms on NVMe but can be tens of milliseconds on a spinning disk or a network share.

### 6. Upload and first present

- **CPU path:** blitting a scaled RGBA image into a `wl_shm` or softbuffer buffer took 3–19 ms for 1600×1200, measured with a synthetic fill. Commit to on-screen then took 8–35 ms (local).
- **GPU path:** a 2000×1500 RGBA texture is 12 MB, a small upload over PCIe or unified memory. Its cost is dominated by the one-time device and pipeline setup in phase 3, not by the copy.

### 7. Work that is not on the pixel path but often lands there

- **Directory listing and sorting.** nomacs on Windows logged "after sorting: 10 sec" for about 5,000 files on the first run after boot, against 9 ms for 9 files ([nomacs#1231](https://github.com/nomacs/nomacs/issues/1231)).
- **Rescanning the folder.** qView's `updateFolderInfo()` was about 70% of the time when stepping through images in a folder of 600 files (debug build). It ran before the next image was shown ([qView#514](https://github.com/jurplel/qView/issues/514)).
- **The recent-files menu.** qView populates it, with existence checks, at startup ([qView#513](https://github.com/jurplel/qView/issues/513)).
- **Decoder sandboxing.** Loupe's decoder, glycin, starts a sandboxed loader process per format through bubblewrap with seccomp, streams the file over a Unix socket, and keeps the process alive for 30 s for reuse ([glycin README](https://gitlab.gnome.org/GNOME/glycin/-/blob/main/README.md)). That adds a process spawn and a D-Bus handshake to every cold open. glycin publishes no figure for it.

## The six viewers, briefly

| Viewer | Stack (startup-relevant) | What is known about its startup |
|---|---|---|
| **feh** | C, Xlib + Imlib2, no GPU, X11 only | The lower-bound reference: no toolkit, no GPU context, no fonts until the caption is drawn. No published timing was found ([derf/feh](https://github.com/derf/feh)). |
| **qView** | C++/Qt (QGraphicsView) | Startup issues trace to I/O on the start path: the recent-files check and repeated folder rescans ([#513](https://github.com/jurplel/qView/issues/513), [#514](https://github.com/jurplel/qView/issues/514)). |
| **nomacs** | C++/Qt (+ OpenCV, exiv2) | About 80 ms initialisation plus about 85 ms load on Windows. Folder sorting can take seconds ([#1231](https://github.com/nomacs/nomacs/issues/1231)). It has a splash screen, with a request to disable it ([#1005](https://github.com/nomacs/nomacs/issues/1005)). |
| **oculante** | Rust, notan (winit + glow/OpenGL), egui UI, libjpeg-turbo through `turbojpeg` by default, threaded loading | Claims "fast startup", with no published numbers ([Cargo.toml](https://github.com/woelper/oculante/blob/master/Cargo.toml), [README](https://github.com/woelper/oculante)). Being GL-based, it is exposed to the libglvnd dGPU wake-up above. |
| **ImageGlass** | v9: .NET WinForms with JIT, Direct2D. v10: Avalonia, Native AOT | Slow-start complaints. The fixes were (a) a "stay resident" setting in 8.5 ([8.5 notes](https://imageglass.org/news/announcing-imageglass-8-5-74)) and (b) Native AOT in 10 ([#2188](https://github.com/d2phap/ImageGlass/issues/2188)). |
| **Eye of GNOME / Loupe** | EoG: GTK3 + gdk-pixbuf. Loupe: GTK4 (Vulkan renderer default on Wayland since 4.16) + glycin sandboxed loaders | No published timings were found. Loupe pays for the GTK4 GPU renderer's initialisation and for a sandboxed loader process ([GTK 4.16, Phoronix](https://www.phoronix.com/news/GTK-4.16-Released), secondary; [glycin](https://gitlab.gnome.org/GNOME/glycin/-/blob/main/README.md)). |

For comparison, Microsoft Photos also solved launch speed by staying resident: it runs "in the background with minimal processes" by default ([Windows Insider blog, 2024-09-06](https://blogs.windows.com/windows-insider/2024/09/06/microsoft-photos-september-2024-update-begins-rolling-out-to-windows-insiders/)).

No viewer surveyed publishes an end-to-end double-click-to-pixels number. The useful data comes from issue-tracker logs and from component timings.

## Critical-path arithmetic

These estimates use the measured numbers above. "Mid-range" scales decode by about 1.7×.

| Plan | Critical path | Fits 150 ms? |
|---|---|---|
| Serial: exec → window → Vulkan device → full decode → present | 5 + 15 + 90–300 + 85 + 16 = **211–421 ms** | No |
| Serial with EGL instead of Vulkan | 5 + 15 + 63–134 + 85 + 16 = **184–255 ms** | No |
| Decode at 1/2 **in parallel** with window + EGL. First frame on GL | max(15 + 63–134, 50) + 16 + 5 = **99–170 ms** | Borderline (NVIDIA). Mesa iGPU unmeasured |
| Decode at 1/2 in parallel with window. **First frame on the CPU path** (`wl_shm`/softbuffer), GPU initialised afterwards | max(20–50, 50) + 5–20 blit + 16 = **70–90 ms** | **Yes, with margin** |
| Any plan, progressive 12 MP JPEG with no buffered-image first scan | +60 ms or more on decode | At risk |

## Verdict per OS

- **Linux (Wayland and X11): realistic.**
  - The measured CPU-first-frame path fits with about 60 ms to spare on this machine.
  - A GPU-first-frame path fits only if the GPU stack is restricted to one backend and one adapter, and does not wake a sleeping dGPU.
  - The pathological cases (libglvnd waking NVIDIA, wgpu initialising both Vulkan and GL) cost 1–5 s and are user-visible ([wgpu#9912](https://github.com/gfx-rs/wgpu/issues/9912), [alacritty#6366](https://github.com/alacritty/alacritty/issues/6366)).
  - X11 window creation is as cheap as Wayland's ([alacritty#7472](https://github.com/alacritty/alacritty/issues/7472)).
- **Windows: probably realistic for warm launches, not verified.**
  - Process start and a Win32 window are cheap, but no first-party millisecond figure was found for D3D11 or DXGI device creation.
  - The only hard data is wgpu DX12: 356 ms warm even with lazy single-adapter selection, and 4.5 s with default enumeration ([PR #10011](https://github.com/gfx-rs/wgpu/pull/10011)). That alone rules out "DX12 device before first frame" on hybrid machines.
  - A CPU first frame (GDI or a DXGI/DirectComposition swapchain filled from a CPU buffer) or a D3D11 device on the default adapter is the likely route. **Measure it on a real Windows machine before locking the stack.**
  - First run after install also pays a Defender scan that is not quantified.
- **macOS: realistic for second and later launches, impossible for the first launch after download.**
  - First launches of notarised apps take seconds in Gatekeeper and CloudKit checks ([Eclectic Light](https://eclecticlight.co/2025/09/30/do-apps-launch-faster-in-macos-tahoe/)). The spec should state that the 150 ms budget excludes the first run after install or update.
  - On Intel dual-GPU Macs, `MTLCreateSystemDefaultDevice` wakes the dGPU ([forum thread](https://developer.apple.com/forums/thread/692769)). Prefer the low-power device, or a CPU-backed layer for the first frame.
  - No Metal device-creation figure for Apple Silicon was found. **Measure it.**

## Concrete techniques, ranked by payoff

1. **Start decoding at t=0, before or alongside window creation.**
   - Parse the command line, open the file and start a decode thread before connecting to the display.
   - On this machine, window creation (15–50 ms) and decode (30–50 ms) are similar in size, so running them in parallel roughly halves the critical path.
2. **Decode at display size first.** Use the largest DCT scale that still covers the window, then decode at full resolution in the background for zoom.
   - libjpeg-turbo: `scale_num`/`scale_denom`, or TurboJPEG scaling factors ([libjpeg.txt](https://github.com/libjpeg-turbo/libjpeg-turbo/blob/main/libjpeg.txt)).
   - Windows WIC: `IWICBitmapSourceTransform::CopyPixels` with a scaled size ([docs](https://learn.microsoft.com/en-us/windows/win32/api/wincodec/nn-wincodec-iwicbitmapsourcetransform)).
   - macOS ImageIO: `CGImageSourceCreateThumbnailAtIndex` with `kCGImageSourceThumbnailMaxPixelSize` ([docs](https://developer.apple.com/documentation/imageio/cgimagesourcecreatethumbnailatindex(_:_:_:))).
   - Measured saving: 40% at 1/2.
3. **Present the first frame from the CPU, and bring the GPU up afterwards, or not at all for v1.**
   - On Wayland a `wl_shm` frame was on screen 18–49 ms after `main()`.
   - softbuffer offers the same path through winit on all three OSes ([rust-windowing/softbuffer](https://github.com/rust-windowing/softbuffer)).
   - iced's own users see "instant" startup with tiny-skia against 1.5 s with wgpu ([wgpu#9912](https://github.com/gfx-rs/wgpu/issues/9912)).
   - *Open question for a prototype:* switching one window from shm buffers to a GPU swapchain without a visible flash.
4. **If the GPU is used, narrow its initialisation.**
   - Request exactly one backend per OS. Do not use `Backends::all()`.
   - Pass `compatible_surface` and `PowerPreference::LowPower`.
   - Never call `enumerate_adapters` on the start path ([wgpu PR #10011](https://github.com/gfx-rs/wgpu/pull/10011), [wgpu#9912](https://github.com/gfx-rs/wgpu/issues/9912)).
   - Use a `PipelineCache` (Vulkan) or precompiled shaders ([wgpu docs](https://docs.rs/wgpu/latest/wgpu/struct.PipelineCache.html)).
   - Do GPU initialisation on a thread, concurrent with decoding.
   - With GTK, avoid the GL probe if GL is not used (`GDK_GL=disable` precedent, [electron PR #53070](https://releases.electronjs.org/pr/53070)).
5. **Keep everything else off the pixel path.**
   - Directory enumeration and natural sort (nomacs: 10 s for 5,000 files).
   - Recent-files checks (qView).
   - Fonts (`FcInit` on a thread, 4–100 ms).
   - Config, EXIF beyond orientation, ICC transforms, update checks.
   - The map's rule that "a directory of 10,000 files doesn't delay the first image" is exactly this item.
6. **Handle progressive JPEG explicitly.** Use buffered-image mode to show the first scan, about 1/8 resolution quality, and refine afterwards ([libjpeg.txt](https://github.com/libjpeg-turbo/libjpeg-turbo/blob/main/libjpeg.txt)).
7. **Ship a native, mostly static binary with no JIT and few dynamic libraries.**
   - ImageGlass needed Native AOT to fix its startup ([#2188](https://github.com/d2phap/ImageGlass/issues/2188)).
   - Apple recommends fewer dylibs and no `dlopen` at launch ([WWDC19-423](https://developer.apple.com/videos/play/wwdc2019/423/)).
   - Choose a fast decoder: libjpeg-turbo, or a Rust decoder benchmarked against it. Plain `jpeg-decoder` is about 30% slower.
8. **Residency is a fallback, not the plan.** Photos and ImageGlass 8.5 stay resident. That defeats "cold start", but zimged's *Allow multiple instances = off* mode gets the same benefit for free: forwarding an open to the running window skips phases 1–3.

## How to measure "double-click to pixels"

The shell's double-click-to-`exec` latency is outside the app's control, so time from process start, `CLOCK_MONOTONIC` at `main()` or earlier, to the compositor confirming that the frame was used:

- **Wayland:** the `wl_surface.frame` callback, or `wp_presentation` feedback for the exact scanout time.
- **X11:** Present extension `PresentCompleteNotify`.
- **Windows:** `IDXGISwapChain::GetFrameStatistics`, or a DWM timing query.
- **macOS:** the `CAMetalDrawable` `addPresentedHandler`.

The probes used for this note follow that model. Recommendation for the test-strategy ticket: make this a gate with a fixed 12 MP baseline JPEG and a progressive one.

## Gaps and confidence

- **High confidence:** Linux phase costs (measured), decode costs and the scaling curve (measured), the wgpu multi-backend and multi-adapter costs (first-party issue and PR data).
- **Medium:** the Windows verdict. It has no first-party D3D11 figure, and the Defender cost is unknown.
- **Medium:** the macOS verdict. It has no Metal-on-Apple-Silicon figure.
- **Not found:** end-to-end startup numbers for feh, Eye of GNOME, Loupe and oculante.
- **Secondary or snippet-only sources,** marked as such inline: the Firefox D3D10 ~100 ms figure, the pango `FcInit` 60–100 ms figure, and the GTK 4.16 Vulkan default (Phoronix).

The local probe sources (`vkprobe.c`, `eglprobe.c`, `shmwin.c`, `bench.py`) lived in a scratch directory and are not committed. They are about 100 lines each and can be recreated for a prototype ticket.
