# Comet Terminal

A lightweight, tabbed terminal emulator built on [`alacritty_terminal`](https://github.com/alacritty/alacritty) with WGPU rendering.

## Features

- GPU-accelerated rendering via WGPU (Vulkan/Metal/DX12)
- Tabbed interface (Windows Terminal style)
- True color, bold/dim/italic/underline
- Kitty keyboard protocol support
- TOML configuration

## Building

```sh
cargo build --release
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
