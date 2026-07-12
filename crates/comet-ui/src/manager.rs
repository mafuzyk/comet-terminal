use comet_config::Config;

use crate::session::TerminalSession;
use crate::workspace::{PaneId, TabId, Workspace};

/// Manages terminal sessions via the workspace model (tabs → panes → sessions).
///
/// # Architecture
///
/// ```text
/// TerminalManager
///   └── Workspace
///        └── Tab[]
///             └── Pane[]
///                  └── TerminalSession
///                       └── PTY
/// ```
pub struct TerminalManager {
    workspace: Workspace,
}

impl TerminalManager {
    /// Creates a new manager with a single initial tab → pane → session.
    pub fn new(config: &Config) -> Self {
        Self {
            workspace: Workspace::new(config),
        }
    }

    // ── Tab operations ─────────────────────────────────────────────────────────

    /// Creates a new tab with a single pane and makes it active.
    pub fn create_tab(&mut self, config: &Config) -> TabId {
        self.workspace.create_tab(config)
    }

    /// Closes the active tab. Returns true if all tabs are closed.
    pub fn close_active_tab(&mut self) -> bool {
        self.workspace.close_active_tab()
    }

    /// Switches to a tab by index.
    pub fn switch_to_tab(&mut self, idx: usize) -> bool {
        self.workspace.switch_to_tab(idx)
    }

    /// Switches to the next tab.
    pub fn next_tab(&mut self) {
        self.workspace.next_tab();
    }

    /// Switches to the previous tab.
    pub fn previous_tab(&mut self) {
        self.workspace.previous_tab();
    }

    // ── Pane operations ────────────────────────────────────────────────────────

    /// Splits the active pane horizontally.
    pub fn split_horizontal(&mut self, config: &Config) -> PaneId {
        self.workspace.split_horizontal(config)
    }

    /// Splits the active pane vertically.
    pub fn split_vertical(&mut self, config: &Config) -> PaneId {
        self.workspace.split_vertical(config)
    }

    /// Closes the active pane.
    pub fn close_active_pane(&mut self) -> bool {
        self.workspace.close_active_pane()
    }

    // ── Session access (legacy API, delegates to active pane) ──────────────────

    /// Returns the active session.
    pub fn active(&self) -> &TerminalSession {
        self.workspace.active_session()
    }

    /// Returns the active session (mutable).
    pub fn active_mut(&mut self) -> &mut TerminalSession {
        self.workspace.active_session_mut()
    }

    // ── Legacy API compatibility ───────────────────────────────────────────────

    /// Spawns a new session in a new tab and makes it active.
    /// Equivalent to `create_tab()`.
    pub fn create_session(&mut self, config: &Config) -> usize {
        self.create_tab(config);
        self.workspace.tabs.len() - 1
    }

    /// Removes a session (tab) by index. Returns true if it was the last one.
    pub fn remove_session(&mut self, idx: usize) -> bool {
        if idx >= self.workspace.tabs.len() {
            return false;
        }
        let was_active = idx == self.workspace.active_tab;
        self.workspace.tabs.remove(idx);
        if !self.workspace.tabs.is_empty() {
            if was_active && self.workspace.active_tab >= self.workspace.tabs.len() {
                self.workspace.active_tab = self.workspace.tabs.len() - 1;
            }
        }
        self.workspace.tabs.is_empty()
    }

    /// Switches to a tab by index.
    pub fn switch_to(&mut self, idx: usize) -> bool {
        self.switch_to_tab(idx)
    }

    /// Returns the number of tabs.
    pub fn len(&self) -> usize {
        self.workspace.tab_count()
    }

    /// Returns true if there are no tabs.
    pub fn is_empty(&self) -> bool {
        self.workspace.tabs.is_empty()
    }

    /// Processes output for all sessions.
    pub fn process_output(&mut self) -> bool {
        self.workspace.process_output()
    }

    /// Returns an iterator over all sessions.
    pub fn iter(&self) -> impl Iterator<Item = &TerminalSession> {
        self.workspace
            .tabs
            .iter()
            .flat_map(|t| t.panes.iter().map(|p| &p.session))
    }

    /// Returns a mutable iterator over all sessions.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut TerminalSession> {
        self.workspace
            .tabs
            .iter_mut()
            .flat_map(|t| t.panes.iter_mut().map(|p| &mut p.session))
    }

    /// Removes dead sessions. Returns true if any were removed.
    pub fn reap_dead(&mut self) -> bool {
        self.workspace.reap_dead()
    }

    /// Returns a reference to the workspace.
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// Returns a mutable reference to the workspace.
    pub fn workspace_mut(&mut self) -> &mut Workspace {
        &mut self.workspace
    }

    /// Finds a session by pane ID across all tabs.
    pub fn session_by_pane_id(&self, pane_id: PaneId) -> Option<&TerminalSession> {
        for tab in &self.workspace.tabs {
            for pane in &tab.panes {
                if pane.id == pane_id {
                    return Some(&pane.session);
                }
            }
        }
        None
    }

    /// Finds a session by pane ID across all tabs (mutable).
    pub fn session_by_pane_id_mut(&mut self, pane_id: PaneId) -> Option<&mut TerminalSession> {
        for tab in &mut self.workspace.tabs {
            for pane in &mut tab.panes {
                if pane.id == pane_id {
                    return Some(&mut pane.session);
                }
            }
        }
        None
    }

    /// Finds the pane by ID and returns its viewport info.
    pub fn pane_viewport(
        &self,
        pane_id: PaneId,
        window_width: u32,
        window_height: u32,
    ) -> Option<crate::workspace::PaneViewport> {
        for vp in self
            .workspace
            .compute_pane_viewports(window_width, window_height)
        {
            if vp.pane_id == pane_id {
                return Some(vp);
            }
        }
        None
    }
}

impl std::ops::Index<usize> for TerminalManager {
    type Output = TerminalSession;
    fn index(&self, idx: usize) -> &Self::Output {
        &self.workspace.tabs[idx].panes[0].session
    }
}

impl std::ops::IndexMut<usize> for TerminalManager {
    fn index_mut(&mut self, idx: usize) -> &mut Self::Output {
        &mut self.workspace.tabs[idx].panes[0].session
    }
}
