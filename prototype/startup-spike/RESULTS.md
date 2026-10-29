# Cold-start spike: results (PROTOTYPE, throwaway)

For [Cold-start spike for the top stacks](https://github.com/acastaner/zimged/issues/9). Branch `prototype/startup-spike`, never merged.

## Setup

- **Spikes:**
  - `qt/`: Qt 6.10 Widgets with the CPU raster backing store. JPEG decoding uses libjpeg-turbo through `QImageReader::setScaledSize`.
  - `rust/`: eframe/egui 0.37 rendered with glow (OpenGL) or wgpu. Decoding uses the `image` crate (zune-jpeg) plus `fast_image_resize`.
- **Common pipeline:** both start decoding on a thread at `main()`, before any window exists, and decode to window size (1600×1000). They paint fit-to-window, then scan the directory and prefetch the Siblings at +1, −1 and +2.
- **Machine:** i5-12600KF (6 P-cores and 4 E-cores), RTX 3070 Ti on the NVIDIA driver, KDE Plasma Wayland. "X11" means XWayland in the same session. Power profile: **power-saver** (governor `powersave`, EPP `power`).
- **Images:** 30 distinct 12 MP JPEGs (4000×3000, about 4 MB each); IMG_0010, 0020 and 0030 are progressive.
- **Method:** `bench/bench.py` runs 15 cold starts per setup, each after 1.5 s idle. The clock starts just before `exec` and stops when the first image is painted, i.e. the frame is handed to the compositor; it appears on screen at the next vblank. Navigation is timed from the key press to the paint.

## Cold start: double-click to first image

| Setup | p50 | p90 | Phases (median ms from launch) |
|---|---|---|---|
| Qt, Wayland | 349 | 380 | main 22 · decoded 207 · QApplication 310 · painted 349 |
| Qt, X11 | 378 | 436 | main 22 · decoded 208 · QApplication 328 · painted 378 |
| egui + glow, Wayland | **225** | 328 | main 6 · renderer ready 199 · decoded 220 · painted 225 |
| egui + wgpu, Wayland | 431 | 472 | main 6 · decoded 215 · renderer ready 411 · painted 431 |

The first run, which decoded at 2560×1600, gave the same ranking: Qt Wayland p50 293, Qt X11 381, glow 250–261, wgpu 472–486. Turning off the KDE platform theme made no difference to Qt (331). Results are in `bench/results-linux*.json`.

**No setup meets the 150 ms budget on this machine in this power state.**

## Switching Siblings

| Setup | Flipping every 200 ms: p50 / p90 / max | Holding the key: p50 / p90 / max |
|---|---|---|
| Qt, Wayland | 10.7 / 14.1 / 14.8 | 19.2 / 180 / 244 |
| egui + glow, Wayland | 5.5 / 10.8 / 18.4 | 11.6 / 282 / 299 |
| egui + wgpu, Wayland | 5.6 / 10.4 / 11.1 | 14.9 / 215 / 233 |

- **At a normal flipping pace, both stacks meet the 50 ms budget comfortably.** That holds as long as the prefetched image is ready.
- **Holding the key outruns the prefetch.** The p90 is the progressive JPEGs and the images that weren't decoded yet; the run is decode-bound in both stacks.

## What dominates

1. **CPU state at launch.**
   - The first JPEG decode in a fresh process takes about 180–230 ms. The same decode warm takes about 50 ms.
   - Pinned to an E-core, a warm decode takes about 106 ms. Pinned to a P-core coming out of idle, the first decode takes about 184 ms, then about 53 ms.
   - Everything at startup (decode, Qt setup, GL setup) runs about 3–4× slower than steady state under power-saver.
2. **Toolkit and renderer setup.**
   - Qt's `QApplication` reaches about 310 ms. The KDE platform theme isn't the cause.
   - egui's glow context is ready in about 200 ms, wgpu in about 410 ms.
3. **Decode cost.**
   - libjpeg-turbo's scaled decode should do much less work, but under these conditions it wasn't faster than zune's full decode plus resize.
   - It still needs checking whether Qt's `setScaledSize` actually uses DCT scaling here.

## Not measured

Windows and macOS (no hosts available), a balanced or performance power profile, and a native X11 session.
