use std::sync::Arc;
use std::time::{Duration, Instant};

use comet_config::{Config, ConfigWatcher, Session};
use comet_renderer::{HasWindowHandle, Renderer, RendererConfig};
use raw_window_handle::{DisplayHandle, HandleError, HasDisplayHandle};
use winit::{
    event::{ElementState, Modifiers, MouseButton, MouseScrollDelta, WindowEvent},
    keyboard::{KeyCode, PhysicalKey},
    raw_window_handle::HasWindowHandle as RawHasWindowHandle,
    window::Window,
};

use crate::input::key_event_to_ansi;
use crate::manager::TerminalManager;

const DOUBLE_CLICK_INTERVAL: Duration = Duration::from_millis(400);

/// Wraps an `Arc<Window>` so the renderer can create a GPU surface.
struct WindowOwner(Arc<Window>);

unsafe impl Send for WindowOwner {}
unsafe impl Sync for WindowOwner {}

impl RawHasWindowHandle for WindowOwner {
    fn window_handle(&self) -> Result<winit::raw_window_handle::WindowHandle<'_>, HandleError> {
        self.0.window_handle()
    }
}

impl HasDisplayHandle for WindowOwner {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.0.display_handle()
    }
}

/// Terminal application state — owns sessions, window, and top-level event handling.
pub struct TerminalApp {
    manager: TerminalManager,
    window: Arc<Window>,
    needs_redraw: bool,
    modifiers: Modifiers,
    config: Config,
    config_watcher: ConfigWatcher,
    session: Session,
    is_selecting: bool,
    clipboard: arboard::Clipboard,
    click_count: u32,
    last_click_time: Instant,
    last_click_pos: (f64, f64),
    mouse_pos: (f64, f64),
}

impl TerminalApp {
    /// Creates a new terminal application.
    pub fn new(window: Arc<Window>, config: Config, session: Session) -> Self {
        let window_size = window.inner_size();

        let manager = TerminalManager::new(&config);

        let mut app = Self {
            manager,
            window,
            needs_redraw: true,
            modifiers: Modifiers::default(),
            config,
            config_watcher: ConfigWatcher::new().unwrap_or_else(|_| ConfigWatcher::dummy()),
            session,
            is_selecting: false,
            clipboard: arboard::Clipboard::new().expect("Failed to initialize clipboard"),
            click_count: 0,
            last_click_time: Instant::now(),
            last_click_pos: (0.0, 0.0),
            mouse_pos: (0.0, 0.0),
        };

        // Initialize the active session's renderer
        let active = app.manager.active_mut();
        active.init_renderer(
            window_size.width,
            window_size.height,
            Some(Box::new(WindowOwner(app.window.clone())) as Box<dyn HasWindowHandle>),
        );
        active
            .renderer_mut()
            .set_show_diagnostics(app.config.debug.show_fps);

        app
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
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_pos = (position.x, position.y);
                self.handle_mouse_move(position.x, position.y);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let (x, y) = self.mouse_pos;
                self.handle_mouse_button(*state, *button, x, y);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.handle_mouse_wheel(delta);
            }
            WindowEvent::RedrawRequested => {
                self.process_pty_output();
                self.render();
                self.needs_redraw = false;
            }
            _ => {}
        }
    }

    /// Processes pending PTY output and checks for config changes.
    pub fn process_pty_output(&mut self) {
        let had_data = self.manager.active_mut().process_output();
        if had_data {
            self.needs_redraw = true;
            self.window.request_redraw();
        }

        // Check for config file changes
        if self.config_watcher.check() {
            self.reload_config();
        }
    }

    /// Reload configuration from disk and apply changes.
    fn reload_config(&mut self) {
        let Ok(new_config) = comet_config::load_config() else {
            return;
        };

        let old = &self.config;
        let new = &new_config;

        // Apply font/color/cursor changes
        if old.font != new.font || old.colors != new.colors || old.cursor != new.cursor {
            let rc = RendererConfig {
                backend: comet_renderer::BackendType::Wgpu,
                font_family: new.font.family.clone(),
                font_size: new.font.size,
                theme: "dark".to_string(),
                dpi_scale: 1.0,
                padding_x: 2.0,
                padding_y: 2.0,
                cursor_blink: new.cursor.blink,
                cursor_shape: match new.cursor.style.as_str() {
                    "beam" => comet_renderer::cursor::CursorShape::Beam,
                    "underline" => comet_renderer::cursor::CursorShape::Underline,
                    "hollow_block" => comet_renderer::cursor::CursorShape::HollowBlock,
                    "bar" => comet_renderer::cursor::CursorShape::Bar,
                    _ => comet_renderer::cursor::CursorShape::Block,
                },
                colors: Some(comet_renderer::CustomColors {
                    background: new.colors.background.clone(),
                    foreground: new.colors.foreground.clone(),
                    cursor: new.colors.cursor.clone(),
                    selection: new.colors.selection.clone(),
                }),
            };

            match Renderer::new(rc) {
                Ok(mut new_renderer) => {
                    let win_size = self.window.inner_size();
                    if let Err(e) =
                        new_renderer.initialize(
                            win_size.width,
                            win_size.height,
                            Some(Box::new(WindowOwner(self.window.clone()))
                                as Box<dyn HasWindowHandle>),
                        )
                    {
                        eprintln!("Failed to re-initialize renderer on config reload: {}", e);
                        return;
                    }
                    new_renderer.set_show_diagnostics(new.debug.show_fps);
                    self.manager.active_mut().set_renderer(new_renderer);
                    self.resize(win_size.width, win_size.height);
                }
                Err(e) => {
                    eprintln!("Failed to create renderer on config reload: {}", e);
                }
            }
        }

        self.needs_redraw = true;
        self.config = new_config;
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
        let session = self.manager.active_mut();
        if let Err(e) = session.render() {
            eprintln!("Render error: {}", e);
        }
    }

    /// Resizes the active session.
    fn resize(&mut self, width: u32, height: u32) {
        self.manager.active_mut().resize(width, height);
        self.needs_redraw = true;
    }

    /// Handles a keyboard event.
    fn handle_key(&mut self, event: &winit::event::KeyEvent) {
        if !event.state.is_pressed() {
            return;
        }

        let session = self.manager.active_mut();
        session.renderer().cursor_renderer().activity();

        let mods = self.modifiers.state();
        let ctrl = mods.control_key();
        let shift = mods.shift_key();

        // Clipboard shortcuts
        if ctrl
            && shift
            && let PhysicalKey::Code(code) = event.physical_key
        {
            match code {
                KeyCode::KeyC => {
                    self.copy_selection();
                    return;
                }
                KeyCode::KeyV => {
                    self.paste_clipboard();
                    return;
                }
                _ => {}
            }
        }

        // Ctrl+Shift+F: Search (stub)
        if ctrl && shift && event.physical_key == PhysicalKey::Code(KeyCode::KeyF) {
            // TODO: open search overlay
            return;
        }

        // Scrollback navigation
        if !ctrl {
            let terminal = session.terminal_mut();
            let handled = match event.physical_key {
                PhysicalKey::Code(KeyCode::PageUp) => {
                    terminal.scroll_viewport_pages(1);
                    true
                }
                PhysicalKey::Code(KeyCode::PageDown) => {
                    terminal.scroll_viewport_pages(-1);
                    true
                }
                PhysicalKey::Code(KeyCode::Home) => {
                    terminal.scroll_viewport_to_top();
                    true
                }
                PhysicalKey::Code(KeyCode::End) => {
                    terminal.scroll_viewport_to_bottom();
                    true
                }
                _ => false,
            };
            if handled {
                self.needs_redraw = true;
                return;
            }
        }

        // Send to PTY
        if let Some(bytes) = key_event_to_ansi(event, ctrl, mods.alt_key()) {
            session.write_input(&bytes);
        }
    }

    /// Handles mouse movement (selection drag).
    fn handle_mouse_move(&mut self, x: f64, y: f64) {
        if !self.is_selecting {
            return;
        }
        let session = self.manager.active();
        let cell_size = session.renderer().metrics().cell_size();
        let col = (x / cell_size.width as f64) as usize;
        let vis_row = (y / cell_size.height as f64) as usize;
        if vis_row >= session.terminal().height() {
            return;
        }
        let abs_row = session.terminal().visible_row_to_absolute(vis_row);
        let session = self.manager.active_mut();
        session.terminal_mut().update_selection(col, abs_row);
        self.needs_redraw = true;
    }

    /// Handles mouse button events.
    fn handle_mouse_button(&mut self, state: ElementState, button: MouseButton, x: f64, y: f64) {
        match button {
            MouseButton::Left => match state {
                ElementState::Pressed => {
                    let now = Instant::now();
                    let same_pos = (x - self.last_click_pos.0).abs() < 4.0
                        && (y - self.last_click_pos.1).abs() < 4.0;
                    if same_pos && now - self.last_click_time < DOUBLE_CLICK_INTERVAL {
                        self.click_count += 1;
                    } else {
                        self.click_count = 1;
                    }
                    self.last_click_time = now;
                    self.last_click_pos = (x, y);

                    self.is_selecting = true;

                    let session = self.manager.active();
                    let cell_size = session.renderer().metrics().cell_size();
                    let col = (x / cell_size.width as f64) as usize;
                    let vis_row = (y / cell_size.height as f64) as usize;
                    if vis_row < session.terminal().height() {
                        let abs_row = session.terminal().visible_row_to_absolute(vis_row);
                        let session = self.manager.active_mut();
                        session.terminal_mut().start_selection(col, abs_row);
                        match self.click_count {
                            2 => {
                                session.terminal_mut().expand_selection_to_word();
                            }
                            n if n >= 3 => {
                                session.terminal_mut().expand_selection_to_line();
                            }
                            _ => {}
                        }
                    }
                }
                ElementState::Released => {
                    if self.is_selecting {
                        self.is_selecting = false;
                        if self.click_count == 1 {
                            let session = self.manager.active_mut();
                            session.terminal_mut().end_selection();
                        }
                        if self.config.terminal.copy_on_select
                            && self.manager.active().terminal().has_selection()
                        {
                            self.copy_selection();
                        }
                        self.needs_redraw = true;
                    }
                }
            },
            MouseButton::Middle
                if state == ElementState::Pressed && self.config.terminal.middle_click_paste =>
            {
                self.paste_clipboard();
            }
            _ => {}
        }
    }

    /// Handles mouse wheel events.
    fn handle_mouse_wheel(&mut self, delta: &MouseScrollDelta) {
        let session = self.manager.active();
        let lines = match delta {
            MouseScrollDelta::LineDelta(_, y) => *y as isize,
            MouseScrollDelta::PixelDelta(pos) => {
                (pos.y / session.renderer().metrics().cell_size().height as f64) as isize
            }
        };
        let terminal = self.manager.active_mut().terminal_mut();
        if lines > 0 {
            terminal.scroll_viewport_up(lines as usize);
        } else if lines < 0 {
            terminal.scroll_viewport_down((-lines) as usize);
        }
        self.needs_redraw = true;
    }

    /// Copies the current selection to the clipboard.
    fn copy_selection(&mut self) {
        let terminal = self.manager.active().terminal();
        if !terminal.has_selection() {
            return;
        }
        let text = terminal.get_selection_text();
        if !text.is_empty() {
            let _ = self.clipboard.set_text(text);
        }
    }

    /// Pastes text from the clipboard into the PTY.
    fn paste_clipboard(&mut self) {
        if let Ok(text) = self.clipboard.get_text()
            && !text.is_empty()
        {
            self.manager.active_mut().write_input(text.as_bytes());
        }
    }

    /// Returns a reference to the window.
    pub fn window(&self) -> &Arc<Window> {
        &self.window
    }

    /// Returns a reference to the config.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Returns a mutable reference to the terminal manager.
    pub fn manager_mut(&mut self) -> &mut TerminalManager {
        &mut self.manager
    }
}

impl Drop for TerminalApp {
    fn drop(&mut self) {
        // Sessions are cleaned up in TerminalSession::drop
    }
}
