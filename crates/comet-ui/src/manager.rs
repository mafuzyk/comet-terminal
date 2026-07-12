//! Terminal session manager — owns all sessions and handles lifecycle.

use comet_config::Config;

use crate::session::TerminalSession;

/// Manages multiple terminal sessions (tabs/panes).
pub struct TerminalManager {
    sessions: Vec<TerminalSession>,
    active_idx: usize,
}

impl TerminalManager {
    /// Creates a new manager with a single initial session.
    pub fn new(config: &Config) -> Self {
        let session = TerminalSession::spawn(config);
        Self {
            sessions: vec![session],
            active_idx: 0,
        }
    }

    /// Returns the active session.
    pub fn active(&self) -> &TerminalSession {
        &self.sessions[self.active_idx]
    }

    /// Returns the active session (mutable).
    pub fn active_mut(&mut self) -> &mut TerminalSession {
        &mut self.sessions[self.active_idx]
    }

    /// Spawns a new session and makes it active.
    pub fn create_session(&mut self, config: &Config) -> usize {
        let idx = self.sessions.len();
        let session = TerminalSession::spawn(config);
        self.sessions.push(session);
        self.active_idx = idx;
        idx
    }

    /// Removes a session by index. Returns true if it was the last one.
    pub fn remove_session(&mut self, idx: usize) -> bool {
        if idx >= self.sessions.len() {
            return false;
        }
        // Adjust active index before removing
        if self.active_idx >= idx && self.active_idx > 0 {
            self.active_idx -= 1;
        }
        self.sessions.remove(idx);
        self.sessions.is_empty()
    }

    /// Switches to a session by index.
    pub fn switch_to(&mut self, idx: usize) -> bool {
        if idx < self.sessions.len() {
            self.active_idx = idx;
            true
        } else {
            false
        }
    }

    /// Returns the number of sessions.
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Returns true if there are no sessions.
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// Processes output for all sessions. Returns true if any had data.
    pub fn process_output(&mut self) -> bool {
        let mut any = false;
        for session in &mut self.sessions {
            any |= session.process_output();
        }
        any
    }

    /// Returns an iterator over all sessions.
    pub fn iter(&self) -> impl Iterator<Item = &TerminalSession> {
        self.sessions.iter()
    }

    /// Returns a mutable iterator over all sessions.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut TerminalSession> {
        self.sessions.iter_mut()
    }

    /// Removes dead sessions. Returns true if any were removed.
    pub fn reap_dead(&mut self) -> bool {
        let before = self.sessions.len();
        self.sessions.retain(|s| s.is_alive());
        if self.active_idx >= self.sessions.len() && !self.sessions.is_empty() {
            self.active_idx = self.sessions.len() - 1;
        }
        self.sessions.len() != before
    }
}
