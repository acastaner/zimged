# Rust with egui on glow, Linux-first

zimged is written in Rust, with eframe/egui for the window and UI, rendered through glow (OpenGL). In the [cold-start spike](https://github.com/acastaner/zimged/issues/9), this was the only stack that met the 150 ms double-click-to-first-image budget on Linux with the balanced power profile: p90 122 ms. Qt 6 Widgets reached p90 175 ms, all of it spent in Qt's own startup, and egui on wgpu reached about 250 ms because of GPU device setup. zimged is **Linux-first**: the performance budgets are defined and measured on Linux with the default (balanced) power profile. Windows and macOS builds are supported on a best-effort basis, with no budget.

Decided in [Lock the stack](https://github.com/acastaner/zimged/issues/10).

## Considered Options

- **C++ with Qt 6 Widgets.** Every OS integration is built in and mature (clipboard images, IME, trash, dialogs, native look). Rejected because its startup cost alone misses the budget, and fixing that would mean speculative static-linking and plugin trimming.
- **egui on wgpu.** Rejected for the first frame: GPU device setup alone takes about 220 ms.

## Consequences

- The UI uses egui's own widgets, not native ones. On GNOME Wayland, winit draws a client-side window frame.
- OS integration comes from separate crates: `arboard` for the clipboard, `rfd` for file dialogs, `trash`, `directories`, `interprocess` for single-instance, and `windows`/`objc2-foundation` for native filename ordering. Clipboard images on Wayland need a compositor with data-control (KWin, wlroots, recent GNOME); otherwise zimged falls back to XWayland.
- Decoding starts on a thread at `main()`, before the window exists, and decodes to display size, so decoding and OpenGL setup finish together.
- The Rust toolchain is pinned in `rust-toolchain.toml` (1.95, edition 2024).
