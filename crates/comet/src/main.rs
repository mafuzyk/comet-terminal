use std::sync::Arc;

use comet_config::{Session, ensure_default_themes, load_config, load_session, save_session};
use comet_ui::TerminalApp;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::WindowAttributes,
};

mod icon;

struct CometApp {
    app: Option<TerminalApp>,
}

impl ApplicationHandler for CometApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.app.is_some() {
            return;
        }

        // Ensure default themes exist
        if let Err(e) = ensure_default_themes() {
            eprintln!("Failed to initialize default themes: {}", e);
        }
        let config = load_config().unwrap_or_else(|e| {
            eprintln!("Failed to load config: {}", e);
            comet_config::Config::default()
        });
        let session = load_session();
        let window_size = winit::dpi::LogicalSize::new(
            session.window_width as f64,
            session.window_height as f64,
        );
        let window = Arc::new(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title("Comet Terminal")
                        .with_window_icon(icon::load_app_icon())
                        .with_inner_size(window_size),
                )
                .expect("Failed to create window"),
        );

        // Restore window position if saved
        if let (Some(x), Some(y)) = (session.window_x, session.window_y) {
            let _ = window.set_outer_position(winit::dpi::LogicalPosition::new(x, y));
        }

        let terminal_app = TerminalApp::new(window, config, session);
        self.app = Some(terminal_app);
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Some(app) = &mut self.app else {
            return;
        };
        match &event {
            WindowEvent::CloseRequested => {
                _event_loop.exit();
            }
            _ => {
                app.handle_window_event(&event);
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let Some(app) = &mut self.app else {
            return;
        };
        app.process_pty_output();
        if app.needs_redraw() {
            app.request_redraw();
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // Save session before exiting
        if let Some(app) = &self.app {
            let size = app.window().inner_size();
            let pos = app.window().outer_position().ok();
            let session = Session {
                window_width: size.width,
                window_height: size.height,
                window_x: pos.map(|p| p.x),
                window_y: pos.map(|p| p.y),
                font_family: app.config().font.family.clone(),
                font_size: app.config().font.size,
                theme: app.config().theme.name.clone(),
            };
            save_session(&session);
        }
        // Drop the TerminalApp — its Drop impl handles PTY kill, thread
        // join, and child reaping.
        self.app.take();
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    let mut app = CometApp { app: None };
    event_loop.run_app(&mut app)?;
    Ok(())
}
