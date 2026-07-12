use std::io::Read;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use comet_config::Config;
use comet_core::Terminal;
use comet_pty::{AnsiParser, PtyConfig, PtyProcess};

/// A single terminal session: PTY + terminal state + reader thread.
///
/// Does NOT own a renderer — rendering is coordinated by TerminalApp
/// through the shared MultiRenderer.
pub struct TerminalSession {
    terminal: Terminal,
    pty: PtyProcess,
    pty_rx: Receiver<Vec<u8>>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl TerminalSession {
    /// Spawns a new PTY and creates the terminal state.
    pub fn spawn(config: &Config) -> Self {
        let pty_config = PtyConfig {
            cols: 80,
            rows: 24,
            // Pass -i so bash/zsh run as interactive shells and print a
            // prompt. Without it they are non-interactive and produce no
            // output, leaving the terminal grid completely empty.
            args: vec!["-i".to_string()],
            ..PtyConfig::default()
        };
        let pty = PtyProcess::spawn(pty_config).expect("Failed to spawn PTY");

        let bg_reader = pty
            .pair()
            .master
            .try_clone_reader()
            .expect("Failed to clone PTY reader");

        let scrollback_size = config.terminal.scrollback.max(100);
        let terminal = Terminal::with_scrollback(80, 24, scrollback_size);

        let (pty_tx, pty_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let mut reader = bg_reader;
            let mut buf = vec![0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        if pty_tx.send(data).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        eprintln!("PTY read error: {}", e);
                        break;
                    }
                }
            }
        });

        Self {
            terminal,
            pty,
            pty_rx,
            thread_handle: Some(handle),
        }
    }

    /// Processes all pending PTY output. Returns true if any data was processed.
    pub fn process_output(&mut self) -> bool {
        let mut any_data = false;
        while let Ok(data) = self.pty_rx.try_recv() {
            any_data = true;
            let mut parser = AnsiParser::new(&mut self.terminal);
            parser.feed(&data);
        }
        any_data
    }

    /// Writes bytes to the PTY (keyboard input).
    pub fn write_input(&mut self, data: &[u8]) {
        if let Err(e) = self.pty.writer().write_all(data) {
            eprintln!("PTY write error: {}", e);
        }
    }

    /// Resizes the PTY and terminal grid. Does NOT resize a renderer
    /// (that's handled by TerminalApp for the shared renderer).
    pub fn resize(&mut self, width: u32, height: u32, cell_size: (u32, u32)) {
        let cols = (width / cell_size.0.max(1)).max(1);
        let rows = (height / cell_size.1.max(1)).max(1);

        let pty_size = portable_pty::PtySize {
            rows: rows as u16,
            cols: cols as u16,
            pixel_width: width as u16,
            pixel_height: height as u16,
        };
        if let Err(e) = self.pty.resize(pty_size) {
            eprintln!("PTY resize error: {}", e);
        }

        self.terminal.resize(cols as usize, rows as usize);
    }

    /// Returns true if the background reader thread is still alive.
    pub fn is_alive(&self) -> bool {
        self.thread_handle.is_some()
    }

    pub fn terminal(&self) -> &Terminal {
        &self.terminal
    }

    pub fn terminal_mut(&mut self) -> &mut Terminal {
        &mut self.terminal
    }

    pub fn pty_mut(&mut self) -> &mut PtyProcess {
        &mut self.pty
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.pty.kill();
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
        let _ = self.pty.wait();
    }
}
