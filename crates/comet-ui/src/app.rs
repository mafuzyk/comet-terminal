use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use comet_config::{Config, ConfigWatcher, Session};
use comet_renderer::atlas::GlyphVertex;
use comet_renderer::colors::Rgba;
use comet_renderer::font::{FontSize, FontStyle};
use comet_renderer::glyph_cache::GlyphCache;
use comet_renderer::{HasWindowHandle, PaneRenderState, Renderer, RendererConfig, Viewport};
use raw_window_handle::{DisplayHandle, HandleError, HasDisplayHandle};
use winit::{
    event::{ElementState, Modifiers, MouseButton, MouseScrollDelta, WindowEvent},
    keyboard::{KeyCode, PhysicalKey},
    raw_window_handle::HasWindowHandle as RawHasWindowHandle,
    window::Window,
};

use crate::input::key_event_to_ansi;
use crate::manager::TerminalManager;
use crate::workspace::{PaneId, PaneViewport, TAB_BAR_HEIGHT};

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

// `WindowOwner` gets `HasWindowHandle` via the blanket impl in `comet_renderer`.

/// Actions that can be bound to keyboard shortcuts.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Action {
    Copy,
    Paste,
    Search,
    NewTab,
    CloseTab,
    NextTab,
    PreviousTab,
    SplitHorizontal,
    SplitVertical,
    ClosePane,
    ScrollPageUp,
    ScrollPageDown,
    ScrollToTop,
    ScrollToBottom,
    FocusUp,
    FocusDown,
    FocusLeft,
    FocusRight,
}

/// Search overlay state.
pub struct SearchState {
    pub active: bool,
    pub query: String,
    pub results: Vec<(usize, usize)>,
    pub current_match: usize,
}

impl SearchState {
    fn new() -> Self {
        Self {
            active: false,
            query: String::new(),
            results: Vec::new(),
            current_match: 0,
        }
    }

    fn search(&mut self, terminal: &comet_core::Terminal) {
        self.results.clear();
        self.current_match = 0;
        if self.query.is_empty() {
            return;
        }
        let total_rows = terminal.height() + terminal.scrollback_len();
        let lower = self.query.to_lowercase();
        for abs_row in 0..total_rows {
            let row_text = terminal.get_row_text(abs_row);
            let row_lower = row_text.to_lowercase();
            if let Some(col) = row_lower.find(&lower) {
                self.results.push((abs_row, col));
            }
        }
    }

    fn next_match(&mut self) {
        if !self.results.is_empty() {
            self.current_match = (self.current_match + 1) % self.results.len();
        }
    }

    #[allow(dead_code)]
    fn previous_match(&mut self) {
        if !self.results.is_empty() {
            self.current_match = if self.current_match == 0 {
                self.results.len() - 1
            } else {
                self.current_match - 1
            };
        }
    }
}

/// Terminal application state — owns shared renderer, workspace, and top-level event handling.
pub struct TerminalApp {
    manager: TerminalManager,
    /// Shared renderer — renders all panes into their respective viewports.
    renderer: Renderer,
    /// Per-pane render state for each session.
    pane_states: HashMap<PaneId, PaneRenderState>,
    window: Arc<Window>,
    needs_redraw: bool,
    modifiers: Modifiers,
    config: Config,
    config_watcher: ConfigWatcher,
    _session: Session,
    is_selecting: bool,
    clipboard: arboard::Clipboard,
    click_count: u32,
    last_click_time: Instant,
    last_click_pos: (f64, f64),
    mouse_pos: (f64, f64),
    search: SearchState,
    bell_pending: bool,
    /// Cached pane viewports for the current frame.
    pane_viewports: Vec<PaneViewport>,
    /// Tab index whose close button the mouse is hovering over.
    hovered_close_button: Option<usize>,
    /// Index of the divider currently being dragged, if any.
    dragging_divider: Option<usize>,
    /// Cursor Y at the start of the drag (screen coordinates).
    drag_start_y: f64,
}

impl TerminalApp {
    /// Creates a new terminal application.
    pub fn new(window: Arc<Window>, config: Config, session: Session) -> Self {
        let window_size = window.inner_size();

        let manager = TerminalManager::new(&config);

        // Create the shared renderer
        let rc = renderer_config_from_config(&config);
        let mut renderer = Renderer::new(rc).expect("Failed to create renderer");

        let window_handle = Box::new(WindowOwner(window.clone())) as Box<dyn HasWindowHandle>;
        renderer
            .initialize(window_size.width, window_size.height, Some(window_handle))
            .expect("Failed to initialize renderer");
        renderer.set_show_diagnostics(config.debug.show_fps);

        // Create per-pane render states
        let mut pane_states = HashMap::new();
        let cell_size = renderer.metrics().cell_size();
        if let Some(ws) = manager.workspace().tabs.first() {
            for pane in &ws.panes {
                let mut ps = PaneRenderState::new(
                    cell_size.width as f32,
                    cell_size.height as f32,
                    renderer.cursor_shape(),
                    renderer.cursor_blink(),
                    renderer.cursor_color(),
                );
                ps.cursor_renderer.set_shape(renderer.cursor_shape());
                ps.cursor_renderer.set_blink(renderer.cursor_blink());
                ps.cursor_renderer.set_color(renderer.cursor_color());
                pane_states.insert(pane.id, ps);
            }
        }

        let mut app = Self {
            manager,
            renderer,
            pane_states,
            window,
            needs_redraw: true,
            modifiers: Modifiers::default(),
            config,
            config_watcher: ConfigWatcher::new().unwrap_or_else(|_| ConfigWatcher::dummy()),
            _session: session,
            is_selecting: false,
            clipboard: arboard::Clipboard::new().expect("Failed to initialize clipboard"),
            click_count: 0,
            last_click_time: Instant::now(),
            last_click_pos: (0.0, 0.0),
            mouse_pos: (0.0, 0.0),
            search: SearchState::new(),
            bell_pending: false,
            pane_viewports: Vec::new(),
            hovered_close_button: None,
            dragging_divider: None,
            drag_start_y: 0.0,
        };

        // Resize all panes to the initial window size
        app.resize_panes(window_size.width, window_size.height);

        app
    }

    /// Handles a winit window event.
    pub fn handle_window_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::Resized(size) => {
                let _ = self.renderer.resize(size.width, size.height);
                self.resize_panes(size.width, size.height);
                self.needs_redraw = true;
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
                // If the frame was skipped (e.g. Wayland initial configure
                // handshake not yet done), keep needs_redraw so the event
                // loop keeps trying.
                self.needs_redraw = self.renderer.frame_skipped();
            }
            _ => {}
        }
    }

    /// Compute viewports for visible panes and resize their sessions.
    fn resize_panes(&mut self, width: u32, height: u32) {
        self.pane_viewports = self
            .manager
            .workspace()
            .compute_pane_viewports(width, height);

        for vp in &self.pane_viewports {
            if let Some(session) = self.manager.session_by_pane_id_mut(vp.pane_id) {
                let cell_size = self.renderer.metrics().cell_size();
                session.resize(vp.width, vp.height, (cell_size.width, cell_size.height));
            }
            // Update pane render state cell sizes
            let cell_size = self.renderer.metrics().cell_size();
            if let Some(state) = self.pane_states.get_mut(&vp.pane_id) {
                state.set_cell_size(cell_size.width as f32, cell_size.height as f32);
            }
        }
    }

    /// Processes pending PTY output and checks for config changes.
    pub fn process_pty_output(&mut self) {
        let had_data = self.manager.active_mut().process_output();
        if had_data {
            self.needs_redraw = true;
            self.window.request_redraw();
        }

        let _ = self.manager.process_output();

        if self.config_watcher.check() {
            self.reload_config();
        }

        if self.bell_pending {
            self.bell_pending = false;
            self.send_bell_notification();
        }
    }

    /// Reload configuration from disk and apply changes.
    fn reload_config(&mut self) {
        let Ok(new_config) = comet_config::load_config() else {
            return;
        };

        let old = &self.config;
        let new = &new_config;

        if old.font != new.font || old.colors != new.colors || old.cursor != new.cursor {
            let rc = renderer_config_from_config(new);
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
                    self.renderer = new_renderer;
                    self.resize_panes(win_size.width, win_size.height);
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

    /// Requests a redraw on the window.
    pub fn request_redraw(&mut self) {
        self.window.request_redraw();
    }

    /// Renders all visible panes + tab bar + overlays.
    fn render(&mut self) {
        // Cache the viewports for visible panes
        let win_size = self.window.inner_size();
        self.pane_viewports = self
            .manager
            .workspace()
            .compute_pane_viewports(win_size.width, win_size.height);

        if self.pane_viewports.is_empty() {
            return;
        }

        if let Err(e) = self.renderer.begin_frame() {
            eprintln!("Begin frame error: {}", e);
            return;
        }

        // Check if the frame was skipped (transient Timeout on Wayland).
        // Must capture BEFORE end_frame() which resets the flag.
        let skipped = self.renderer.frame_skipped();

        if !skipped {
            for vp in &self.pane_viewports {
                let pane_id = vp.pane_id;
                let Some(session) = self.manager.session_by_pane_id_mut(pane_id) else {
                    continue;
                };
                let Some(state) = self.pane_states.get_mut(&pane_id) else {
                    continue;
                };
                let terminal = session.terminal();
                if let Err(e) = self
                    .renderer
                    .render_pane(terminal, vp.to_render_viewport(), state)
                {
                    eprintln!("Render pane error: {}", e);
                }
            }

            // Render split dividers between panes
            self.render_dividers(win_size.width, win_size.height);

            // Render focus indicator around the active pane
            self.render_focus_indicator(win_size.width, win_size.height);

            // Render tab bar overlay (background + tab titles)
            self.render_tab_bar(win_size.width);
        }

        if let Err(e) = self.renderer.end_frame() {
            eprintln!("End frame error: {}", e);
        }
    }

    /// Renders the tab bar at the top of the window.
    fn render_tab_bar(&mut self, window_width: u32) {
        let tabs = &self.manager.workspace().tabs;
        if tabs.is_empty() {
            return;
        }

        let tab_bar_height = TAB_BAR_HEIGHT;
        let window_width_f = window_width as f32;

        // Derive tab bar colors from the terminal theme
        let palette = self.renderer.colors();
        let default_bg = palette.default_bg;
        let default_fg = palette.default_fg;

        // Darken background by lerping towards black
        let tab_bar_bg = default_bg.lerp(&Rgba::rgb(0, 0, 0), 0.25);
        let inactive_tab_bg = default_bg.lerp(&Rgba::rgb(0, 0, 0), 0.12);
        let active_tab_bg = default_bg;
        let inactive_text_fg = default_fg.lerp(&Rgba::rgb(128, 128, 128), 0.5);
        let active_text_fg = default_fg;

        let mut solid_vertices = Vec::new();
        let mut glyph_vertices = Vec::new();

        // Full tab bar background
        add_rect_vertices(
            &mut solid_vertices,
            0.0,
            0.0,
            window_width_f,
            tab_bar_height as f32,
            tab_bar_bg.to_f32_array(),
        );

        // Each tab
        let tab_count = tabs.len();
        let tab_width = window_width_f / tab_count as f32;
        let active_tab_idx = self.manager.workspace().active_tab;

        let glyph_cache = match self.renderer.glyph_cache() {
            Some(gc) => gc,
            None => return,
        };
        let font_size = FontSize::new(13);
        let font_style = FontStyle::normal();

        for (i, tab) in tabs.iter().enumerate() {
            let is_active = i == active_tab_idx;
            let bg = if is_active {
                active_tab_bg
            } else {
                inactive_tab_bg
            };
            let tc = if is_active {
                active_text_fg
            } else {
                inactive_text_fg
            };
            let x = i as f32 * tab_width;

            // Tab background (1px gap between tabs)
            add_rect_vertices(
                &mut solid_vertices,
                x + 1.0,
                2.0,
                tab_width - 2.0,
                (tab_bar_height - 2) as f32,
                bg.to_f32_array(),
            );

            // Active tab indicator line (bottom border)
            if is_active {
                add_rect_vertices(
                    &mut solid_vertices,
                    x + 2.0,
                    (tab_bar_height - 2) as f32,
                    tab_width - 4.0,
                    2.0,
                    palette.cursor.to_f32_array(),
                );
            }

            // Tab title text (left-aligned with padding)
            let title = &tab.title;
            let max_title_w = tab_width - 38.0;
            let display_title = if max_title_w > 0.0 {
                let mut w = 0.0;
                let mut truncated = String::new();
                for ch in title.chars() {
                    if let Ok(cached) = glyph_cache.get_glyph(ch, font_size, font_style) {
                        if w + cached.advance_width > max_title_w {
                            truncated.push('…');
                            break;
                        }
                        w += cached.advance_width;
                        truncated.push(ch);
                    }
                }
                if truncated.is_empty() { title.clone() } else { truncated }
            } else {
                title.clone()
            };
            add_text_vertices(
                &mut glyph_vertices,
                &display_title,
                x + 8.0,
                8.0,
                tc.to_f32_array(),
                glyph_cache,
                font_size,
                font_style,
            );

            // Close button on the right side of the tab
            let btn_size = 20.0;
            let btn_x = x + tab_width - btn_size - 4.0;
            let btn_y = ((tab_bar_height as f32) - btn_size) / 2.0;
            let is_hovered = self.hovered_close_button == Some(i);
            let btn_bg = if is_hovered {
                default_bg.lerp(&Rgba::rgb(0, 0, 0), 0.05)
            } else {
                default_bg.lerp(&Rgba::rgb(0, 0, 0), 0.20)
            };
            add_rect_vertices(
                &mut solid_vertices,
                btn_x,
                btn_y,
                btn_size,
                btn_size,
                btn_bg.to_f32_array(),
            );
            // "×" glyph centered in the button
            add_text_vertices(
                &mut glyph_vertices,
                "\u{00D7}",
                btn_x + 4.0,
                btn_y + 2.0,
                tc.to_f32_array(),
                glyph_cache,
                font_size,
                font_style,
            );
        }

        if let Err(e) = self.renderer.render_overlay(
            &solid_vertices,
            &glyph_vertices,
            Viewport {
                x: 0,
                y: 0,
                width: window_width,
                height: tab_bar_height,
            },
        ) {
            eprintln!("Tab bar render error: {}", e);
        }
    }

    /// Renders split dividers between adjacent panes.
    fn render_dividers(&mut self, window_width: u32, window_height: u32) {
        let dividers = self
            .manager
            .workspace()
            .compute_dividers(window_width, window_height);
        if dividers.is_empty() {
            return;
        }

        // Derive divider color from theme background (darker variant)
        let palette = self.renderer.colors();
        let divider_color = palette
            .default_bg
            .lerp(&Rgba::rgb(0, 0, 0), 0.35)
            .to_f32_array();
        let focus_color = palette
            .default_bg
            .lerp(&Rgba::rgb(0, 0, 0), 0.15)
            .to_f32_array();

        let active_pane_id = self.manager.workspace().active_pane().id;
        let mut solid_vertices = Vec::with_capacity(dividers.len() * 6);

        for d in &dividers {
            // Highlight dividers adjacent to the active pane
            let adjacent_to_active = self.pane_viewports.iter().any(|vp| {
                vp.pane_id == active_pane_id
                    && match d.orientation {
                        crate::workspace::DividerOrientation::Horizontal => {
                            vp.y + vp.height == d.y || vp.y == d.y + d.height
                        }
                        crate::workspace::DividerOrientation::Vertical => {
                            vp.x + vp.width == d.x || vp.x == d.x + d.width
                        }
                    }
            });

            let color = if adjacent_to_active {
                focus_color
            } else {
                divider_color
            };

            add_rect_vertices(
                &mut solid_vertices,
                d.x as f32,
                d.y as f32,
                d.width as f32,
                d.height as f32,
                color,
            );
        }

        if let Err(e) = self.renderer.render_overlay(
            &solid_vertices,
            &[],
            Viewport {
                x: 0,
                y: TAB_BAR_HEIGHT,
                width: window_width,
                height: window_height.saturating_sub(TAB_BAR_HEIGHT),
            },
        ) {
            eprintln!("Divider render error: {}", e);
        }
    }

    /// Render a 2px inset focus border around the active pane.
    fn render_focus_indicator(&mut self, window_width: u32, window_height: u32) {
        let active_pane_id = self.manager.workspace().active_pane().id;
        let Some(vp) = self.pane_viewports.iter().find(|vp| vp.pane_id == active_pane_id) else {
            return;
        };

        let palette = self.renderer.colors();
        let border_color = palette.cursor.to_f32_array();

        let px = vp.x as f32;
        let py = vp.y as f32;
        let w = vp.width as f32;
        let h = vp.height as f32;
        let border = 2.0;

        let mut solid_vertices = Vec::with_capacity(4 * 6);
        // top
        add_rect_vertices(&mut solid_vertices, px, py, w, border, border_color);
        // bottom
        add_rect_vertices(&mut solid_vertices, px, py + h - border, w, border, border_color);
        // left
        add_rect_vertices(&mut solid_vertices, px, py, border, h, border_color);
        // right
        add_rect_vertices(&mut solid_vertices, px + w - border, py, border, h, border_color);

        if let Err(e) = self.renderer.render_overlay(
            &solid_vertices,
            &[],
            Viewport {
                x: 0,
                y: 0,
                width: window_width,
                height: window_height,
            },
        ) {
            eprintln!("Focus indicator render error: {}", e);
        }
    }

    /// Dispatch an action from keyboard or other trigger.
    fn dispatch_action(&mut self, action: &Action) {
        match action {
            Action::Copy => self.copy_selection(),
            Action::Paste => self.paste_clipboard(),
            Action::Search => self.toggle_search(),
            Action::NewTab => {
                self.manager.create_tab(&self.config);
                self.init_pane_state_for_active();
                let win_size = self.window.inner_size();
                self.resize_panes(win_size.width, win_size.height);
                self.needs_redraw = true;
            }
            Action::CloseTab => {
                let active_pane = self.manager.workspace().active_pane().id;
                self.pane_states.remove(&active_pane);
                if self.manager.close_active_tab() {
                    // All tabs closed
                }
                let win_size = self.window.inner_size();
                self.resize_panes(win_size.width, win_size.height);
                self.needs_redraw = true;
            }
            Action::NextTab => {
                self.manager.next_tab();
                let win_size = self.window.inner_size();
                self.resize_panes(win_size.width, win_size.height);
                self.needs_redraw = true;
            }
            Action::PreviousTab => {
                self.manager.previous_tab();
                let win_size = self.window.inner_size();
                self.resize_panes(win_size.width, win_size.height);
                self.needs_redraw = true;
            }
            Action::SplitHorizontal | Action::SplitVertical => {
                let split_fn = match action {
                    Action::SplitHorizontal => TerminalManager::split_horizontal,
                    _ => TerminalManager::split_vertical,
                };
                split_fn(&mut self.manager, &self.config);
                self.init_pane_state_for_active();
                let win_size = self.window.inner_size();
                self.resize_panes(win_size.width, win_size.height);
                self.needs_redraw = true;
            }
            Action::ClosePane => {
                if self.manager.close_active_pane() {
                    // all panes gone
                }
                let win_size = self.window.inner_size();
                self.resize_panes(win_size.width, win_size.height);
                self.needs_redraw = true;
            }
            Action::ScrollPageUp => {
                let terminal = self.manager.active_mut().terminal_mut();
                terminal.scroll_viewport_pages(1);
                self.needs_redraw = true;
            }
            Action::ScrollPageDown => {
                let terminal = self.manager.active_mut().terminal_mut();
                terminal.scroll_viewport_pages(-1);
                self.needs_redraw = true;
            }
            Action::ScrollToTop => {
                let terminal = self.manager.active_mut().terminal_mut();
                terminal.scroll_viewport_to_top();
                self.needs_redraw = true;
            }
            Action::ScrollToBottom => {
                let terminal = self.manager.active_mut().terminal_mut();
                terminal.scroll_viewport_to_bottom();
                self.needs_redraw = true;
            }
            Action::FocusUp => {
                self.manager.workspace_mut().focus_up();
                self.needs_redraw = true;
            }
            Action::FocusDown => {
                self.manager.workspace_mut().focus_down();
                self.needs_redraw = true;
            }
            Action::FocusLeft => {
                self.manager.workspace_mut().focus_left();
                self.needs_redraw = true;
            }
            Action::FocusRight => {
                self.manager.workspace_mut().focus_right();
                self.needs_redraw = true;
            }
        }
    }

    /// Initialize the pane render state for the active session.
    fn init_pane_state_for_active(&mut self) {
        let ws = self.manager.workspace();
        let cell_size = self.renderer.metrics().cell_size();
        if let Some(tab) = ws.tabs.get(ws.active_tab) {
            if let Some(pane) = tab.panes.last() {
                if !self.pane_states.contains_key(&pane.id) {
                    let ps = PaneRenderState::new(
                        cell_size.width as f32,
                        cell_size.height as f32,
                        self.renderer.cursor_shape(),
                        self.renderer.cursor_blink(),
                        self.renderer.cursor_color(),
                    );
                    self.pane_states.insert(pane.id, ps);
                }
            }
        }
    }

    /// Toggle the search overlay.
    fn toggle_search(&mut self) {
        self.search.active = !self.search.active;
        if self.search.active {
            self.search.query.clear();
            self.search.results.clear();
            self.search.current_match = 0;
        }
        self.needs_redraw = true;
    }

    /// Handle a character input during search.
    fn search_input(&mut self, c: char) {
        self.search.query.push(c);
        let terminal = self.manager.active().terminal();
        self.search.search(terminal);
        self.needs_redraw = true;
    }

    /// Handle search backspace.
    fn search_backspace(&mut self) {
        self.search.query.pop();
        let terminal = self.manager.active().terminal();
        self.search.search(terminal);
        self.needs_redraw = true;
    }

    /// Send a desktop notification for bell.
    fn send_bell_notification(&self) {
        #[cfg(target_os = "linux")]
        {
            match self.config.terminal.bell {
                true => {
                    let _ = std::process::Command::new("notify-send")
                        .arg("Comet Terminal")
                        .arg("Bell")
                        .arg("--icon=terminal")
                        .arg("--expire-time=2000")
                        .spawn();
                }
                false => {}
            }
        }
    }

    /// Parse a keybinding string and check if it matches the current modifiers and key.
    fn match_keybinding(
        binding: &str,
        ctrl: bool,
        shift: bool,
        alt: bool,
        key: PhysicalKey,
    ) -> bool {
        let parts: Vec<&str> = binding.split('+').collect();
        if parts.is_empty() {
            return false;
        }

        let mut need_ctrl = false;
        let mut need_shift = false;
        let mut need_alt = false;

        for part in &parts[..parts.len() - 1] {
            match *part {
                "Ctrl" | "Control" => need_ctrl = true,
                "Shift" => need_shift = true,
                "Alt" | "Option" => need_alt = true,
                _ => {}
            }
        }

        if need_ctrl != ctrl || need_shift != shift || need_alt != alt {
            return false;
        }

        let key_name = parts[parts.len() - 1];
        match key {
            PhysicalKey::Code(code) => match (key_name, code) {
                ("C", KeyCode::KeyC)
                | ("V", KeyCode::KeyV)
                | ("F", KeyCode::KeyF)
                | ("T", KeyCode::KeyT)
                | ("W", KeyCode::KeyW)
                | ("Q", KeyCode::KeyQ)
                | ("H", KeyCode::KeyH) => true,
                ("Tab", KeyCode::Tab)
                | ("Enter", KeyCode::Enter)
                | ("Escape", KeyCode::Escape)
                | ("Backspace", KeyCode::Backspace)
                | ("Space", KeyCode::Space)
                | ("Delete", KeyCode::Delete)
                | ("Insert", KeyCode::Insert)
                | ("Home", KeyCode::Home)
                | ("End", KeyCode::End)
                | ("PageUp", KeyCode::PageUp)
                | ("PageDown", KeyCode::PageDown)
                | ("Up", KeyCode::ArrowUp)
                | ("Down", KeyCode::ArrowDown)
                | ("Left", KeyCode::ArrowLeft)
                | ("Right", KeyCode::ArrowRight) => true,
                _ => key_name == format!("{:?}", code),
            },
            _ => false,
        }
    }

    /// Handle a keyboard event.
    fn handle_key(&mut self, event: &winit::event::KeyEvent) {
        if !event.state.is_pressed() {
            return;
        }

        let mods = self.modifiers.state();
        let ctrl = mods.control_key();
        let shift = mods.shift_key();
        let alt = mods.alt_key();

        if self.search.active {
            match event.physical_key {
                PhysicalKey::Code(KeyCode::Escape) => {
                    self.toggle_search();
                    return;
                }
                PhysicalKey::Code(KeyCode::Enter) => {
                    self.search.next_match();
                    if let Some(&(abs_row, _col)) =
                        self.search.results.get(self.search.current_match)
                    {
                        let terminal = self.manager.active_mut().terminal_mut();
                        terminal.scroll_to_absolute_row(abs_row);
                    }
                    self.needs_redraw = true;
                    return;
                }
                PhysicalKey::Code(KeyCode::Backspace) => {
                    self.search_backspace();
                    return;
                }
                _ => {
                    if let Some(text) = &event.text {
                        for c in text.chars() {
                            if c.is_control() {
                                continue;
                            }
                            self.search_input(c);
                        }
                        return;
                    }
                }
            }
            return;
        }

        let s = &self.config.shortcuts;

        macro_rules! try_binding {
            ($binding:expr, $action:expr) => {
                if Self::match_keybinding($binding, ctrl, shift, alt, event.physical_key) {
                    self.dispatch_action(&$action);
                    return;
                }
            };
        }

        try_binding!(&s.copy, Action::Copy);
        try_binding!(&s.paste, Action::Paste);
        try_binding!(&s.search, Action::Search);
        try_binding!(&s.new_tab, Action::NewTab);
        try_binding!(&s.close_tab, Action::CloseTab);
        try_binding!(&s.next_tab, Action::NextTab);
        try_binding!(&s.previous_tab, Action::PreviousTab);
        try_binding!(&s.split_horizontal, Action::SplitHorizontal);
        try_binding!(&s.split_vertical, Action::SplitVertical);
        try_binding!(&s.close_pane, Action::ClosePane);
        try_binding!(&s.scroll_page_up, Action::ScrollPageUp);
        try_binding!(&s.scroll_page_down, Action::ScrollPageDown);
        try_binding!(&s.scroll_to_top, Action::ScrollToTop);
        try_binding!(&s.scroll_to_bottom, Action::ScrollToBottom);
        try_binding!(&s.focus_up, Action::FocusUp);
        try_binding!(&s.focus_down, Action::FocusDown);
        try_binding!(&s.focus_left, Action::FocusLeft);
        try_binding!(&s.focus_right, Action::FocusRight);

        // Send to PTY
        let pane_id = self.manager.workspace().active_pane().id;
        let session = self.manager.active_mut();
        if let Some(state) = self.pane_states.get_mut(&pane_id) {
            state.cursor_activity();
        }
        if let Some(bytes) = key_event_to_ansi(event, ctrl, alt) {
            session.write_input(&bytes);
        }
    }

    /// Handle mouse movement.
    fn handle_mouse_move(&mut self, x: f64, y: f64) {
        // If dragging a divider, update pane sizes
        if self.dragging_divider.is_some() {
            let win_size = self.window.inner_size();
            let delta_y = (y - self.drag_start_y) as i32;
            if let Some(di) = self.dragging_divider {
                self.manager
                    .workspace_mut()
                    .drag_divider(di, delta_y, win_size.height);
            }
            self.resize_panes(win_size.width, win_size.height);
            self.needs_redraw = true;
            return;
        }

        // Check for tab bar region
        if y < TAB_BAR_HEIGHT as f64 {
            let old_hover = self.hovered_close_button.take();
            let new_hover = self.close_button_at(x, y);
            if old_hover != new_hover {
                self.needs_redraw = true;
            }
            self.hovered_close_button = new_hover;
            return;
        }
        if self.hovered_close_button.is_some() {
            self.hovered_close_button = None;
            self.needs_redraw = true;
        }

        let mods = self.modifiers.state();
        let ctrl = mods.control_key();
        if ctrl && self.find_pane_at(x, y).is_some() {
            // Future: set cursor to pointer when hovering over hyperlink
        }

        if !self.is_selecting {
            return;
        }

        // Find which pane we're in
        let (pane_id, vp_x, vp_y) = match self.find_pane_at(x, y) {
            Some((id, vp)) => (id, vp.x as f64, vp.y as f64),
            None => return,
        };
        let Some(session) = self.manager.session_by_pane_id_mut(pane_id) else {
            return;
        };

        let cell_size = self.renderer.metrics().cell_size();
        let col = ((x - vp_x) / cell_size.width as f64) as usize;
        let vis_row = ((y - vp_y) / cell_size.height as f64) as usize;
        if vis_row >= session.terminal().height() {
            return;
        }
        let abs_row = session.terminal().visible_row_to_absolute(vis_row);
        session.terminal_mut().update_selection(col, abs_row);
        self.needs_redraw = true;
    }

    /// Handle mouse button events.
    fn handle_mouse_button(&mut self, state: ElementState, button: MouseButton, x: f64, y: f64) {
        // Ending a divider drag on any button release
        if state == ElementState::Released && self.dragging_divider.is_some() {
            self.dragging_divider = None;
            return;
        }

        // Left button release ends selection (do this before the Pressed-only
        // return so selection finalisation still works)
        if state == ElementState::Released && button == MouseButton::Left {
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
            return;
        }

        if state != ElementState::Pressed {
            return;
        }

        // Starting a divider drag (must check before tab bar since
        // dividers are visually inside the pane area)
        if button == MouseButton::Left
            && y >= TAB_BAR_HEIGHT as f64
            && self.divider_at(x, y).is_some()
        {
            if let Some(di) = self.divider_at(x, y) {
                self.dragging_divider = Some(di);
                self.drag_start_y = y;
            }
            return;
        }

        // Tab bar click handling
        if y < TAB_BAR_HEIGHT as f64 {
            if let Some(idx) = self.close_button_at(x, y) {
                self.manager.switch_to_tab(idx);
                if self.manager.close_active_tab() {
                    // all tabs closed – application may exit
                }
                let win_size = self.window.inner_size();
                self.resize_panes(win_size.width, win_size.height);
                self.needs_redraw = true;
                return;
            }
            let tab_idx = self.tab_at_x(x);
            if let Some(idx) = tab_idx {
                self.manager.switch_to_tab(idx);
                let win_size = self.window.inner_size();
                self.resize_panes(win_size.width, win_size.height);
                self.needs_redraw = true;
            }
            return;
        }

        let mods = self.modifiers.state();
        let ctrl = mods.control_key();

        match button {
            MouseButton::Left => {
                if ctrl && state == ElementState::Pressed {
                    self.handle_ctrl_click(x, y);
                    return;
                }

                match state {
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

                        let (pane_id, vp_x, vp_y) = match self.find_pane_at(x, y) {
                            Some((id, vp)) => (id, vp.x as f64, vp.y as f64),
                            None => return,
                        };
                        let Some(session) = self.manager.session_by_pane_id_mut(pane_id) else {
                            return;
                        };
                        let cell_size = self.renderer.metrics().cell_size();
                        let col = ((x - vp_x) / cell_size.width as f64) as usize;
                        let vis_row = ((y - vp_y) / cell_size.height as f64) as usize;
                        if vis_row < session.terminal().height() {
                            let abs_row = session.terminal().visible_row_to_absolute(vis_row);
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
                    _ => {}
                }
            }
            MouseButton::Middle
                if state == ElementState::Pressed && self.config.terminal.middle_click_paste =>
            {
                self.paste_clipboard();
            }
            _ => {}
        }
    }

    /// Handle Ctrl+Click to open hyperlinks.
    fn handle_ctrl_click(&mut self, x: f64, y: f64) {
        let (pane_id, vp_x, vp_y) = match self.find_pane_at(x, y) {
            Some((id, vp)) => (id, vp.x as f64, vp.y as f64),
            None => return,
        };
        let Some(session) = self.manager.session_by_pane_id(pane_id) else {
            return;
        };
        let cell_size = self.renderer.metrics().cell_size();
        let col = ((x - vp_x) / cell_size.width as f64) as usize;
        let vis_row = ((y - vp_y) / cell_size.height as f64) as usize;
        if vis_row >= session.terminal().height() {
            return;
        }
        let abs_row = session.terminal().visible_row_to_absolute(vis_row);
        let hyperlink = session.terminal().hyperlink_at(col, abs_row);
        if let Some(url) = hyperlink {
            open_url(url);
        }
    }

    /// Handle mouse wheel events.
    fn handle_mouse_wheel(&mut self, delta: &MouseScrollDelta) {
        let session = self.manager.active_mut();
        let cell_size = self.renderer.metrics().cell_size();
        let lines = match delta {
            MouseScrollDelta::LineDelta(_, y) => *y as isize,
            MouseScrollDelta::PixelDelta(pos) => (pos.y / cell_size.height as f64) as isize,
        };
        let terminal = session.terminal_mut();
        if lines > 0 {
            terminal.scroll_viewport_up(lines as usize);
        } else if lines < 0 {
            terminal.scroll_viewport_down((-lines) as usize);
        }
        self.needs_redraw = true;
    }

    /// Find which pane (if any) contains the given screen coordinates.
    fn find_pane_at(&self, x: f64, y: f64) -> Option<(PaneId, &PaneViewport)> {
        for vp in &self.pane_viewports {
            let rect = vp.x as f64..(vp.x + vp.width) as f64;
            let rect_y = vp.y as f64..(vp.y + vp.height) as f64;
            if rect.contains(&x) && rect_y.contains(&y) {
                return Some((vp.pane_id, vp));
            }
        }
        None
    }

    /// Determine which tab index is at the given x coordinate.
    fn tab_at_x(&self, x: f64) -> Option<usize> {
        let tabs = &self.manager.workspace().tabs;
        if tabs.is_empty() {
            return None;
        }
        // Each tab gets roughly equal width
        let tab_width = self.window.inner_size().width / tabs.len() as u32;
        let idx = (x as u32) / tab_width;
        if (idx as usize) < tabs.len() {
            Some(idx as usize)
        } else {
            Some(tabs.len() - 1)
        }
    }

    /// Returns the tab index whose close button contains `(x, y)`, or `None`.
    fn close_button_at(&self, x: f64, y: f64) -> Option<usize> {
        let tabs = &self.manager.workspace().tabs;
        if tabs.is_empty() {
            return None;
        }
        let tab_width = self.window.inner_size().width as f32 / tabs.len() as f32;
        let tab_idx = (x as f32 / tab_width) as usize;
        if tab_idx >= tabs.len() {
            return None;
        }
        let btn_size = 20.0;
        let btn_x = tab_idx as f32 * tab_width + tab_width - btn_size - 4.0;
        let btn_y = ((TAB_BAR_HEIGHT as f32) - btn_size) / 2.0;
        let xf = x as f32;
        let yf = y as f32;
        if xf >= btn_x && xf <= btn_x + btn_size && yf >= btn_y && yf <= btn_y + btn_size {
            Some(tab_idx)
        } else {
            None
        }
    }

    /// Returns the divider index at `(x, y)` in the pane area, or `None`.
    fn divider_at(&self, x: f64, y: f64) -> Option<usize> {
        let win_size = self.window.inner_size();
        let dividers = self
            .manager
            .workspace()
            .compute_dividers(win_size.width, win_size.height);
        for (i, d) in dividers.iter().enumerate() {
            let dx = d.x as f64;
            let dy = d.y as f64;
            let dw = d.width as f64;
            let dh = d.height as f64;
            if x >= dx && x <= dx + dw && y >= dy && y <= dy + dh {
                return Some(i);
            }
        }
        None
    }

    /// Copy selection to clipboard.
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

    /// Paste clipboard contents.
    fn paste_clipboard(&mut self) {
        if let Ok(text) = self.clipboard.get_text()
            && !text.is_empty()
        {
            self.manager.active_mut().write_input(text.as_bytes());
        }
    }

    pub fn window(&self) -> &Arc<Window> {
        &self.window
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn manager_mut(&mut self) -> &mut TerminalManager {
        &mut self.manager
    }

    pub fn search_state(&self) -> &SearchState {
        &self.search
    }
}

impl Drop for TerminalApp {
    fn drop(&mut self) {}
}

fn renderer_config_from_config(config: &Config) -> RendererConfig {
    RendererConfig {
        backend: comet_renderer::BackendType::Wgpu,
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

/// Adds a filled rectangle as 6 triangle vertices (2 triangles) to the vertex list.
fn add_rect_vertices(
    vertices: &mut Vec<GlyphVertex>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: [f32; 4],
) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let zero_uv = [0.0, 0.0];
    vertices.push(GlyphVertex::new([x, y], zero_uv, color));
    vertices.push(GlyphVertex::new([x + w, y], zero_uv, color));
    vertices.push(GlyphVertex::new([x, y + h], zero_uv, color));
    vertices.push(GlyphVertex::new([x + w, y], zero_uv, color));
    vertices.push(GlyphVertex::new([x + w, y + h], zero_uv, color));
    vertices.push(GlyphVertex::new([x, y + h], zero_uv, color));
}

/// Adds text glyph vertices for each character in the string.
fn add_text_vertices(
    vertices: &mut Vec<GlyphVertex>,
    text: &str,
    x: f32,
    y: f32,
    color: [f32; 4],
    glyph_cache: &GlyphCache,
    font_size: FontSize,
    font_style: FontStyle,
) {
    let atlas = glyph_cache.atlas();
    let aw = atlas.dimensions().0 as f32;
    let ah = atlas.dimensions().1 as f32;
    let mut cx = x;

    for ch in text.chars() {
        if let Ok(cached) = glyph_cache.get_glyph(ch, font_size, font_style) {
            let rect = cached.rect;
            let u0 = rect.x as f32 / aw;
            let v0 = rect.y as f32 / ah;
            let u1 = (rect.x + rect.width) as f32 / aw;
            let v1 = (rect.y + rect.height) as f32 / ah;
            let gw = rect.width as f32;
            let gh = rect.height as f32;

            vertices.push(GlyphVertex::new([cx, y], [u0, v0], color));
            vertices.push(GlyphVertex::new([cx + gw, y], [u1, v0], color));
            vertices.push(GlyphVertex::new([cx, y + gh], [u0, v1], color));
            vertices.push(GlyphVertex::new([cx + gw, y], [u1, v0], color));
            vertices.push(GlyphVertex::new([cx + gw, y + gh], [u1, v1], color));
            vertices.push(GlyphVertex::new([cx, y + gh], [u0, v1], color));

            cx += cached.advance_width;
        }
    }
}

fn open_url(url: &str) {
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", url])
            .spawn();
    }
}
