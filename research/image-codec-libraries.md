# Image codec libraries

Research for [#3](https://github.com/acastaner/zimged/issues/3) (map: [#1](https://github.com/acastaner/zimged/issues/1)).
Written 2026-10-29. The stack isn't chosen yet, so this covers Rust, C/C++ and Go, plus the C
libraries that all three can call.

Requirements from the map: read PNG, JPEG, WebP, GIF, BMP, TIFF and ICO (plus AVIF and HEIC if they
come cheap). Write PNG, JPEG and WebP. Animations play, then freeze on frame 1 at the first edit.
EXIF orientation is respected, and EXIF and ICC data survive a save. Hard budgets: under 150 ms cold
start to pixels for a 12 MP JPEG, under 50 ms to move to the next Sibling (with prefetch).

## TL;DR

- **Every ecosystem ends up on the same C core for the hard formats**: libjpeg-turbo, libwebp,
  dav1d/libavif and libheif. Each has bindings in Rust, C++ and Go.
- **Rust is the strongest fit.** `image` 0.25 covers the whole read list and PNG/JPEG writing in pure
  Rust, with metadata hooks. Two gaps need C: **lossy WebP encoding** (`image-webp` only encodes
  lossless) and, optionally, **libjpeg-turbo** for scaled decode and faster encoding.
- **C/C++**: put libjpeg-turbo, libspng (or libpng), libwebp, giflib, libtiff and libavif together
  yourself, or use Qt's image plugins (and KDE's kimageformats for AVIF/HEIF). BMP and ICO are
  small enough to parse by hand, or take them from stb_image or Qt.
- **Go is the weakest fit.** `image/jpeg` is about 3–5× slower than libjpeg-turbo. There's no WebP
  encoder, no ICO and no metadata passthrough. You'd end up calling the same C libraries through
  cgo, which gives up Go's main advantage.
- **The JPEG budget is achievable.** A 12 MP baseline JPEG decodes in about 50 ms on one core (both
  zune-jpeg and libjpeg-turbo). libjpeg-turbo's DCT-domain scaling gives a fit-to-window first frame
  in 22–30 ms.
- **WebP lossy decode is the slow one**: 210–285 ms for 12 MP. Moving to a WebP Sibling only meets
  the 50 ms budget if it was prefetched.
- **AVIF is cheap enough to include**: about 2 MB of libraries, BSD licences plus a royalty-free AV1
  patent licence, and 97–164 ms to decode 12 MP with dav1d.
- **Make HEIC optional, or leave it out of v1.** It costs about 2 MB of LGPL libraries and about
  900 ms to decode 12 MP with libde265, and the HEVC patent pools may want royalties (details below).

## Measured speed (this machine)

To get directly comparable numbers I ran a quick benchmark: Intel Core i5-12600KF, Linux, release
builds, best of 15 runs (best of 7 for AVIF and HEIC). Single-thread runs were pinned with
`taskset -c 2`. The test image is 4000×3000 (12 MP) RGB, a Lanczos upscale of a detailed
illustration. That is smoother than a camera photo, so expect real photos to decode somewhat slower.
JPEG is quality 90, 4:2:0. PNG is zlib level 6 (20 MB). WebP is lossy q90 (3.1 MB) or lossless
(15 MB). HEIC and AVIF are libheif quality 80. The libjpeg-turbo and libwebp figures go through
Pillow 12.1.1 (system libjpeg-turbo 2.1.5, libwebp 1.5.0), so they include a little Python overhead.
The harness (Rust crate, C and Python scripts) was throwaway and is not committed.

| Operation (12 MP) | Library | Time (ms) |
|---|---|---|
| JPEG baseline decode | zune-jpeg 0.5.15 (what `image` 0.25 uses) | **48.5** (1 core) |
| | libjpeg-turbo 2.1.5 | 51.5 |
| | jpeg-decoder 0.3 (old `image` backend) | 60.8 (1 core), 47.4 (multithreaded) |
| JPEG progressive decode | zune-jpeg / libjpeg-turbo / jpeg-decoder | 133 / 127 / 160 |
| JPEG decode, DCT-scaled to 2000 / 1000 / 500 px wide | libjpeg-turbo (via `draft()`) | 29.7 / 26.7 / 22.4 |
| PNG decode | `png` 0.18 (image-rs) | **93.0** |
| | zune-png 0.5 | 129.1 |
| | libpng 1.6.57 (simplified API, C) | 190.9 |
| WebP lossy decode | libwebp 1.5.0 | 212.6 |
| | image-webp 0.2 (pure Rust) | 285.9 |
| WebP lossless decode | image-webp / libwebp | 196.9 / 263.0 |
| AVIF decode to RGB | libavif 1.3.0 + dav1d 1.5.3 | 163.6 (1 thread), **96.6** (4 threads) |
| | libheif 1.21.2 + libaom | 376.5 |
| HEIC decode to RGB | libheif 1.21.2 + libde265 1.0.16 | **903.6** |
| JPEG q90 encode | libjpeg-turbo | **28.9** |
| | jpeg-encoder 0.7 (`image` 0.25's encoder, SIMD) | 112.0 |
| PNG encode | `png` 0.18 default / `Compression::Fast` | 1770 / **62.9** |
| WebP encode | libwebp lossy q90 (method 4) | 970 |
| | libwebp lossless (default) | 5422 |
| | image-webp lossless (fast, larger output) | 81.2 |

What this means:

- The 150 ms cold start budget leaves about 100 ms after a 50 ms full JPEG decode, for process start,
  the window and the first frame. Decoding scaled to the window (libjpeg-turbo `tjDecompress2` with
  a scaling factor, or `jpeg-decoder`'s `scale()`) buys back another 20–30 ms. The zune-jpeg README
  lists no scaled decode.
- The default PNG compression level is far too slow for Ctrl+S. Use a fast level, which costs about
  15% more bytes here.
- A lossy WebP save takes about 1 s at libwebp's default method 4. A lower `method` trades size for
  speed. Lossless WebP at the default effort takes several seconds, so don't make it the Ctrl+S
  default.
- The HEIC test image was encoded as a single image. iPhone HEICs are grids of 512×512 tiles, which
  decode differently, so take the HEIC figure as indicative only.

### Published numbers (for cross-checking)

- **libjpeg-turbo** is "2.1 - 6.0x as fast as libjpeg v6b" on x86 desktops, with SIMD for x86-64,
  x86, Armv8 and Armv7 (https://libjpeg-turbo.org/About/Performance).
- **Rust `png`**: in December 2024 the image-png and zune-png developers reported that the top Rust
  decoder ran 1.8× faster than libpng on x86 (Zen 4) and 1.5× faster on ARM
  (https://www.reddit.com/r/rust/comments/1ha7uyi/memorysafe_png_decoders_now_vastly_outperform_c/,
  reported at https://www.phoronix.com/news/Rust-PNG-Outperforms-C-PNG). My figure of about 2× over
  libpng agrees. Chromium has an experimental Skia codec built on the `png` crate
  (https://skia.googlesource.com/skia/+/refs/heads/chrome/m129/experimental/rust_png/).
- **zune-jpeg** aims to be "as fast as libjpeg-turbo" and publishes criterion comparisons against
  mozjpeg (a libjpeg-turbo fork) and jpeg-decoder (https://github.com/etemesi254/zune-image,
  https://etemesi254.github.io/assets/criterion/report/index.html).
- **Go `image/jpeg`**: about 30 ms of CPU against 7 ms for libjpeg on 1080p (Go issue,
  https://golang.org/issue/24499). go-libjpeg measured libjpeg-turbo at about 2.8× faster
  (https://github.com/codeskyblue/go-libjpeg).
- **libvips** (8.18, Aug 2025) is the fastest in its own whole-pipeline benchmark against ImageMagick,
  GraphicsMagick, OpenCV and Pillow-SIMD
  (https://github.com/libvips/libvips/wiki/Speed-and-memory-use). That benchmark measures a
  load-process-save pipeline, not a viewer's decode path.

## Per-format recommendations

| Format | Rust | C/C++ | Go | Notes |
|---|---|---|---|---|
| JPEG read | `image` 0.25 (zune-jpeg); `turbojpeg` crate for scaled decode | **libjpeg-turbo** (IJG + BSD-3 + zlib) | `image/jpeg` (slow), or libjpeg-turbo via cgo | libjpeg-turbo also does lossless rotate and crop (`tjTransform`) |
| JPEG write | `image` (jpeg-encoder, 112 ms) or `turbojpeg` 1.5.1 (29 ms) | libjpeg-turbo | `image/jpeg` | mozjpeg makes smaller files but encodes more slowly (not needed) |
| PNG read/write | **`png` 0.18** (in `image`) | libspng (BSD-2, 5.2 KSLOC vs libpng's 20 KSLOC) or libpng | `image/png` | Rust `png` decodes about 2× faster than libpng |
| WebP read | `image-webp` 0.2 (in `image`) or `webp`/`libwebp-sys` | **libwebp** + libwebpdemux (BSD-3) | `golang.org/x/image/webp` (decode only) | |
| WebP write | **`libwebp-sys` 0.14 / `webp` 0.3** (lossy). `image-webp` is lossless only | libwebp + libwebpmux | no pure-Go encoder; `gen2brain/webp` (libwebp in WASM) or cgo | |
| GIF read | `gif` 0.14 (in `image`) | giflib (MIT) or stb_image | `image/gif` | see Animation |
| BMP read | `image` | stb_image (public domain) or hand-written | `golang.org/x/image/bmp` | |
| TIFF read | `tiff` 0.11 (in `image`) | libtiff | `golang.org/x/image/tiff` | multi-page TIFF: show page 1 in v1 |
| ICO read | `image` (`ico` feature, uses `bmp` + `png`) or `ico` 0.6 | hand-written (directory + PNG/DIB), or Qt | no std or x/ support | pick the largest entry |
| AVIF read | `image` with `avif-native` (dav1d + mp4parse) or `libavif` crate | **libavif + dav1d** | cgo libavif, or `gen2brain/avif` | |
| HEIC read | `libheif-rs` 3.0 | libheif + libde265 | cgo libheif | optional, see costs |

Sources: `image` codec table (https://docs.rs/image/latest/image/codecs/index.html), `image` main
`Cargo.toml` (zune-jpeg 0.5.15, jpeg-encoder 0.7, png 0.18, gif 0.14, tiff 0.11.2, image-webp 0.2,
moxcms 0.8, ravif 0.13, optional dav1d 0.11;
https://github.com/image-rs/image/blob/main/Cargo.toml), crate versions from crates.io
(2026-10-29), libjpeg-turbo licences
(https://github.com/libjpeg-turbo/libjpeg-turbo/blob/main/LICENSE.md), libspng vs libpng
(https://libspng.org/comparison/), `golang.org/x/image` subpackages
(https://pkg.go.dev/golang.org/x/image), `gen2brain/webp` (https://pkg.go.dev/github.com/gen2brain/webp).

### C++ shortcuts

- **Qt** (`QImageReader`) reads BMP, GIF, JPEG and PNG natively. The Qt Image Formats module adds
  more, including TIFF and WebP. `setAutoTransform(true)` applies orientation metadata, and
  `read()` / `nextImageDelay()` / `loopCount()` step through animation frames
  (https://doc.qt.io/qt-6/qimagereader.html). KDE's **kimageformats** (LGPL) adds AVIF, HEIF and
  JXL through libavif and libheif (https://invent.kde.org/frameworks/kimageformats). QImageWriter
  doesn't carry arbitrary EXIF through, so you'd still need libexif or exiv2 to keep it.
- **libvips** (LGPL-2.1+) reads all the formats in scope, including HEIC and AVIF, and has Go
  (vipsgen, MIT) and C++ bindings. It's built for demand-driven pipelines and keeps metadata through
  a save. It is a heavy dependency (GLib and every codec) for a viewer that only needs a decoded
  frame (https://www.libvips.org/, https://github.com/cshum/vipsgen).

## Animation (GIF, WebP)

- **Rust `image` 0.25**: `GifDecoder`, `WebPDecoder` and `PngDecoder` (APNG) implement
  `AnimationDecoder::into_frames()`. Each frame is a **fully composited** RGBA canvas with its delay.
  The GIF code tracks disposal state (`non_disposed_frame`) and composites it in `src/codecs/gif.rs`.
  On `main`, which is heading for 1.0, this becomes `ImageReader::into_frames()`, so plan for a small
  API change (checked at commit `075e65d`).
- **libwebp**: `WebPAnimDecoder` returns a fully reconstructed canvas for each frame, in RGBA or
  BGRA, and `WebPAnimDecoderReset()` restarts the loop. Dispose and blend rules are handled inside it
  (https://developers.google.com/speed/webp/docs/container-api).
- **giflib** returns raw frames, so the app has to apply disposal itself. **Go `image/gif`**'s
  `DecodeAll` returns a paletted sub-image, a delay and a disposal value for each frame, so Go also
  composites by hand (https://pkg.go.dev/image/gif).
- **libvips / vipsgen** stacks the frames vertically, with `PageHeight()` and `PageDelay()`.
- For zimged: decode frames lazily, with a small ring buffer, rather than holding them all. At the
  first edit, keep the composited frame 1 as the Image. No ecosystem needs an animation encoder for
  v1, because a saved edited animation is a single frame.

## EXIF orientation and EXIF/ICC round-tripping

There are two separate jobs: carrying the **raw blobs** across, and **patching the orientation**.

- **Rust `image`**: `ImageDecoder` has `icc_profile()`, `exif_metadata()`, `xmp_metadata()` and
  `orientation()`, and `DynamicImage::apply_orientation()`
  (https://docs.rs/image/latest/image/trait.ImageDecoder.html). `ImageEncoder` has
  `set_icc_profile()` and `set_exif_metadata()`
  (https://docs.rs/image/latest/image/trait.ImageEncoder.html). On `main` (commit `075e65d`), the
  JPEG, PNG and WebP encoders support both, TIFF supports ICC and AVIF supports EXIF. The decoders
  for JPEG, PNG, WebP, TIFF and ICO expose EXIF, and the decoders for those plus GIF, BMP and AVIF
  expose ICC. So the round trip for the three write formats works without extra libraries.
- **Orientation on save**: when zimged bakes the rotation into the pixels, it has to set EXIF tag
  0x0112 to 1, or the saved file is rotated twice. That means editing the blob. In Rust,
  `little_exif` 0.6 can read and write it, and `kamadak-exif` 0.6 can only read it. Rewriting a
  single SHORT in the TIFF IFD in place is about 30 lines of code. `img-parts` 0.4 swaps
  JPEG/PNG/RIFF metadata segments without re-encoding. In C/C++, libexif (LGPL) or exiv2 (GPL-2+)
  can do the edit. In Go, `dsoprea/go-exif` can read and write it (not verified here).
- **libjpeg-turbo**: `jpeg_write_icc_profile` / `jpeg_read_icc_profile`. EXIF is an APP1 marker
  that you keep with `jpeg_save_markers` and write with `jpeg_write_marker`. Note the 64 KB marker
  limit: ICC profiles are split across several markers, and the Rust encoder rejects an EXIF chunk
  that is too large.
- **libwebp**: `WebPMuxGetChunk` / `WebPMuxSetChunk` with `"ICCP"` and `"EXIF"`, followed by
  `WebPMuxAssemble`. `WebPAnimDecoderGetDemuxer()` reads them from animated files
  (https://developers.google.com/speed/webp/docs/container-api).
- **libpng / libspng**: iCCP and eXIf chunks.
- **Go std**: the `image/*` decoders drop APP markers and chunks, so passthrough means splicing
  segments yourself.
- **ICC on screen** is still undecided in the map. `image` already depends on `moxcms` 0.9 (pure
  Rust). `lcms2` (Little CMS) is the C option.

## AVIF and HEIC costs

Sizes are the stripped Ubuntu x86-64 shared libraries on this machine. Statically linked builds
that include only decoders would be smaller.

| | AVIF (libavif 1.3.0 + dav1d 1.5.3) | HEIC (libheif 1.21.2 + libde265 1.0.16) |
|---|---|---|
| Binary size | dav1d 1.78 MB + libavif 0.24 MB (+ optional libyuv 0.64 MB for faster YUV to RGB) | libheif 1.65 MB + libde265 0.38 MB. Encoders, which we don't need, are much larger: x265 16.9 MB, libaom 5.5 MB |
| 12 MP decode | 164 ms (1 thread), 97 ms (4 threads) | 904 ms (single-image file) |
| Licence | libavif BSD (https://github.com/AOMediaCodec/libavif), dav1d BSD-2 with no CLA (https://github.com/videolan/dav1d) | libheif LGPL, examples MIT (https://github.com/strukturag/libheif), libde265 LGPL (https://github.com/strukturag/libde265), x265 GPL. LGPL is fine under the "GPL OK" rule, but static linking brings relinking obligations |
| Patents | The AOMedia Patent License 1.0 is "no-charge, royalty-free, irrevocable", with defensive termination if you sue over AV1 patents (https://aomedia.org/license/patent-license/) | HEVC is under pool licences. Access Advance covers decoders and encoders in products and "HEVC Software Sold separately" (https://accessadvance.com/?p=184). In 2016 HEVC Advance waived royalties for application-layer software downloaded to PCs and phones after the device's first sale (https://www.streamingmedia.com/Articles/News/Online-Video-News/HEVC-Advance-Makes-Some-Software-Royalty-Free-114938.aspx). The Via LA HEVC/VVC pool, now run by Access Advance, charges per unit with the first 100,000 units free, then $0.30 or $0.20, capped at $30M (https://www.via-la.com/licensing-programs/hevc-vvc/). Other patent holders sit outside both pools |
| Rust | `image` `avif-native` feature (dav1d + mp4parse); `ravif` 0.13 is an encoder | `libheif-rs` 3.0 (needs a system libheif) |

Recommendation: include **AVIF read** with dav1d. Its size, speed and licensing all qualify as
"cheap". Ship **HEIC read as optional**: a build feature, or loading libheif at runtime if the
system has it, which is how Linux distros already package it. That keeps zimged's own binaries free
of HEVC code and keeps the 900 ms decode out of the default path. Distributing an HEVC decoder in an
open-source app is a legal-risk call for the maintainer to make, not something this research settles.
(I haven't verified the alternative of using OS codecs on Windows and macOS, WIC and ImageIO.)

## Open points for the ticket this one blocks (#9)

- Whether to use libjpeg-turbo in a Rust stack: it makes encoding 4× faster, adds DCT-scaled decode
  for first paint and lossless JPEG rotation. The cost is a CMake/NASM build step.
- Whether WebP decode should be pure Rust or libwebp. libwebp is about 25% faster for lossy, but
  image-webp was faster for lossless in my test.
- The prefetch depth for WebP and AVIF Siblings, since a single decode takes longer than the 50 ms
  budget.
