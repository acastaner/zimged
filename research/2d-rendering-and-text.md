# 2D rendering and text libraries for Annotations

Research for [#4](https://github.com/acastaner/zimged/issues/4), part of the v1 spec map [#1](https://github.com/acastaner/zimged/issues/1). Sources were read on 2026-10-29.

## Question

Which 2D library should draw Annotations and do the pixel work? The work covers anti-aliased strokes and fills, arrowheads, dashed lines, drop shadows on objects, shaped text with system fonts, fast blur and pixelate over regions, and high-quality resampling for resize and zoom. The stack is not chosen yet, so each candidate is judged against Rust, C++ (including Qt) and Go.

The constraints that matter, from the map:

- **Single native binary**, no webview.
- **Cold start under 150 ms** to pixels for a 12 MP JPEG, and Sibling switching under 50 ms. Any GPU device creation and shader compilation therefore sits on the critical path. Binary size and the number of dynamic libraries also count.
- **Prefer mature, popular libraries over bespoke code.** GPL is acceptable.
- The editor needs these from the renderer: rectangle, ellipse, line, arrow and freehand shapes; a text box and step counter; highlights (text, area, grayscale-outside, magnify); obfuscation (pixelate, blur); crop, rotate, flip and resize; and per-object thickness, colour, shadow and font.

## Short answer

- **Rust:** use **Skia through `skia-safe`** on the CPU raster backend, with the `textlayout` feature. It is the only Rust-reachable option that ships every required primitive, including blur, drop shadow, cubic and mipmap resampling, and HarfBuzz/ICU shaping. A pure-Rust alternative is **tiny-skia + cosmic-text + fast_image_resize** plus a small hand-written blur. It is far smaller and builds faster, but it means more bespoke code. **Vello** is promising but not yet API-stable. Watch it; don't adopt it for v1.
- **C++ with Qt:** use **QPainter on the raster engine** with Qt's own text stack (QFont, QTextLayout). Qt has no public blur for a `QImage` and its documented smooth scaling is only bilinear. Plan on a small blur routine and a better downscaler, or embed Skia for those two jobs.
- **C++ without Qt:** **Skia** again. **Blend2D** is the fastest CPU rasteriser, but it documents no blur or shaping, so it would need HarfBuzz and FreeType plus your own filters.
- **Go:** no mature library covers the requirement. Shaping is solved by `go-text/typesetting`, but blur, drop shadow and good resampling would be bespoke or cgo-bound. That counts against Go as a stack for this ticket.

## Comparison table

"Not documented" means the primary docs read here don't mention the feature. It doesn't prove the feature is absent.

| Library | Quality | Speed | GPU / CPU | Binary-size impact | Text shaping | Blur / pixelate | Resampling | Licence |
|---|---|---|---|---|---|---|---|---|
| **Skia** (C++; Rust via `skia-safe`) | Reference quality; it is Chrome's and Android's renderer | Fast CPU raster; tiny-skia's README puts tiny-skia 20–100% slower on x86-64 | CPU raster by default (rust-skia ≥0.25 is CPU-only by default). Optional Ganesh/Graphite GPU backends: GL, Vulkan, Metal, D3D | Large: 3–8 MiB per tiny-skia's README. Building from source needs LLVM, Python and Ninja; `skia-safe` downloads prebuilt binaries | Yes. `textlayout` gives SkShaper (HarfBuzz + ICU) and SkParagraph; ICU data is embedded by default | Blur: `SkImageFilters::Blur`, `DropShadow`, `DropShadowOnly`. Pixelate: none built in (downscale then upscale with nearest sampling) | `SkSamplingOptions`: nearest/linear, mipmaps, cubic (`Mitchell()`, `CatmullRom()`), anisotropic | BSD-3 (Skia); MIT (rust-skia) |
| **tiny-skia** (Rust) | Skia-derived anti-aliasing for fills, strokes, dashes and gradients | 20–100% slower than Skia on x86-64, 100–300% on ARM, but "faster than cairo and raqote in many cases" | CPU only; GPU is out of scope | About 200 KiB; about 14 KLOC; builds in under 5 s | **None** ("the main missing feature"). Pair it with cosmic-text or parley | None. Write your own blur and pixelate | `FilterQuality` Nearest/Bilinear/Bicubic for patterns, but no bilinear or mipmap downscaling | BSD-3 |
| **Vello** (Rust; `vello_cpu`, `vello_gpu`, `vello`) | High (analytic AA, W3C filter effects) | Designed for speed: SIMD and multithreaded CPU; GPU paths | `vello_cpu` (most mature), `vello_gpu` (wgpu/WebGL2, "early stages"), compute `vello` (experimental) | Not documented; wgpu adds a lot if the GPU path is used | Glyph runs via `glifo`; shaping comes from parley (HarfRust) | `filter_effects` (W3C-based). Complex filter graphs panic, and filters don't run in multithreaded mode | Bilinear/bicubic image sampling (texel-boundary fixes in 0.3.0) | Apache-2.0 / MIT |
| **Blend2D** (C/C++) | High (8-bit AA, all Porter-Duff ops, blend modes) | Very fast: JIT pipelines for SSE2–AVX-512 and ASIMD, plus a multithreaded mode. Its own benchmark compares it with AGG, Cairo, Qt, JUCE and Skia | CPU only | Small to moderate; one dependency (AsmJit, needed only for the JIT) | Loads TrueType/OpenType fonts and renders text. **Shaping not documented**, so plan on HarfBuzz | Not documented | Not documented | Zlib |
| **Cairo** (C) | Good, mature | Slower than tiny-skia in many cases (per tiny-skia's README) | CPU image surface; 1.18 deprecated the Skia surface | Moderate; pulls in pixman, plus FreeType and Fontconfig on Linux | Not itself. Pair it with Pango/HarfBuzz | None | Pattern filters (fast/good/best/bilinear); not reviewed in depth | LGPL-2.1 / MPL-1.1 |
| **QPainter** (Qt, C++) | Good AA, dashes, pens and brushes; full Qt text | Good raster engine; numbers not reviewed | Raster engine on CPU; Qt Quick/RHI on GPU for the UI | Qt's core and GUI libraries add tens of MB (dynamic) or a large static binary | Yes. Full Unicode layout, bidi and complex scripts through QTextLayout; system fonts via QFontDatabase | Blur only via `QGraphicsBlurEffect` (widgets and graphics items), not on a `QImage`. Pixelate: scale down then up with `FastTransformation` | `SmoothTransformation` is documented as **bilinear** only | LGPLv3 / GPL (some modules GPL-only) |
| **Go: Gio renderer / tdewolff/canvas / gg** | canvas: good vector quality; gg: basic | canvas claims an optimised rasteriser with assembly; others not documented | Gio: GPU; canvas and gg: CPU | Small (pure Go) | Gio and Fyne use go-text/typesetting; canvas uses HarfBuzz/FriBidi (Go or cgo) | None documented | `x/image/draw`: NearestNeighbor, ApproxBiLinear, BiLinear ("slow"), CatmullRom ("very slow") | canvas MIT (FriBidi LGPL); gg MIT |

### Text stacks

| Stack | What it does | Fits | Licence |
|---|---|---|---|
| **HarfBuzz** | The reference OpenType shaper, used by Chrome, Firefox, Android, GNOME, Qt and LibreOffice. It now ships `libharfbuzz-raster` and `libharfbuzz-gpu`, but has no hinting (use FreeType or Skrifa for that) | C++, and Rust via FFI | Old MIT |
| **FreeType** | Font loading and hinted rasterisation | C++ (Skia, Cairo and Qt use it on Linux) | FTL or GPLv2 |
| **HarfRust** | Rust port of HarfBuzz that tracks HarfBuzz v15.0.0. It forked from rustybuzz, has no `unsafe` code, and is under 25% slower than HarfBuzz on common fonts | Rust | MIT |
| **cosmic-text** | Multi-line text: HarfRust shaping, fontdb system fonts, Chromium/Firefox fallback lists, bidi, editing and selection; swash rasterising with colour emoji | Rust (tiny-skia stack) | Apache-2.0 / MIT |
| **parley + fontique** | Rich text layout: HarfRust shaping, line breaking, bidi, system-font enumeration and fallback, selection and editing helpers | Rust (Vello stack) | Apache-2.0 / MIT |
| **go-text/typesetting** | Pure-Go shaping, bidi, segmentation and font scanning (`fontscan`). Shared by Fyne, Gio and Ebitengine; still v0.x | Go | see repo |

## How each requirement maps onto the libraries

- **Strokes, fills, dashes, arrowheads:** every candidate does these. Arrowheads are plain paths everywhere.
- **Drop shadow on objects:** Skia has `DropShadow` and `DropShadowOnly` image filters. `vello_cpu` has drop-shadow and inset-shadow primitives. The others have no shadow: draw the shape into an offscreen mask, blur it, and composite it with an offset. That makes blur the real dependency.
- **Blur over a region (obfuscate):** a built-in, quality-controlled Gaussian blur exists only in Skia (`SkImageFilters::Blur` with a crop rect) and Vello CPU (single filters). QPainter has `QGraphicsBlurEffect`, but it is a widget/item effect and not suited to blurring part of a `QImage` before Flatten. Elsewhere the blur is hand-written, typically a three-pass box or stack blur. That is about 100 lines and well understood, but it is bespoke.
- **Pixelate:** no library has a pixelate filter. It's the same everywhere: downscale the region with box or area averaging, then upscale with nearest sampling. This is trivial with any of the candidates.
- **Grayscale-outside and highlight:** these are colour filters and blend modes. Skia has `SkColorFilters`; Blend2D, QPainter and Vello have blend and composition modes. Grayscale itself is a trivial per-pixel loop.
- **Magnify:** this is a clipped, scaled redraw of the Image, so it comes down to sampling quality.
- **Resampling for zoom and resize:** Skia is the strongest here, with mipmaps for zoom-out and Mitchell or Catmull-Rom cubic for resize. Qt documents only bilinear. tiny-skia has bicubic patterns but no mipmap or bilinear downscaling, so it needs a companion such as `fast_image_resize` (Lanczos3 and others, SSE4.1/AVX2/NEON). Its AVX2 path is about 13 times faster than the `image` crate on a 4928×3279 → 852×567 Lanczos3 resize: 13.26 ms against 173.98 ms. For the fit-to-window path in the 150 ms budget, a fast box/area pre-reduction followed by a cubic final pass is the usual approach whatever the library.
- **Shaped text with system fonts:** Skia (textlayout), Qt and the Rust trio (HarfRust with cosmic-text or parley) all handle complex scripts, bidi and fallback. Blend2D and tiny-skia need a separate shaper. Go has go-text/typesetting.

## GPU or CPU

The editor composites a single Image plus a handful of Annotations, and Flatten has to produce pixels in memory anyway (save, copy). A CPU raster backend is therefore sufficient for v1, and it avoids GPU device and shader start-up costs on the 150 ms cold-start path. Display can then be a plain texture upload done by the chosen GUI toolkit. Both Skia and Vello keep a GPU path open for later. This is engineering judgement, not a measured result; prototype it when the stack is chosen (#9).

## Recommendation per stack

### Rust

1. **Primary: `skia-safe`** with features `textlayout` (and maybe `svg` later). Use the CPU raster surface; leave `gl`/`vulkan`/`metal`/`d3d` off for v1. It covers every editor requirement with mature code: blur, drop shadow, colour filters, cubic and mipmap resampling, HarfBuzz/ICU shaping and system font managers. Costs: a few MiB of binary, prebuilt-binary downloads on build (or an LLVM, Python and Ninja toolchain), and on Linux a requirement of glibc ≥ 2.31. The newest release is 0.156.0 (Skia m156, a pre-release dated 26 October).
2. **Pure-Rust alternative:** `tiny-skia` + `cosmic-text` + `fast_image_resize`, plus a hand-written box or stack blur for blur and shadows. It is about 200 KiB of rasteriser, builds in seconds and has no C++ toolchain. It goes against "prefer mature libraries" (tiny-skia calls itself "more of a research project"), and blur and shadow would be your own code.
3. **Watch, don't adopt yet: Vello.** `vello_cpu` and `vello_gpu` 0.3.0 (October) have no API-stability guarantees. Filter graphs are limited, and filters don't run with multithreading.

### C++ with Qt

**QPainter on `QImage`** (raster engine) for strokes, fills and text, and Qt's text stack (HarfBuzz underneath; HarfBuzz lists Qt as a user) for system fonts and shaping. Fill two gaps:

- **Blur and drop shadow:** a small separable blur over a `QImage::Format_ARGB32_Premultiplied` buffer. `QGraphicsBlurEffect` only applies to scene items and widgets.
- **High-quality downscale:** `Qt::SmoothTransformation` is documented as bilinear. Pre-reduce by area averaging, or add a cubic or Lanczos resampler. Check the visual quality of Qt's smooth downscale in a prototype before deciding.

If those gaps grow, Skia can be embedded alongside Qt for the pixel work. Qt is LGPLv3/GPL; for a GPL project, static linking for a single binary is fine. Qt itself is the largest dependency of any option here.

### C++ without Qt

**Skia** for the same reasons as in Rust. Blend2D (Zlib, 0.21.2) is the speed leader and the easiest to build. But with no documented blur, filters or shaping, it would need HarfBuzz and FreeType plus custom filters, which is more bespoke code than Skia.

### Go

Go has no mature, popular 2D library that meets this requirement:

- `tdewolff/canvas` is capable and fast but says "bugs are likely" and asks for funding.
- `fogleman/gg` documents no shaping.
- Gio's renderer is tied to its GUI.

Shaping is solved (go-text/typesetting, shared by Fyne, Gio and Ebitengine). Resampling in `golang.org/x/image/draw` works, but its own docs call CatmullRom "very slow". Blur and shadow would be hand-written, and no official or maintained Go binding for Skia or Blend2D was found (Blend2D's binding list has none for Go). **For this ticket, Go is the weakest stack.**

## Open points for the stack decision (#9)

- Measure Skia CPU raster plus a toolkit texture upload against the 150 ms cold start on a 12 MP JPEG, especially the fit-to-window downscale.
- Measure the binary size of `skia-safe` with `textlayout` and embedded ICU data. No primary source gives a number beyond "3–8 MiB" for Skia itself.
- For Qt, check `SmoothTransformation` zoom-out quality at large ratios before trusting the documented bilinear behaviour.

## Sources

- tiny-skia README: https://github.com/linebender/tiny-skia
- tiny-skia `FilterQuality`: https://docs.rs/tiny-skia/latest/tiny_skia/enum.FilterQuality.html
- Vello README: https://github.com/linebender/vello
- Vello releases (vello 0.11.0, vello_cpu/vello_gpu 0.3.0, glifo 0.4.0): https://github.com/linebender/vello/releases
- vello_cpu docs: https://docs.rs/vello_cpu/latest/vello_cpu/
- rust-skia README: https://github.com/rust-skia/rust-skia
- skia-safe features: https://github.com/rust-skia/rust-skia/blob/master/skia-safe/README.md
- rust-skia releases (0.156.0): https://github.com/rust-skia/rust-skia/releases
- Skia `SkImageFilters`: https://api.skia.org/classSkImageFilters.html
- Skia `SkSamplingOptions`: https://api.skia.org/structSkSamplingOptions.html
- Skia `SkCubicResampler`: https://api.skia.org/structSkCubicResampler.html
- Blend2D: https://blend2d.com/
- Blend2D repo: https://github.com/blend2d/blend2d
- Blend2D downloads and bindings (0.21.2): https://blend2d.com/download.html
- Blend2D performance: https://blend2d.com/performance.html
- Cairo news: https://www.cairographics.org/news/
- Cairo 1.18.0 notes: https://www.cairographics.org/news/cairo-1.18.0/
- Qt `QImage`: https://doc.qt.io/qt-6/qimage.html
- Qt namespace (`TransformationMode`): https://doc.qt.io/qt-6/qt.html
- Qt `QGraphicsBlurEffect`: https://doc.qt.io/qt-6/qgraphicsblureffect.html
- Qt `QTextLayout`: https://doc.qt.io/qt-6/qtextlayout.html
- Qt licensing obligations: https://www.qt.io/licensing/open-source-lgpl-obligations
- HarfBuzz: https://github.com/harfbuzz/harfbuzz
- HarfBuzz licence: https://raw.githubusercontent.com/harfbuzz/harfbuzz/main/COPYING
- HarfRust: https://github.com/harfbuzz/harfrust
- FreeType licence: https://freetype.org/license.html
- cosmic-text: https://github.com/pop-os/cosmic-text
- parley: https://github.com/linebender/parley
- fast_image_resize: https://github.com/Cykooz/fast_image_resize
- tdewolff/canvas: https://github.com/tdewolff/canvas
- fogleman/gg: https://github.com/fogleman/gg
- go-text/typesetting: https://github.com/go-text/typesetting
- Gio: https://github.com/gioui/gio
- `golang.org/x/image/draw`: https://pkg.go.dev/golang.org/x/image/draw

Not verified against a primary source in this pass: the Cairo licence and filter list, and the Cairo row's binary-size note (stated from general knowledge). Skia's 3–8 MiB figure comes from tiny-skia's README, not from Skia itself.
