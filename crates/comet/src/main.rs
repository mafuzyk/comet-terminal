use std::sync::Arc;

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
            let window = Arc::new(
                event_loop
                    .create_window(
                        WindowAttributes::default()
                            .with_title("Comet Terminal")
                            .with_inner_size(winit::dpi::LogicalSize::new(800, 600)),
                    )
                    .expect("Failed to create window"),
            );
            let terminal_app = TerminalApp::new(window, 14);
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
        if let Some(mut app) = self.app.take() {
            let _ = app.pty_mut().kill();
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    let mut app = CometApp { app: None };
    event_loop.run_app(&mut app)?;
    Ok(())
}
