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

## Rerun with the performance power profile

The user switched the power profile; `powerprofilesctl` reports **performance** (EPP `performance`). Same method, 15 runs per setup, each after 1.5 s idle. Results are in `bench/results-linux-performance.json`.

| Setup | First image p50 / p90 / max | Phases (median ms) | Flip every 200 ms: p50 / p90 | Holding the key: p50 / p90 / max |
|---|---|---|---|---|
| Qt, Wayland | 135 / 146 / 151 | decoded 55 · QApplication 121 · painted 135 | 3.4 / 3.7 | 10.6 / 42.5 / 122 |
| Qt, X11 | 144 / 158 / 162 | decoded 54 · QApplication 125 · painted 144 | 3.5 / 4.0 | 11.8 / 41.3 / 125 |
| egui + glow, Wayland | **96 / 110 / 118** | decoded 77 · renderer ready 94 · painted 96 | 2.0 / 5.7 | 33.2 / 66.9 / 160 |
| egui + glow, X11 | **95 / 101 / 102** | decoded 78 · renderer ready 92 · painted 95 | 0.8 / 1.0 | 25.0 / 66.5 / 158 |
| egui + wgpu, Wayland | 221 / 239 / 241 | decoded 75 · renderer ready 215 · painted 221 | 1.4 / 8.3 | 25.5 / 67.4 / 154 |
| egui + wgpu, X11 | 245 / 281 / 288 | decoded 75 · renderer ready 242 · painted 245 | 1.0 / 1.3 | 16.9 / 67.4 / 158 |

**Cold start:**
- **egui + glow has the most headroom:** p90 is about 100–110 ms against the 150 ms budget. Decoding and GL setup run in parallel and finish at about the same time.
- **Qt meets the budget only just:** p90 is 146 ms on Wayland, and its X11 p90 is 158 ms, which misses. Its critical path is `QApplication` setup (about 115 ms after `main`), not decoding. Its reduced-size JPEG decode is the fastest decode measured (about 49 ms after `main`, against about 75 ms for Rust's full decode plus resize).
- **wgpu fails the budget:** about 220–245 ms, all of it GPU device setup.

**Switching Siblings:** every setup is far under 50 ms at a normal pace. Holding the key, Qt is better (p90 about 42 ms, against about 67 ms for egui), helped by its cheaper scaled decode. Both have a long tail of 120–160 ms on progressive JPEGs.

**Power profile:** the result depends heavily on it. Under power-saver nothing came under 225 ms (median).

## Rerun with the balanced power profile (the default)

`powerprofilesctl` reports **balanced** (EPP `balance_performance`). Wayland only, 15 runs per setup, each after 1.5 s idle. Results are in `bench/results-linux-balanced.json`.

| Setup | First image p50 / p90 / max | Phases (median ms) | Flip every 200 ms: p50 / p90 | Holding the key: p50 / p90 / max |
|---|---|---|---|---|
| Qt, Wayland | 154 / 175 / 178 | decoded 75 · QApplication 140 · painted 154 | 8.0 / 11.3 | 14.5 / 49.4 / 126 |
| egui + glow, Wayland | **109 / 122 / 126** | decoded 93 · renderer ready 106 · painted 108 | 4.7 / 11.6 | 33.6 / 75.8 / 158 |
| egui + wgpu, Wayland | 232 / 252 / 260 | decoded 94 · renderer ready 226 · painted 232 | 4.4 / 10.8 | 13.4 / 75.0 / 158 |

**On the default profile, only egui + glow meets the 150 ms budget** (p90 122 ms). Qt misses it, by a small margin at the median (154) and more at p90 (175), and its whole critical path is `QApplication` setup. wgpu misses by about 100 ms. All three are far inside the 50 ms budget for normal Sibling flipping.
