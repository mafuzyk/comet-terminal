use std::borrow::Cow;
use std::sync::Arc;
use std::thread::JoinHandle;

use alacritty_terminal::event::{WindowSize, Notify};
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::event_loop::{EventLoop, Notifier, Msg};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Term, Config as TermConfig};
use alacritty_terminal::tty;
use winit::event_loop::EventLoopProxy;

use crate::event::{Proxy, AppEvent};

pub struct Tab {
    pub term: Arc<FairMutex<Term<Proxy>>>,
    pub notifier: Notifier,
    pub channel: alacritty_terminal::event_loop::EventLoopSender,
    pub title: String,
    pub size: WindowSize,
    _join_handle: JoinHandle<(EventLoop<tty::Pty, Proxy>, alacritty_terminal::event_loop::State)>,
}

impl Tab {
    pub fn new(
        id: usize,
        _config: &crate::config::Config,
        app_sender: std::sync::mpsc::Sender<AppEvent>,
        wakeup_proxy: EventLoopProxy<()>,
        window_size: WindowSize,
        window_id: u64,
    ) -> Option<Self> {
        tty::setup_env();

        let event_proxy = Proxy { tab_id: id, sender: app_sender, wakeup: wakeup_proxy };

        let term_config = TermConfig {
            scrolling_history: 10000,
            ..TermConfig::default()
        };

        let term_size = TermSize::new(window_size.num_cols as usize, window_size.num_lines as usize);
        let term = Arc::new(FairMutex::new(Term::new(
            term_config,
            &term_size,
            event_proxy.clone(),
        )));

        let pty_options = tty::Options {
            shell: None,
            working_directory: None,
            drain_on_exit: true,
            env: std::collections::HashMap::new(),
        };

        let pty = match tty::new(&pty_options, window_size, window_id) {
            Ok(pty) => pty,
            Err(e) => {
                log::error!("Failed to create PTY: {e}");
                return None;
            }
        };

        let event_loop = match EventLoop::new(Arc::clone(&term), event_proxy, pty, true, false) {
            Ok(el) => el,
            Err(e) => {
                log::error!("Failed to create event loop: {e}");
                return None;
            }
        };

        let channel = event_loop.channel();
        let notifier = Notifier(channel.clone());
        let join_handle = event_loop.spawn();

        Some(Self {
            term,
            notifier,
            channel,
            title: String::new(),
            size: window_size,
            _join_handle: join_handle,
        })
    }

    pub fn write_to_pty(&self, bytes: &[u8]) {
        let cow: Cow<'static, [u8]> = Cow::Owned(bytes.to_vec());
        self.notifier.notify(cow);
    }

    pub fn resize(&mut self, size: WindowSize) {
        self.size = size;
        let _ = self.channel.send(Msg::Resize(size));
        let term_size = TermSize::new(size.num_cols as usize, size.num_lines as usize);
        self.term.lock().resize(term_size);
    }
}
