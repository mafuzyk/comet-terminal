# ☄️ Comet Terminal

<p align="center">
  <img src="assets/mochi/logo.svg" width="160" alt="Mochi">
</p>

<h3 align="center">
A modern, fast and highly customizable terminal emulator built for the Atlas ecosystem.
</h3>

<p align="center">
  <i>Explore your system. One command at a time.</i>
</p>

---

## 🦝 Mochi

Meet **Mochi**, the little explorer of the Atlas ecosystem.

A curious astronaut raccoon traveling through systems, galaxies and code.

Mochi represents the philosophy behind Atlas:

* exploration;
* customization;
* freedom;
* curiosity.

---

## ✨ Features

🚧 **Currently under development**

Planned features:

* ⚡ GPU accelerated rendering
* 🌌 Native Wayland support
* 🦀 Built with Rust
* 🎨 Deep customization system
* 🪟 Tabs and split views
* 🔌 Modular architecture
* 🎭 Custom themes and animations
* 🌐 Atlas ecosystem integration

---

## 🏗️ Architecture

Comet is designed with modularity as a core principle.

Instead of a monolithic terminal, Comet is divided into independent components:

```
Comet Terminal

├── comet-core
│   └── Terminal engine
│
├── comet-renderer
│   └── GPU rendering layer
│
├── comet-ui
│   └── Interface components
│
└── comet
    └── Main application
```

Each part can evolve independently while keeping the whole experience integrated.

---

## 🚀 Philosophy

Comet follows the same philosophy as the Atlas ecosystem:

> Powerful by default. Flexible by design.

The terminal should not force a workflow.

Users should be able to customize:

* appearance;
* behavior;
* shortcuts;
* integrations;
* workflows.

---

## 🦀 Technology

Built using:

| Technology | Purpose             |
| ---------- | ------------------- |
| Rust       | Core language       |
| Wayland    | Display protocol    |
| wgpu       | GPU rendering       |
| PTY        | Shell communication |

---

## 📦 Installation

Currently, Comet is in early development.

Build from source:

```bash
git clone https://github.com/mafuzyk/comet-terminal
cd comet-terminal

cargo run --release
```

---

## 🛣️ Roadmap

### Phase 1 — Foundation

* [x] Project structure
* [ ] PTY integration
* [ ] Basic terminal rendering
* [ ] ANSI support

### Phase 2 — Experience

* [ ] GPU rendering
* [ ] Themes
* [ ] Configuration system
* [ ] Custom fonts

### Phase 3 — Atlas Integration

* [ ] Mochi Shell integration
* [ ] AtlasWM features
* [ ] Shared Atlas configuration

---

## 🤝 Contributing

Contributions, ideas and discussions are welcome.

See:

* `CONTRIBUTING.md`

---

## 📜 License

See `LICENSE`.
