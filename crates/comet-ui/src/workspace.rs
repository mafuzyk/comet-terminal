use comet_config::Config;

use crate::session::TerminalSession;

pub type PaneId = u64;
pub type TabId = u64;

pub const TAB_BAR_HEIGHT: u32 = 30;
pub const DIVIDER_WIDTH: u32 = 3;

/// A single terminal pane within a tab.
pub struct Pane {
    pub id: PaneId,
    pub session: TerminalSession,
    pub title: String,
}

/// A tab containing one or more panes.
pub struct Tab {
    pub id: TabId,
    pub title: String,
    pub panes: Vec<Pane>,
    pub active_pane: usize,
    /// Relative size proportions for each pane (used in vertical-stack layout
    /// so that divider drags persist across frames). Normalised to sum to 1.0.
    pub pane_ratios: Vec<f32>,
}

/// A viewport rectangle for a pane.
#[derive(Debug, Clone, Copy)]
pub struct PaneViewport {
    pub pane_id: PaneId,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PaneViewport {
    /// Converts to a `comet_renderer::Viewport` for rendering.
    pub fn to_render_viewport(&self) -> comet_renderer::Viewport {
        comet_renderer::Viewport {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }
}

/// Orientation of a divider between panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DividerOrientation {
    Horizontal,
    Vertical,
}

/// A visual divider between two adjacent panes.
#[derive(Debug, Clone, Copy)]
pub struct Divider {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub orientation: DividerOrientation,
}

/// The workspace model: tabs containing panes containing terminal sessions.
///
/// Tree:
///   Workspace
///    └── Tab
///         └── Pane
///              └── TerminalSession
///                   └── PTY
pub struct Workspace {
    pub tabs: Vec<Tab>,
    pub active_tab: usize,
    next_pane_id: PaneId,
    next_tab_id: TabId,
}

impl Workspace {
    /// Creates a new workspace with a single initial tab and pane.
    pub fn new(config: &Config) -> Self {
        let session = TerminalSession::spawn(config);
        Self {
            tabs: vec![Tab {
                id: 1,
                title: "terminal".to_string(),
                panes: vec![Pane {
                    id: 1,
                    session,
                    title: "shell".to_string(),
                }],
                active_pane: 0,
                pane_ratios: vec![1.0],
            }],
            active_tab: 0,
            next_pane_id: 2,
            next_tab_id: 2,
        }
    }

    pub fn active_tab(&self) -> &Tab {
        &self.tabs[self.active_tab]
    }

    pub fn active_tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active_tab]
    }

    pub fn active_pane(&self) -> &Pane {
        let tab = self.active_tab();
        &tab.panes[tab.active_pane]
    }

    pub fn active_pane_mut(&mut self) -> &mut Pane {
        let tab = self.active_tab_mut();
        &mut tab.panes[tab.active_pane]
    }

    pub fn active_session(&self) -> &TerminalSession {
        &self.active_pane().session
    }

    pub fn active_session_mut(&mut self) -> &mut TerminalSession {
        &mut self.active_pane_mut().session
    }

    /// Compute viewports for all panes in the active tab.
    /// Accounts for divider space between adjacent panes.
    /// Uses `pane_ratios` so that divider drags persist across frames.
    pub fn compute_pane_viewports(
        &self,
        window_width: u32,
        window_height: u32,
    ) -> Vec<PaneViewport> {
        let tab = self.active_tab();
        let available_y = TAB_BAR_HEIGHT;
        let available_h = window_height.saturating_sub(available_y);

        if tab.panes.is_empty() {
            return vec![];
        }

        let count = tab.panes.len();
        if count == 1 {
            return vec![PaneViewport {
                pane_id: tab.panes[0].id,
                x: 0,
                y: available_y,
                width: window_width,
                height: available_h,
            }];
        }

        // Multiple panes stacked vertically with divider gaps between them.
        // Sizes are distributed according to pane_ratios.
        let divider_count = count - 1;
        let total_divider_space = divider_count as u32 * DIVIDER_WIDTH;
        let content_h = available_h - total_divider_space;

        let ratio_sum: f32 = tab.pane_ratios.iter().sum();
        let mut y = available_y;
        tab.panes
            .iter()
            .enumerate()
            .map(|(i, pane)| {
                let height = if i == count - 1 {
                    content_h.saturating_sub(y - available_y)
                } else {
                    let h = (content_h as f32 * tab.pane_ratios[i] / ratio_sum) as u32;
                    h.clamp(1, content_h - (count - 1 - i) as u32 * DIVIDER_WIDTH)
                };
                let vp = PaneViewport {
                    pane_id: pane.id,
                    x: 0,
                    y,
                    width: window_width,
                    height,
                };
                y += height + DIVIDER_WIDTH;
                vp
            })
            .collect()
    }

    /// Compute divider positions between adjacent panes in the active tab.
    /// Derives positions from the pane viewport layout.
    pub fn compute_dividers(&self, window_width: u32, window_height: u32) -> Vec<Divider> {
        let viewports = self.compute_pane_viewports(window_width, window_height);
        let count = viewports.len();
        if count <= 1 {
            return vec![];
        }

        let mut dividers = Vec::with_capacity(count - 1);
        for i in 0..count - 1 {
            let a = &viewports[i];
            let b = &viewports[i + 1];

            // Vertical stack: horizontal divider between a and b
            if a.y + a.height < b.y {
                dividers.push(Divider {
                    x: a.x,
                    y: a.y + a.height,
                    width: a.width.min(b.width),
                    height: b.y - (a.y + a.height),
                    orientation: DividerOrientation::Horizontal,
                });
            }
            // Horizontal split: vertical divider between a and b
            else if a.x + a.width < b.x {
                dividers.push(Divider {
                    x: a.x + a.width,
                    y: a.y,
                    width: b.x - (a.x + a.width),
                    height: a.height.min(b.height),
                    orientation: DividerOrientation::Vertical,
                });
            }
        }
        dividers
    }

    /// Creates a new tab with a single pane and switches to it.
    pub fn create_tab(&mut self, config: &Config) -> TabId {
        let tab_id = self.next_tab_id;
        self.next_tab_id += 1;

        let pane_id = self.next_pane_id;
        self.next_pane_id += 1;

        let session = TerminalSession::spawn(config);
        let tab = Tab {
            id: tab_id,
            title: format!("terminal-{}", tab_id),
            panes: vec![Pane {
                id: pane_id,
                session,
                title: "shell".to_string(),
            }],
            active_pane: 0,
            pane_ratios: vec![1.0],
        };
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        tab_id
    }

    /// Closes the active tab. Returns true if the workspace is empty.
    pub fn close_active_tab(&mut self) -> bool {
        if self.tabs.len() <= 1 {
            return false;
        }
        self.tabs.remove(self.active_tab);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        self.tabs.is_empty()
    }

    /// Switches to a tab by index.
    pub fn switch_to_tab(&mut self, idx: usize) -> bool {
        if idx < self.tabs.len() {
            self.active_tab = idx;
            true
        } else {
            false
        }
    }

    /// Switches to the next tab.
    pub fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active_tab = (self.active_tab + 1) % self.tabs.len();
        }
    }

    /// Switches to the previous tab.
    pub fn previous_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active_tab = if self.active_tab == 0 {
                self.tabs.len() - 1
            } else {
                self.active_tab - 1
            };
        }
    }

    /// Splits the active pane horizontally (creates a new pane).
    pub fn split_horizontal(&mut self, config: &Config) -> PaneId {
        self.split_pane(config)
    }

    /// Splits the active pane vertically (creates a new pane).
    pub fn split_vertical(&mut self, config: &Config) -> PaneId {
        self.split_pane(config)
    }

    fn split_pane(&mut self, config: &Config) -> PaneId {
        let pane_id = self.next_pane_id;
        self.next_pane_id += 1;

        let session = TerminalSession::spawn(config);
        let pane = Pane {
            id: pane_id,
            session,
            title: "split".to_string(),
        };

        let tab = self.active_tab_mut();
        tab.panes.push(pane);
        // Split the previous active pane's ratio in half
        let active_idx = tab.active_pane;
        let half = tab.pane_ratios[active_idx] * 0.5;
        tab.pane_ratios[active_idx] = half;
        tab.pane_ratios.push(half);
        tab.active_pane = tab.panes.len() - 1;
        pane_id
    }

    /// Closes the active pane. If it's the last pane, closes the tab.
    /// Move focus to the pane above the active pane.
    pub fn focus_up(&mut self) {
        let tab = self.active_tab_mut();
        if tab.active_pane > 0 {
            tab.active_pane -= 1;
        }
    }

    /// Move focus to the pane below the active pane.
    pub fn focus_down(&mut self) {
        let tab = self.active_tab_mut();
        if tab.active_pane + 1 < tab.panes.len() {
            tab.active_pane += 1;
        }
    }

    /// Move focus to the pane left of the active pane.
    /// For vertical-stack layouts this is a no-op; future horizontal splits
    /// will use viewport adjacency to determine the left neighbour.
    pub fn focus_left(&mut self) {
        // No-op in vertical stack — left neighbour is undefined.
        let _ = self.active_tab_mut();
    }

    /// Move focus to the pane right of the active pane.
    /// For vertical-stack layouts this is a no-op; future horizontal splits
    /// will use viewport adjacency to determine the right neighbour.
    pub fn focus_right(&mut self) {
        // No-op in vertical stack — right neighbour is undefined.
        let _ = self.active_tab_mut();
    }

    /// Adjust pane ratios when dragging divider at `divider_index`
    /// (between pane `divider_index` and `divider_index + 1`) by `delta_y`
    /// pixels.  `window_height` is needed to convert the pixel delta into a
    /// ratio delta.
    pub fn drag_divider(&mut self, divider_index: usize, delta_y: i32, window_height: u32) {
        let tab = self.active_tab_mut();
        if divider_index + 1 >= tab.pane_ratios.len() {
            return;
        }
        let available_h = window_height.saturating_sub(TAB_BAR_HEIGHT) as f32;
        if available_h <= 0.0 {
            return;
        }
        let ratio_delta = delta_y as f32 / available_h;
        let total = tab.pane_ratios[divider_index] + tab.pane_ratios[divider_index + 1];
        let min_ratio = total * 0.05; // at least 5% of the combined space
        let max_ratio = total - min_ratio;

        let mut new_a = (tab.pane_ratios[divider_index] + ratio_delta).clamp(min_ratio, max_ratio);
        let mut new_b = total - new_a;
        // Re-balance so both respect the minimum
        if new_b < min_ratio {
            new_b = min_ratio;
            new_a = total - min_ratio;
        }

        tab.pane_ratios[divider_index] = new_a;
        tab.pane_ratios[divider_index + 1] = new_b;
    }

    pub fn close_active_pane(&mut self) -> bool {
        let tab = self.active_tab_mut();
        if tab.panes.len() <= 1 {
            return false;
        }
        tab.panes.remove(tab.active_pane);
        tab.pane_ratios.remove(tab.active_pane);
        if tab.active_pane >= tab.panes.len() {
            tab.active_pane = tab.panes.len() - 1;
        }
        tab.panes.is_empty()
    }

    /// Processes output for all sessions. Returns true if any had data.
    pub fn process_output(&mut self) -> bool {
        let mut any = false;
        for tab in &mut self.tabs {
            for pane in &mut tab.panes {
                any |= pane.session.process_output();
            }
        }
        any
    }

    /// Removes dead sessions across all tabs/panes. Returns true if any were removed.
    pub fn reap_dead(&mut self) -> bool {
        let mut changed = false;
        for tab in &mut self.tabs {
            let before = tab.panes.len();
            tab.panes.retain(|p| p.session.is_alive());
            if tab.panes.len() != before {
                changed = true;
                if tab.active_pane >= tab.panes.len() && !tab.panes.is_empty() {
                    tab.active_pane = tab.panes.len() - 1;
                }
            }
        }
        self.tabs.retain(|t| !t.panes.is_empty());
        if self.active_tab >= self.tabs.len() && !self.tabs.is_empty() {
            self.active_tab = self.tabs.len() - 1;
        }
        changed
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    pub fn pane_count(&self) -> usize {
        self.tabs.iter().map(|t| t.panes.len()).sum()
    }

    pub fn session_count(&self) -> usize {
        self.pane_count()
    }
}
