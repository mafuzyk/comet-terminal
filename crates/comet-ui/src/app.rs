use std::io::Read;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread;

use comet_config::Config;
use comet_core::Terminal;
use comet_pty::{AnsiParser, PtyConfig, PtyProcess};
use comet_renderer::{BackendType, HasWindowHandle, Renderer, RendererConfig};
use raw_window_handle::{DisplayHandle, HandleError, HasDisplayHandle};
use winit::{
    event::{Modifiers, WindowEvent},
    raw_window_handle::HasWindowHandle as RawHasWindowHandle,
    window::Window,
};

use crate::input::key_event_to_ansi;

/// Wraps an `Arc<Window>` so the renderer can create a GPU surface from it.
struct WindowOwner(Arc<Window>);

unsafe impl Send for WindowOwner {}
unsafe impl Sync for WindowOwner {}

impl RawHasWindowHandle for WindowOwner {
    fn window_handle(
        &self,
    ) -> Result<winit::raw_window_handle::WindowHandle<'_>, HandleError> {
        self.0.window_handle()
    }
}

impl HasDisplayHandle for WindowOwner {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.0.display_handle()
    }
}

// Blanket impl from comet-renderer covers WindowOwner

/// Terminal application state — owns the PTY, terminal state, renderer, and window.
pub struct TerminalApp {
    terminal: Terminal,
    pty: PtyProcess,
    renderer: Renderer,
    window: Arc<Window>,
    pty_rx: Receiver<Vec<u8>>,
    thread_handle: Option<thread::JoinHandle<()>>,
    needs_redraw: bool,
    modifiers: Modifiers,
}

impl TerminalApp {
    /// Creates a new terminal application.
    pub fn new(window: Arc<Window>, config: Config) -> Self {
        let window_size = window.inner_size();

        // Spawn PTY
        let pty_config = PtyConfig {
            cols: 80,
            rows: 24,
            ..PtyConfig::default()
        };
        let pty = PtyProcess::spawn(pty_config).expect("Failed to spawn PTY");

        // Clone a reader for the background thread
        let bg_reader = pty
            .pair()
            .master
            .try_clone_reader()
            .expect("Failed to clone PTY reader");

        // Create terminal core
        let terminal = Terminal::new(80, 24);

        // Background thread: reads PTY output, sends raw bytes through channel
        let (pty_tx, pty_rx) = mpsc::channel();
        // Clone window Arc for background thread to wake up the event loop
        let wakeup_window = window.clone();
        let handle = thread::spawn(move || {
            let mut reader = bg_reader;
            let mut buf = vec![0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        if pty_tx.send(data).is_err() {
                            break;
                        }
                        // Wake up the event loop so PTY output is consumed and
                        // rendered without blocking on window events.
                        wakeup_window.request_redraw();
                    }
                    Err(e) => {
                        eprintln!("PTY read error: {}", e);
                        break;
                    }
                }
            }
        });

        // Create renderer from config
        let renderer_config = RendererConfig {
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
        };
        let mut renderer =
            Renderer::new(renderer_config).expect("Failed to create renderer");
        renderer
            .initialize(
                window_size.width,
                window_size.height,
                Some(Box::new(WindowOwner(window.clone())) as Box<dyn HasWindowHandle>),
            )
            .expect("Failed to initialize renderer");

        Self {
            terminal,
            pty,
            renderer,
            window,
            pty_rx,
            thread_handle: Some(handle),
            needs_redraw: true,
            modifiers: Modifiers::default(),
        }
    }

    /// Handles a winit window event.
    pub fn handle_window_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::Resized(size) => {
                self.resize(size.width, size.height);
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = *modifiers;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                self.handle_key(event);
            }
            WindowEvent::RedrawRequested => {
                // Always consume pending PTY data before rendering so the
                // frame reflects the latest terminal state.
                self.process_pty_output();
                self.render();
                // Clear flag — any new data that arrived between
                // process_pty_output and render will be picked up by
                // about_to_wait and trigger another redraw.
                self.needs_redraw = false;
            }
            _ => {}
        }
    }

    /// Processes pending PTY output.
    /// Should be called each iteration before deciding whether to redraw.
    pub fn process_pty_output(&mut self) {
        let mut any_data = false;
        while let Ok(data) = self.pty_rx.try_recv() {
            any_data = true;
            let mut parser = AnsiParser::new(&mut self.terminal);
            parser.feed(&data);
        }
        if any_data {
            self.needs_redraw = true;
        }
    }

    /// Returns whether a redraw has been requested.
    pub fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    /// Requests a redraw on the window and resets the flag.
    pub fn request_redraw(&mut self) {
        self.needs_redraw = false;
        self.window.request_redraw();
    }

    /// Renders the current terminal state.
    fn render(&mut self) {
        if let Err(e) = self.renderer.render(&self.terminal) {
            eprintln!("Render error: {}", e);
        }
    }

    /// Resizes the terminal, PTY, and renderer.
    fn resize(&mut self, width: u32, height: u32) {
        let cell_size = self.renderer.metrics().cell_size();
        let cols = (width / cell_size.width.max(1)).max(1);
        let rows = (height / cell_size.height.max(1)).max(1);

        // Resize PTY
        let pty_size = portable_pty::PtySize {
            rows: rows as u16,
            cols: cols as u16,
            pixel_width: width as u16,
            pixel_height: height as u16,
        };
        if let Err(e) = self.pty.resize(pty_size) {
            eprintln!("PTY resize error: {}", e);
        }

        // Resize terminal core
        self.terminal.resize(cols as usize, rows as usize);

        // Resize renderer
        if let Err(e) = self.renderer.resize(width, height) {
            eprintln!("Renderer resize error: {}", e);
        }

        self.needs_redraw = true;
    }

    /// Handles a keyboard event.
    fn handle_key(&mut self, event: &winit::event::KeyEvent) {
        let mods = self.modifiers.state();
        if let Some(bytes) = key_event_to_ansi(event, mods.control_key(), mods.alt_key()) {
            if let Err(e) = self.pty.writer().write_all(&bytes) {
                eprintln!("PTY write error: {}", e);
            }
        }
    }

    /// Returns a reference to the window.
    pub fn window(&self) -> &Arc<Window> {
        &self.window
    }

    /// Returns a mutable reference to the PTY for external access (e.g., waiting).
    pub fn pty_mut(&mut self) -> &mut PtyProcess {
        &mut self.pty
    }
}

impl Drop for TerminalApp {
    fn drop(&mut self) {
        // Kill the child process first. This closes the slave side of the
        // PTY, causing the background reader thread to get EOF.
        let _ = self.pty.kill();
        // Join the background thread so the cloned reader is dropped before
        // the PtyProcess (which owns the original PtyMaster).
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
        // Reap the child process to prevent zombies.
        let _ = self.pty.wait();
    }
}
