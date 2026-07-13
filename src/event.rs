use std::sync::mpsc;

use alacritty_terminal::event::{Event, EventListener};
use winit::event_loop::EventLoopProxy;

#[derive(Debug)]
pub enum AppEvent {
    Wakeup(usize),
    Title(usize, String),
    ResetTitle(usize),
    Exit(usize),
    Bell(()),
    ChildExit(usize),
}

#[derive(Clone)]
pub struct Proxy {
    pub tab_id: usize,
    pub sender: mpsc::Sender<AppEvent>,
    pub wakeup: EventLoopProxy<()>,
}

impl EventListener for Proxy {
    fn send_event(&self, event: Event) {
        let app_event = match event {
            Event::Wakeup => AppEvent::Wakeup(self.tab_id),
            Event::Title(title) => AppEvent::Title(self.tab_id, title),
            Event::ResetTitle => AppEvent::ResetTitle(self.tab_id),
            Event::Exit => AppEvent::Exit(self.tab_id),
            Event::Bell => AppEvent::Bell(()),
            Event::ChildExit(_status) => AppEvent::ChildExit(self.tab_id),
            _ => return,
        };
        let _ = self.sender.send(app_event);
        let _ = self.wakeup.send_event(());
    }
}
