use comet_core::Terminal;
use comet_renderer::{
    cursor::CursorShape, BackendType, HasWindowHandle, Renderer, RendererConfig,
};
use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    raw_window_handle::{DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle as RawHasWindowHandle},
    window::{Window, WindowAttributes},
};

/// Wraps an `Arc<Window>` so it can be used as a `Box<dyn HasWindowHandle>`.
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

struct DemoApp {
    renderer: Option<Renderer>,
    terminal: Terminal,
    window: Option<Arc<Window>>,
}

impl DemoApp {
    fn new() -> Self {
        Self {
            renderer: None,
            terminal: Terminal::new(80, 24),
            window: None,
        }
    }
}

impl ApplicationHandler for DemoApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window_attrs = WindowAttributes::default()
                .with_title("Comet Terminal Renderer Demo")
                .with_inner_size(winit::dpi::LogicalSize::new(800, 600));

            let window = Arc::new(event_loop.create_window(window_attrs).unwrap());
            let size = window.inner_size();
            self.window = Some(window.clone());

            // Initialize renderer with window handle
            let config = RendererConfig {
                backend: BackendType::Wgpu,
                font_family: "Monospace".to_string(),
                font_size: 14,
                theme: "dark".to_string(),
                dpi_scale: 1.0,
                padding_x: 2.0,
                padding_y: 2.0,
                cursor_blink: true,
                cursor_shape: CursorShape::Block,
            };

            let mut renderer = Renderer::new(config).unwrap();
            let handle: Box<dyn HasWindowHandle> = Box::new(WindowOwner(window));
            renderer
                .initialize(size.width, size.height, Some(handle))
                .unwrap();
            self.renderer = Some(renderer);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(physical_size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer
                        .resize(physical_size.width, physical_size.height)
                        .unwrap();
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state,
                        ..
                    },
                ..
            } => {
                if state == ElementState::Pressed {
                    match code {
                        KeyCode::Escape => {
                            event_loop.exit();
                        }
                        KeyCode::KeyC => {
                            self.terminal = Terminal::new(80, 24);
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(renderer) = &mut self.renderer {
                    if let Err(e) = renderer.render(&self.terminal) {
                        eprintln!("Render error: {}", e);
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        self.terminal.write("Hello Comet Terminal!\n");
        self.terminal.write("Testing Unicode: A Z *\n");
        self.terminal
            .write("\x1b[31mRed\x1b[0m \x1b[32mGreen\x1b[0m \x1b[34mBlue\x1b[0m\n");
        self.terminal
            .write("\x1b[1mBold\x1b[0m \x1b[3mItalic\x1b[0m \x1b[4mUnderline\x1b[0m\n");

        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    let mut app = DemoApp::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}
