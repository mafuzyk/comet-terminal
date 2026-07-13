# Comet Terminal

⚠️ **This terminal is NOT functional.** Keyboard input does not produce visible output and the tab bar has rendering issues. Use at your own risk.

A lightweight, tabbed terminal emulator built on [`alacritty_terminal`](https://github.com/alacritty/alacritty) with WGPU rendering.

## Features (planned)

- GPU-accelerated rendering via WGPU (Vulkan/Metal/DX12)
- Tabbed interface (Windows Terminal style)
- True color, bold/dim/italic/underline
- Kitty keyboard protocol support
- TOML configuration

## Installation

### From source

```sh
git clone https://github.com/mafuzyk/comet-terminal
cd comet-terminal
cargo build --release
cp target/release/comet ~/.local/bin/
```

Requires Rust 1.97+, a GPU with Vulkan/Metal/DX12 support, and the usual C build tools (`gcc`, `pkg-config`, `libfontconfig`).

### Dependencies (Linux)

```sh
# Debian/Ubuntu
sudo apt install gcc pkg-config libfontconfig-dev

# Fedora
sudo dnf install gcc pkgconfig fontconfig-devel

# Arch
sudo pacman -S gcc pkgconf fontconfig
```

## Configuration

Place a `comet.toml` at `~/.config/comet/comet.toml`. All fields are optional:

```toml
font_family = "monospace"
font_size = 12.0

[colors]
background = "#1a1b1e"
foreground = "#cdd6f4"
tab_bar = "#11111b"
tab_active = "#313244"
tab_inactive = "#1e1e2e"

[window]
tab_height = 32.0
```

## Keyboard Shortcuts

| Shortcut           | Action          |
|--------------------|-----------------|
| `Ctrl+Shift+T`     | New tab         |
| `Ctrl+Shift+W`     | Close tab       |
| `Ctrl+Tab`         | Next tab        |
| `Ctrl+Shift+Tab`   | Previous tab    |

## License

Apache 2.0 / MIT
