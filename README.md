
<p align="center">
  <img src="assets/mochi/logo.svg" width="140" alt="Mochi">
</p>

<h1 align="center">☄️ Comet Terminal</h1>

<p align="center">
  <strong>A modern, GPU-accelerated terminal emulator built in Rust.</strong>
</p>

<p align="center">
  <a href="https://github.com/mafuzyk/comet-terminal/actions">
    <img src="https://img.shields.io/github/actions/workflow/status/mafuzyk/comet-terminal/ci.yml?style=flat-square" alt="CI">
  </a>
  <a href="https://www.rust-lang.org">
    <img src="https://img.shields.io/badge/rust-stable-orange?style=flat-square&logo=rust" alt="Rust">
  </a>
  <a href="https://github.com/mafuzyk/comet-terminal/blob/main/LICENSE">
    <img src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" alt="License">
  </a>
</p>

<p align="center">
  <i>Explore your system. One command at a time.</i>
</p>

---

## Architecture

Comet is decomposed into independent crates, each with a clear responsibility:

```
comet-terminal/
├── comet-core         # Terminal state: grid, cursor, colors — pure data, no I/O
├── comet-pty          # PTY process + ANSI escape parser
├── comet-renderer     # GPU/CPU rendering with WGPU backend
├── comet-ui           # Windowing, input, and UI integration (WIP)
└── comet              # Main application binary
```

### `comet-core`

Pure terminal engine with zero I/O dependencies. Owns the **state** of a terminal session:

- **Grid** — cell matrix (character + colors + attributes per cell)
- **Cursor** — position and visibility
- **Color** — named, indexed (256), and true-color (24-bit) support
- **Pen** — foreground/background color and text attributes for subsequent writes

The crate is usable entirely on its own — no GPU, no PTY, no window system required.

### `comet-pty`

Connects a real shell process to `comet-core::Terminal`:

- Spawns and manages a PTY (pseudo-terminal) via `portable-pty`
- Parses ANSI escape sequences with `vte`
- Applies decoded mutations directly to the terminal grid, cursor, and pen state

### `comet-renderer`

Bidirectional rendering layer — two backends sharing the same interface:

| Backend | Tech | Status |
|---------|------|--------|
| **WGPU** | `wgpu` GPU API (Vulkan/Metal/DX12) | ✅ Working |
| **CPU** | Software fallback | ✅ Working |

**GPU pipeline:**
1. Font rasterization via `cosmic-text` + `swash`
2. Glyph atlas with shelf-packing (cached on GPU)
3. Per-frame vertex buffer (screen-space quads with UV coords)
4. WGSL shaders: vertex transform + fragment alpha-blend
5. Single draw call batches all visible glyphs

**Architecture highlights:**
- `GraphicsContext` owns the window handle alongside GPU resources — guaranteed correct drop order (surface before handle)
- Window abstraction via `HasWindowHandle` trait (no direct winit dependency)
- `RenderBackend` trait allows backend switching at initialization
- Surface lifecycle handles `Timeout`, `Outdated`, and `Lost` errors

---

## Build

### Requirements

- **Rust** stable toolchain (see `rust-toolchain.toml`)
- **Linux** with Wayland (primary target; WGPU also supports X11, macOS, Windows)

### Build and run

```bash
git clone https://github.com/mafuzyk/comet-terminal
cd comet-terminal

cargo build --release
cargo run --release
```

For the renderer demo standalone:

```bash
cargo run --release -p comet-renderer-demo
```

### Run tests

```bash
cargo test --workspace
```

---

## Project status

```
comet-core   ████████████████░░░░  80%  — grid, cursor, colors, pen done
comet-pty    ██████████░░░░░░░░░░  50%  — PTY + parser done, resize WIP
comet-render ██████████████░░░░░░  70%  — WGPU + CPU backends working, cursor rendering TBD
comet-ui     ░░░░░░░░░░░░░░░░░░░░   0%  — planned
comet        ░░░░░░░░░░░░░░░░░░░░   0%  — planned
```

### Implemented

- Terminal state (grid, cursor, pen, colors)
- ANSI escape parsing (VT100/xterm subset via `vte`)
- PTY process management
- GPU-accelerated glyph rendering via WGPU (Vulkan/Metal/DX12)
- CPU software rendering fallback
- Font rasterization with `cosmic-text` + `swash`
- Glyph atlas with shelf-packing and GPU caching
- Single-pass batching (all visible glyphs in one draw call)

### In progress / planned

- Cursor rendering (block, beam, underline, blink)
- Selection / background color per cell
- Performance: persistent vertex buffer, bind group caching
- Window integration (`comet-ui`)
- Theme system
- Configuration files
- Tabs and split panes

---

## Technical stack

| Component | Crate |
|-----------|-------|
| GPU rendering | [`wgpu`](https://github.com/gfx-rs/wgpu) 23 |
| Font layout | [`cosmic-text`](https://github.com/pop-os/cosmic-text) 0.10 |
| Glyph rasterization | [`swash`](https://github.com/dfrg/swash) 0.2 |
| ANSI parser | [`vte`](https://github.com/withoutboats/vte) |
| PTY | [`portable-pty`](https://github.com/wez/wezterm/tree/master/pty) |
| Async executor | [`pollster`](https://github.com/zesterer/pollster) (blocking) |
| Color handling | [`bytemuck`](https://github.com/Lokathor/bytemuck) for GPU upload |

---

## License

MIT &mdash; see [LICENSE](LICENSE).
