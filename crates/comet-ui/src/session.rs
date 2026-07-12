//! Terminal session abstraction — owns PTY, terminal state, and renderer.

use std::io::Read;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use comet_config::Config;
use comet_core::Terminal;
use comet_pty::{AnsiParser, PtyConfig, PtyProcess};
use comet_renderer::{BackendType, HasWindowHandle, Renderer, RendererConfig};

/// A single terminal session: PTY + terminal state + renderer.
pub struct TerminalSession {
    terminal: Terminal,
    pty: PtyProcess,
    renderer: Renderer,
    pty_rx: Receiver<Vec<u8>>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl TerminalSession {
    /// Spawns a new PTY and creates the terminal state.
    pub fn spawn(config: &Config) -> Self {
        let pty_config = PtyConfig {
            cols: 80,
            rows: 24,
            ..PtyConfig::default()
        };
        let pty = PtyProcess::spawn(pty_config).expect("Failed to spawn PTY");

        let bg_reader = pty
            .pair()
            .master
            .try_clone_reader()
            .expect("Failed to clone PTY reader");

        let scrollback_size = config.terminal.scrollback.max(100);
        let terminal = Terminal::with_scrollback(80, 24, scrollback_size);

        let (pty_tx, pty_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let mut reader = bg_reader;
            let mut buf = vec![0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        if pty_tx.send(data).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        eprintln!("PTY read error: {}", e);
                        break;
                    }
                }
            }
        });

        let renderer_config = SessionRendererConfig::from_config(config);
        let renderer = Renderer::new(renderer_config).expect("Failed to create renderer");

        Self {
            terminal,
            pty,
            renderer,
            pty_rx,
            thread_handle: Some(handle),
        }
    }

    /// Initializes the renderer with a window handle.
    pub fn init_renderer(
        &mut self,
        width: u32,
        height: u32,
        window_handle: Option<Box<dyn HasWindowHandle>>,
    ) {
        if let Err(e) = self.renderer.initialize(width, height, window_handle) {
            eprintln!("Failed to initialize renderer: {}", e);
        }
    }

    /// Processes all pending PTY output. Returns true if any data was processed.
    pub fn process_output(&mut self) -> bool {
        let mut any_data = false;
        while let Ok(data) = self.pty_rx.try_recv() {
            any_data = true;
            let mut parser = AnsiParser::new(&mut self.terminal);
            parser.feed(&data);
        }
        any_data
    }

    /// Writes bytes to the PTY (keyboard input).
    pub fn write_input(&mut self, data: &[u8]) {
        if let Err(e) = self.pty.writer().write_all(data) {
            eprintln!("PTY write error: {}", e);
        }
    }

    /// Resizes the PTY, terminal, and renderer.
    pub fn resize(&mut self, width: u32, height: u32) {
        let cell_size = self.renderer.metrics().cell_size();
        let cols = (width / cell_size.width.max(1)).max(1);
        let rows = (height / cell_size.height.max(1)).max(1);

        let pty_size = portable_pty::PtySize {
            rows: rows as u16,
            cols: cols as u16,
            pixel_width: width as u16,
            pixel_height: height as u16,
        };
        if let Err(e) = self.pty.resize(pty_size) {
            eprintln!("PTY resize error: {}", e);
        }

        self.terminal.resize(cols as usize, rows as usize);

        if let Err(e) = self.renderer.resize(width, height) {
            eprintln!("Renderer resize error: {}", e);
        }
    }

    /// Returns true if the background reader thread is still alive.
    pub fn is_alive(&self) -> bool {
        self.thread_handle.is_some()
    }

    pub fn terminal(&self) -> &Terminal {
        &self.terminal
    }

    pub fn terminal_mut(&mut self) -> &mut Terminal {
        &mut self.terminal
    }

    pub fn renderer(&self) -> &Renderer {
        &self.renderer
    }

    pub fn renderer_mut(&mut self) -> &mut Renderer {
        &mut self.renderer
    }

    pub fn pty_mut(&mut self) -> &mut PtyProcess {
        &mut self.pty
    }

    /// Replaces the renderer (used during config hot-reload).
    pub fn set_renderer(&mut self, renderer: Renderer) {
        self.renderer = renderer;
    }

    /// Renders the terminal state through this session's renderer.
    pub fn render(&mut self) -> comet_renderer::error::RendererResult<()> {
        self.renderer.render(&self.terminal)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.pty.kill();
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
        let _ = self.pty.wait();
    }
}

// ---- Internal helper ----

struct SessionRendererConfig;

impl SessionRendererConfig {
    fn from_config(config: &Config) -> RendererConfig {
        RendererConfig {
            backend: BackendType::Wgpu,
            font_family: config.font.family.clone(),
            font_size: config.font.size,
            theme: "dark".to_string(),
            dpi_scale: 1.0,
            padding_x: 2.0,
            padding_y: 2.0,
            cursor_blink: config.cursor.blink,
            cursor_shape: match config.cursor.style.as_str() {
                "beam" => comet_renderer::cursor::CursorShape::Beam,
                "underline" => comet_renderer::cursor::CursorShape::Underline,
                "hollow_block" => comet_renderer::cursor::CursorShape::HollowBlock,
                "bar" => comet_renderer::cursor::CursorShape::Bar,
                _ => comet_renderer::cursor::CursorShape::Block,
            },
            colors: Some(comet_renderer::CustomColors {
                background: config.colors.background.clone(),
                foreground: config.colors.foreground.clone(),
                cursor: config.colors.cursor.clone(),
                selection: config.colors.selection.clone(),
            }),
        }
    }
}
