use std::sync::mpsc;
use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::window::WindowId;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::Window;

use alacritty_terminal::event::WindowSize;

use crate::config::Config;
use crate::event::AppEvent;
use crate::input::{TabAction, process_key};
use winit::keyboard::ModifiersState;
use crate::renderer::Renderer;
use crate::tab::Tab;

pub struct App {
    config: Config,
    renderer: Option<Renderer>,
    tabs: Vec<Tab>,
    active_tab: usize,
    window: Option<Arc<Window>>,
    app_sender: mpsc::Sender<AppEvent>,
    app_receiver: mpsc::Receiver<AppEvent>,
    wakeup_proxy: EventLoopProxy<()>,
    mods: ModifiersState,
}

impl App {
    pub fn new(
        config: Config,
        app_sender: mpsc::Sender<AppEvent>,
        app_receiver: mpsc::Receiver<AppEvent>,
        wakeup_proxy: EventLoopProxy<()>,
    ) -> Self {
        Self {
            config,
            renderer: None,
            tabs: Vec::new(),
            active_tab: 0,
            window: None,
            app_sender,
            app_receiver,
            wakeup_proxy,
            mods: ModifiersState::default(),
        }
    }

    fn process_events(&mut self, event_loop: &ActiveEventLoop) {
        while let Ok(event) = self.app_receiver.try_recv() {
            match event {
                AppEvent::Wakeup(_tab_id) => {
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
                AppEvent::Title(tab_id, title) => {
                    if let Some(tab) = self.tabs.get_mut(tab_id) {
                        tab.title = title.clone();
                    }
                    if tab_id == self.active_tab {
                        if let Some(w) = &self.window {
                            let display = if title.is_empty() { "Comet".into() } else { format!("Comet - {title}") };
                            w.set_title(&display);
                        }
                    }
                }
                AppEvent::ResetTitle(tab_id) => {
                    if let Some(tab) = self.tabs.get_mut(tab_id) {
                        tab.title = String::new();
                    }
                    if tab_id == self.active_tab {
                        if let Some(w) = &self.window {
                            w.set_title("Comet");
                        }
                    }
                }
                AppEvent::Exit(tab_id) | AppEvent::ChildExit(tab_id) => {
                    self.close_tab(tab_id, event_loop);
                }
                _ => {}
            }
        }
    }
}

impl ApplicationHandler<()> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_attrs = Window::default_attributes()
            .with_title("Comet")
            .with_inner_size(winit::dpi::PhysicalSize::new(800, 600));
        let window = Arc::new(event_loop.create_window(window_attrs).unwrap());

        let size = window.inner_size();

        let font_size = self.config.font_size;
        let cell_w = font_size * 0.6;
        let cell_h = font_size * 1.4;
        let tab_h = self.config.window.tab_height as f32;

        let win_size = WindowSize {
            num_lines: ((size.height as f32 - tab_h) / cell_h).max(1.0) as u16,
            num_cols: (size.width as f32 / cell_w).max(1.0) as u16,
            cell_width: cell_w as u16,
            cell_height: cell_h as u16,
        };

        let tab = Tab::new(
            0,
            &self.config,
            self.app_sender.clone(),
            self.wakeup_proxy.clone(),
            win_size,
            0,
        );
        if let Some(tab) = tab {
            self.tabs.push(tab);
            self.active_tab = 0;
        }

        let renderer = pollster::block_on(Renderer::new(window.clone(), self.config.clone()));
        self.renderer = Some(renderer);
        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let renderer = match &mut self.renderer {
            Some(r) => r,
            None => return,
        };

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                renderer.resize(size);
                if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                    let cell_w = renderer.glyph_cache.cell_width;
                    let cell_h = renderer.glyph_cache.cell_height;
                    let tab_h = self.config.window.tab_height as f32;
                    let win_size = WindowSize {
                        num_lines: ((size.height as f32 - tab_h) / cell_h).max(1.0) as u16,
                        num_cols: (size.width as f32 / cell_w).max(1.0) as u16,
                        cell_width: cell_w as u16,
                        cell_height: cell_h as u16,
                    };
                    tab.resize(win_size);
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let tab_info: Vec<(String, bool)> = self.tabs.iter().enumerate().map(|(i, t)| {
                    let title = if t.title.is_empty() { "bash".into() } else { t.title.clone() };
                    (title, i == self.active_tab)
                }).collect();

                let term = if self.tabs.is_empty() {
                    None
                } else {
                    Some(&self.tabs[self.active_tab].term)
                };

                renderer.render(term, &tab_info);
            }
            WindowEvent::ModifiersChanged(new_mods) => {
                self.mods = new_mods.state();
            }
            WindowEvent::KeyboardInput {
                event: ke,
                ..
            } if !self.tabs.is_empty() && ke.state.is_pressed() => {
                let (action, bytes) = process_key(
                    ke.physical_key,
                    self.mods,
                    ke.text.as_deref(),
                );

                match action {
                    TabAction::NewTab => self.new_tab(event_loop),
                    TabAction::CloseTab => self.close_tab(self.active_tab, event_loop),
                    TabAction::NextTab => {
                        if !self.tabs.is_empty() {
                            self.active_tab = (self.active_tab + 1) % self.tabs.len();
                            self.window.as_ref().unwrap().request_redraw();
                        }
                    }
                    TabAction::PrevTab => {
                        if !self.tabs.is_empty() {
                            self.active_tab = if self.active_tab == 0 {
                                self.tabs.len() - 1
                            } else {
                                self.active_tab - 1
                            };
                            self.window.as_ref().unwrap().request_redraw();
                        }
                    }
                    TabAction::None => {}
                }

                if let Some(bytes) = bytes {
                    if let Some(tab) = self.tabs.get(self.active_tab) {
                        tab.write_to_pty(&bytes);
                    }
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
            }
            _ => {}
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, _event: ()) {
        self.process_events(event_loop);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.process_events(event_loop);
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.tabs.clear();
        self.renderer = None;
    }
}

impl App {
    fn new_tab(&mut self, _event_loop: &ActiveEventLoop) {
        let id = self.tabs.len();
        let window = match &self.window {
            Some(w) => w,
            None => return,
        };
        let size = window.inner_size();
        let renderer = match &self.renderer {
            Some(r) => r,
            None => return,
        };

        let cell_w = renderer.glyph_cache.cell_width;
        let cell_h = renderer.glyph_cache.cell_height;
        let tab_h = self.config.window.tab_height as f32;

        let win_size = WindowSize {
            num_lines: ((size.height as f32 - tab_h) / cell_h).max(1.0) as u16,
            num_cols: (size.width as f32 / cell_w).max(1.0) as u16,
            cell_width: cell_w as u16,
            cell_height: cell_h as u16,
        };

        if let Some(tab) = Tab::new(
            id,
            &self.config,
            self.app_sender.clone(),
            self.wakeup_proxy.clone(),
            win_size,
            id as u64,
        ) {
            self.tabs.push(tab);
            self.active_tab = id;
            window.request_redraw();
        }
    }

    fn close_tab(&mut self, tab_id: usize, event_loop: &ActiveEventLoop) {
        if tab_id >= self.tabs.len() {
            return;
        }

        self.tabs.remove(tab_id);

        if self.tabs.is_empty() {
            event_loop.exit();
            return;
        }

        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }

        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}
