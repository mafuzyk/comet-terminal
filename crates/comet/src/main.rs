use std::sync::Arc;

use comet_config::{load_config, ensure_default_themes};
use comet_ui::TerminalApp;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::WindowAttributes,
};

struct CometApp {
    app: Option<TerminalApp>,
}

impl ApplicationHandler for CometApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.app.is_none() {
            // Ensure default themes exist
            if let Err(e) = ensure_default_themes() {
                eprintln!("Failed to initialize default themes: {}", e);
            }
            let config = load_config().unwrap_or_else(|e| {
                eprintln!("Failed to load config: {}", e);
                comet_config::Config::default()
            });
            let window = Arc::new(
                event_loop
                    .create_window(
                        WindowAttributes::default()
                            .with_title("Comet Terminal")
                            .with_inner_size(winit::dpi::LogicalSize::new(800, 600)),
                    )
                    .expect("Failed to create window"),
            );
            let terminal_app = TerminalApp::new(window, config);
            self.app = Some(terminal_app);
        }
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
