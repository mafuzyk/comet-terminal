
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
├── comet-config       # Configuration loading, validation, themes, session save/restore
├── comet-ui           # Windowing, input, session management, clipboard
└── comet              # Main application binary
```

### `comet-core`

Pure terminal engine with zero I/O dependencies. Owns the **state** of a terminal session:

- **Grid** — cell matrix (character + colors + attributes per cell)
- **Cursor** — position and visibility
- **Color** — named, indexed (256), and true-color (24-bit) support
- **Pen** — foreground/background color and text attributes for subsequent writes
- **Scrollback** — configurable history with viewport scrolling
- **Selection** — word/line/arbitrary range selection with clipboard integration
- **Hyperlinks** — OSC 8 hyperlink tracking per cell
- **Bell** — BEL character detection

The crate is usable entirely on its own — no GPU, no PTY, no window system required.

### `comet-pty`

Connects a real shell process to `comet-core::Terminal`:

- Spawns and manages a PTY (pseudo-terminal) via `portable-pty`
- Parses ANSI escape sequences with `vte`
- Applies decoded mutations directly to the terminal grid, cursor, and pen state
- Supports OSC 8 hyperlinks, OSC 52 clipboard, and BEL notifications
- Configurable resize handling

### `comet-renderer`

Bidirectional rendering layer — two backends sharing the same interface:

| Backend | Tech | Status |
|---------|------|--------|
| **WGPU** | `wgpu` GPU API (Vulkan/Metal/DX12) | ✅ Working |
| **CPU** | Software fallback | ✅ Working |

**GPU pipeline:**
1. Font rasterization via `cosmic-text` + `swash`
2. Glyph atlas with shelf-packing (cached on GPU)
3. Damage-tracked rendering with persistent vertex buffers
4. WGSL shaders: vertex transform + fragment alpha-blend
5. Single draw call batches all visible glyphs
6. LRU glyph cache with auto-resize for long-running sessions

**Architecture highlights:**
- `GraphicsContext` owns the window handle alongside GPU resources — guaranteed correct drop order (surface before handle)
- Window abstraction via `HasWindowHandle` trait (no direct winit dependency)
- `RenderBackend` trait allows backend switching at initialization
- Surface lifecycle handles `Timeout`, `Outdated`, and `Lost` errors
- GlyphCache proper LRU eviction and configurable max entries

### `comet-ui`

Integration layer connecting all components:

- **TerminalSession** — owns PTY + terminal state + renderer per session
- **TerminalManager** — multi-session lifecycle (prepares for tabs/panes)
- **Input handling** — keyboard, mouse, scrollback, clipboard shortcuts
- **Cursor rendering** — 5 shapes with configurable blink interval
- **Config hot-reload** — applies font/color/cursor changes at runtime
- **Session save/restore** — remembers window geometry, font, and theme

### `comet-config`

Configuration system with:

- TOML-based config file with validation
- Theme system with 16-color ANSI palettes
- Built-in themes: **mochi-galaxy** (default), **mochi-dark**, **catppuccin-mocha**, **tokyo-night**
- `BehaviorConfig`, `AppearanceConfig`, `ShortcutsConfig` for UX customization
- Session save/restore (window position, font, theme)
- File watcher for config hot-reload

---

## Installation

### Any Linux distro (generic)

Install the required system dependencies, then compile from source:

| Distro | Dependencies |
|--------|-------------|
| Debian/Ubuntu | `sudo apt install build-essential pkg-config libxkbcommon-dev libwayland-dev libfontconfig-dev libfreetype-dev` |
| Fedora | `sudo dnf install gcc pkg-config libxkbcommon-devel wayland-devel fontconfig-devel freetype-devel` |
| Arch Linux | `sudo pacman -S base-devel pkg-config libxkbcommon wayland fontconfig freetype2` |
| openSUSE | `sudo zypper install gcc pkg-config libxkbcommon-devel wayland-devel fontconfig-devel freetype2-devel` |
| Void Linux | `sudo xbps-install base-devel pkg-config libxkbcommon-devel wayland-devel fontconfig-devel freetype-devel` |

After installing dependencies, build with Cargo:

```bash
git clone https://github.com/mafuzyk/comet-terminal
cd comet-terminal
cargo build --release
cargo run --release
```

### Manual compilation (any distro)

Comet is a standard Rust workspace project. No special build system is needed beyond Rust and the system libraries above.

```bash
# 1. Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. Install system dependencies (see table above for your distro)

# 3. Clone and build
git clone https://github.com/mafuzyk/comet-terminal
cd comet-terminal
cargo build --release

# 4. Run
./target/release/comet

# (optional) Install to ~/.cargo/bin
cargo install --path crates/comet

# (optional) Add desktop entry for launchers (KDE, GNOME, etc.)
cp assets/comet.desktop ~/.local/share/applications/
update-desktop-database ~/.local/share/applications/
```

### Nix / Flakes

Comet provides a `flake.nix` for Nix users. To build and run:

```bash
# Build the package
nix build github:mafuzyk/comet-terminal

# Run directly
nix run github:mafuzyk/comet-terminal

# Or clone and use the dev shell
git clone https://github.com/mafuzyk/comet-terminal
cd comet-terminal
nix develop
cargo build --release
cargo run --release
```

The flake also exposes `.#devShells.default` with Rust toolchain, `rustfmt`, `clippy`, and `rust-analyzer` pre-configured for development.

### Run tests

```bash
cargo test --workspace
```

---

## Project status

```
comet-core      ████████████████████ 100% — grid, cursor, colors, scrollback, selection, hyperlinks
comet-pty       ██████████████████░░  90% — PTY + parser, OSC 8/52, bell, resize
comet-renderer  ██████████████████░░  90% — WGPU + CPU, damage tracking, LRU cache, persistent buffers
comet-config    ████████████████████ 100% — config, themes, hot-reload, session save/restore
comet-ui        ████████████████████░  90% — window, tabs, panes, session management, clipboard
comet           ████████████████████░  90% — application binary, icon, config, session
```

**87 tests** across all crates.

### Implemented

- Terminal state (grid, cursor, pen, colors, scrollback, selection)
- ANSI escape parsing (VT100/xterm subset via `vte`)
- PTY process management with `portable-pty`
- GPU-accelerated glyph rendering via WGPU (Vulkan/Metal/DX12)
- CPU software rendering fallback
- Font rasterization with `cosmic-text` + `swash`
- Glyph atlas with shelf-packing and GPU caching
- Single-pass batching (all visible glyphs in one draw call)
- Damage-tracked rendering (cell-by-cell updates)
- Persistent GPU vertex buffers (reduces allocation per frame)
- LRU glyph cache with auto-resize and configurable max entries
- Cursor rendering (block, beam, underline, hollow block, bar) with blinking
- 5 cursor shapes with configurable blink interval
- Selection with word/line expansion, copy-on-select, middle-click paste
- Scrollback viewport navigation (PageUp/Down, Home/End, mouse wheel)
- Mouse support (click, drag, selection, wheel)
- Configuration system with TOML + theme support
- Built-in themes: **mochi-galaxy** (default), **mochi-dark**, **catppuccin-mocha**, **tokyo-night**
- OSC 8 hyperlink tracking
- OSC 52 clipboard support
- BEL (bell) visual feedback
- Config hot-reload at runtime
- Session save/restore (window geometry, font, theme)
- `TerminalSession` / `TerminalManager` abstractions for tabs and panes
- Tab bar with visual tabs, close buttons (×), and title truncation
- Split pane support (horizontal/vertical) with keyboard shortcuts
- Pane focus navigation (Ctrl+Alt+Arrow) and focus indicator border
- Split divider rendering with drag-to-resize handles
- Overlay render pass for UI chrome (tab bar, dividers, focus indicators)
- Search in scrollback (Ctrl+Shift+F)
- Ctrl+Click hyperlink opening
- Customizable keybindings (14 mappable shortcuts)
- Application icon placeholder (ready for Mochi mascot)
- Cross-platform config path resolution

### In progress / planned

- Search match highlighting in terminal content
- Mochi mascot and final visual identity
- Desktop notifications for bell
- Multi-PTY session isolation
- Configurable tab positioning (top/bottom)
- Unlimited split pane nesting

---

## Configuration

Comet reads configuration from:

| Platform | Path |
|----------|------|
| Linux | `~/.config/atlas/comet/config.toml` |
| macOS | `~/Library/Application Support/atlas/comet/config.toml` |
| Windows | `C:\Users\<user>\AppData\Roaming\atlas\comet\config.toml` |

Themes are loaded from `themes/` in the same directory (e.g., `~/.config/atlas/comet/themes/`).

All Atlas ecosystem tools share the `~/.config/atlas/` base directory:
- **Comet**: `~/.config/atlas/comet/`
- **AtlasWM**: `~/.config/atlas/atlaswm/`
- **AtlasFetch**: `~/.config/atlas/atlasfetch/`

This makes it easy to migrate or share configuration across the Atlas suite.

### Example `config.toml`

```toml
[theme]
name = "mochi-galaxy"  # mochi-galaxy, mochi-dark, catppuccin-mocha, tokyo-night

[font]
family = "JetBrains Mono"
size = 14
ligatures = true

[colors]
background = "#0B1020"
foreground = "#D8E4FF"
cursor = "#8BE9FD"
selection = "#263B66"

[cursor]
style = "block"       # block, beam, underline, hollow_block, bar
blink = true
blink_interval = 500

[appearance]
opacity = 0.95
blur = true

[behavior]
copy_on_select = true
confirm_close = false

[terminal]
scrollback = 10000
bell = true
middle_click_paste = true

[shortcuts]
search = "CTRL_SHIFT_F"

[renderer]
vsync = true
```

### Theme system

Themes are complete 16-color ANSI palettes + base colors. Priority:

1. **Explicit config** — values in `[colors]` override theme
2. **Selected theme** — `[theme].name` provides full palette
3. **Defaults** — Mochi Galaxy fallback

Built-in themes (auto-created on first run):

| Theme | Preview |
|-------|---------|
| **mochi-galaxy** (default) | Deep space blue-black, starlight text, cyan cursor |
| mochi-dark | Deep purple base, warm pink accents |
| catppuccin-mocha | Popular community theme |
| tokyo-night | VS Code favorite, blue accent |

Add custom themes by placing `*.toml` files in `~/.config/comet/themes/`.

---

## Technical stack

| Component | Crate |
|-----------|-------|
| GPU rendering | [`wgpu`](https://github.com/gfx-rs/wgpu) 23 |
| Font layout | [`cosmic-text`](https://github.com/pop-os/cosmic-text) 0.10 |
| Glyph rasterization | [`swash`](https://github.com/dfrg/swash) 0.2 |
| ANSI parser | [`vte`](https://github.com/withoutboats/vte) |
| PTY | [`portable-pty`](https://github.com/wez/wezterm/tree/master/pty) |
| Config serialization | [`toml`](https://github.com/toml-rs/toml) + [`serde`](https://github.com/serde-rs/serde) |
| Window system | [`winit`](https://github.com/rust-windowing/winit) 0.30 |
| Color handling | [`bytemuck`](https://github.com/Lokathor/bytemuck) for GPU upload |

---

## Related projects

- [**AtlasWM**](https://github.com/mafuzyk/atlaswm) — a modern Wayland compositor built in Rust
- [**AtlasFetch**](https://github.com/mafuzyk/atlasfetch) — a fast, minimal system information fetcher

---

## License

MIT — see [LICENSE](LICENSE).

---

<sub>AI (Claude) was used for code review, refactoring suggestions, and translation assistance during development.</sub>
