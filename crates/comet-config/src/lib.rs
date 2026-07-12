//! Configuration management for Comet Terminal.
//!
//! Handles loading, validation, and defaults for user configuration.

use std::path::PathBuf;

use directories::ProjectDirs;
use notify::{
    Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{self, Receiver};
use thiserror::Error;

/// Errors that can occur during configuration loading.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to determine config directory: {0}")]
    ConfigDir(String),
    #[error("Failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse config TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("Invalid configuration: {0}")]
    Validation(String),
}

/// Result type for config operations.
pub type ConfigResult<T> = Result<T, ConfigError>;

/// Complete Comet Terminal configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub font: FontConfig,
    #[serde(default)]
    pub colors: ColorsConfig,
    #[serde(default)]
    pub window: WindowConfig,
    #[serde(default)]
    pub cursor: CursorConfig,
    #[serde(default)]
    pub renderer: RendererConfig,
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default)]
    pub terminal: TerminalConfig,
    #[serde(default)]
    pub behavior: BehaviorConfig,
    #[serde(default)]
    pub appearance: AppearanceConfig,
    #[serde(default)]
    pub shortcuts: ShortcutsConfig,
    #[serde(default)]
    pub debug: DebugConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            font: FontConfig::default(),
            colors: ColorsConfig::default(),
            window: WindowConfig::default(),
            cursor: CursorConfig::default(),
            renderer: RendererConfig::default(),
            theme: ThemeConfig::default(),
            terminal: TerminalConfig::default(),
            behavior: BehaviorConfig::default(),
            appearance: AppearanceConfig::default(),
            shortcuts: ShortcutsConfig::default(),
            debug: DebugConfig::default(),
        }
    }
}

/// Font configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontConfig {
    /// Font family name (e.g., "JetBrains Mono", "Fira Code", "Monospace").
    pub family: String,
    /// Font size in points.
    pub size: u16,
    /// Enable font ligatures (requires font support).
    #[serde(default = "default_true")]
    pub ligatures: bool,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: "JetBrains Mono".to_string(),
            size: 14,
            ligatures: true,
        }
    }
}

/// Color configuration using hex strings (e.g., "#1e1e2e" or "#ffffff").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColorsConfig {
    /// Default background color.
    pub background: String,
    /// Default foreground (text) color.
    pub foreground: String,
    /// Cursor color.
    pub cursor: String,
    /// Selection highlight color.
    pub selection: String,
    /// ANSI color palette (16 colors). Optional - uses theme defaults if not specified.
    #[serde(default)]
    pub ansi: Option<AnsiColors>,
}

/// ANSI 16-color palette.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnsiColors {
    pub black: String,
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub blue: String,
    pub magenta: String,
    pub cyan: String,
    pub white: String,
    pub bright_black: String,
    pub bright_red: String,
    pub bright_green: String,
    pub bright_yellow: String,
    pub bright_blue: String,
    pub bright_magenta: String,
    pub bright_cyan: String,
    pub bright_white: String,
}

impl AnsiColors {
    fn validate(&self) -> ConfigResult<()> {
        for (name, color) in [
            ("black", &self.black),
            ("red", &self.red),
            ("green", &self.green),
            ("yellow", &self.yellow),
            ("blue", &self.blue),
            ("magenta", &self.magenta),
            ("cyan", &self.cyan),
            ("white", &self.white),
            ("bright_black", &self.bright_black),
            ("bright_red", &self.bright_red),
            ("bright_green", &self.bright_green),
            ("bright_yellow", &self.bright_yellow),
            ("bright_blue", &self.bright_blue),
            ("bright_magenta", &self.bright_magenta),
            ("bright_cyan", &self.bright_cyan),
            ("bright_white", &self.bright_white),
        ] {
            if !color.starts_with('#') || color.len() != 7 {
                return Err(ConfigError::Validation(format!(
                    "colors.ansi.{} must be a 6-digit hex color",
                    name
                )));
            }
            if !color[1..].chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(ConfigError::Validation(format!(
                    "colors.ansi.{} contains invalid hex digits",
                    name
                )));
            }
        }
        Ok(())
    }
}

impl Default for ColorsConfig {
    fn default() -> Self {
        // Mochi Galaxy — Comet's signature dark space theme
        Self {
            background: "#0B1020".to_string(),
            foreground: "#D8E4FF".to_string(),
            cursor: "#8BE9FD".to_string(),
            selection: "#263B66".to_string(),
            ansi: None,
        }
    }
}

/// Window configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowConfig {}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {}
    }
}

/// Cursor configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CursorConfig {
    /// Cursor style: "block", "beam", "underline", "hollow_block", "bar".
    #[serde(default = "default_cursor_style")]
    pub style: String,
    /// Whether the cursor should blink.
    #[serde(default = "default_true")]
    pub blink: bool,
    /// Blink interval in milliseconds.
    #[serde(default = "default_blink_interval")]
    pub blink_interval: u64,
}

fn default_cursor_style() -> String {
    "block".to_string()
}

fn default_true() -> bool {
    true
}

fn default_blink_interval() -> u64 {
    500
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self {
            style: "block".to_string(),
            blink: true,
            blink_interval: 500,
        }
    }
}

/// Renderer configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendererConfig {
    /// Enable VSync.
    #[serde(default = "default_true")]
    pub vsync: bool,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self { vsync: true }
    }
}

/// Terminal behavior configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalConfig {
    /// Maximum number of lines to keep in scrollback history.
    #[serde(default = "default_scrollback")]
    pub scrollback: usize,
    /// Automatically copy selection to clipboard.
    #[serde(default)]
    pub copy_on_select: bool,
    /// Paste on middle click.
    #[serde(default = "default_true")]
    pub middle_click_paste: bool,
    /// Enable terminal bell (BEL character) visual feedback.
    #[serde(default = "default_true")]
    pub bell: bool,
}

fn default_scrollback() -> usize {
    10000
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            scrollback: 10000,
            copy_on_select: false,
            middle_click_paste: true,
            bell: true,
        }
    }
}

/// Behavior configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorConfig {
    /// Copy selection to clipboard automatically on selection.
    #[serde(default)]
    pub copy_on_select: bool,
    /// Show confirmation dialog when closing a window with multiple tabs.
    #[serde(default)]
    pub confirm_close: bool,
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            copy_on_select: false,
            confirm_close: false,
        }
    }
}

/// Window appearance configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppearanceConfig {
    /// Window opacity (0.0 = transparent, 1.0 = opaque).
    #[serde(default = "default_float_one")]
    pub opacity: f32,
    /// Enable background blur (requires compositor support).
    #[serde(default)]
    pub blur: bool,
}

fn default_float_one() -> f32 {
    1.0
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            opacity: 0.95,
            blur: true,
        }
    }
}

/// Keyboard shortcut configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutsConfig {
    /// Keybinding for search (e.g., "CTRL_SHIFT_F").
    #[serde(default = "default_search_shortcut")]
    pub search: String,
}

fn default_search_shortcut() -> String {
    "CTRL_SHIFT_F".to_string()
}

impl Default for ShortcutsConfig {
    fn default() -> Self {
        Self {
            search: default_search_shortcut(),
        }
    }
}

/// Debug/Diagnostics configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugConfig {
    /// Show FPS and diagnostics overlay.
    #[serde(default)]
    pub show_fps: bool,
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self { show_fps: false }
    }
}

/// Theme configuration section in config.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    /// Theme name to load from ~/.config/comet/themes/
    #[serde(default = "default_theme_name")]
    pub name: String,
}

fn default_theme_name() -> String {
    "mochi-galaxy".to_string()
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: default_theme_name(),
        }
    }
}

/// A complete theme with all colors defined.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub background: String,
    pub foreground: String,
    pub cursor: String,
    pub selection: String,
    pub ansi: AnsiColors,
}

impl Theme {
    /// Load a theme from the themes directory.
    pub fn load(name: &str) -> ConfigResult<Self> {
        let themes_dir = themes_dir()?;
        let path = themes_dir.join(format!("{}.toml", name));
        if !path.exists() {
            return Err(ConfigError::Validation(format!(
                "Theme '{}' not found at {}",
                name,
                path.display()
            )));
        }
        let content = std::fs::read_to_string(&path)?;
        let mut theme: Theme = toml::from_str(&content)?;
        theme.name = name.to_string();
        theme.ansi.validate()?;
        Ok(theme)
    }

    /// Get the list of available theme names.
    pub fn list_available() -> ConfigResult<Vec<String>> {
        let themes_dir = themes_dir()?;
        if !themes_dir.exists() {
            return Ok(vec![]);
        }
        let mut themes = Vec::new();
        for entry in std::fs::read_dir(themes_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "toml") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    themes.push(name.to_string());
                }
            }
        }
        themes.sort();
        Ok(themes)
    }
}

/// Get the themes directory path.
fn themes_dir() -> ConfigResult<PathBuf> {
    let config_dir = config_dir()?;
    Ok(config_dir.join("themes"))
}

/// Create default themes in the themes directory.
/// Call this on first run to populate ~/.config/comet/themes/ with built-in themes.
pub fn ensure_default_themes() -> ConfigResult<()> {
    let themes_dir = themes_dir()?;
    std::fs::create_dir_all(&themes_dir)?;

    // Mochi Galaxy — Comet's signature theme (deep space / nebula palette)
    let mochi_galaxy = r###"name = "mochi-galaxy"
background = "#0B1020"
foreground = "#D8E4FF"
cursor = "#8BE9FD"
selection = "#263B66"

[ansi]
black = "#101626"
red = "#FF6B8A"
green = "#8AFFC1"
yellow = "#FFE28A"
blue = "#7AA2FF"
magenta = "#D8A7FF"
cyan = "#7DEBFF"
white = "#D8E4FF"
bright_black = "#35405C"
bright_red = "#FF8FA3"
bright_green = "#B0FFD8"
bright_yellow = "#FFF0B8"
bright_blue = "#A8C1FF"
bright_magenta = "#E8C8FF"
bright_cyan = "#B5F4FF"
bright_white = "#FFFFFF"
"###;

    let galaxy_path = themes_dir.join("mochi-galaxy.toml");
    if !galaxy_path.exists() {
        std::fs::write(&galaxy_path, mochi_galaxy)?;
    }

    // Mochi Dark theme (legacy — Comet's first theme)
    let mochi_dark = r###"name = "mochi-dark"
background = "#1a1a2e"
foreground = "#e0def4"
cursor = "#ebbcba"
selection = "#403d52"

[ansi]
black = "#191724"
red = "#eb6f92"
green = "#31748f"
yellow = "#f6c177"
blue = "#9ccfd8"
magenta = "#c4a7e7"
cyan = "#9ccfd8"
white = "#e0def4"
bright_black = "#6e6a86"
bright_red = "#eb6f92"
bright_green = "#31748f"
bright_yellow = "#f6c177"
bright_blue = "#9ccfd8"
bright_magenta = "#c4a7e7"
bright_cyan = "#9ccfd8"
bright_white = "#e0def4"
"###;

    let mochi_path = themes_dir.join("mochi-dark.toml");
    if !mochi_path.exists() {
        std::fs::write(&mochi_path, mochi_dark)?;
    }

    // Catppuccin Mocha theme
    let catppuccin_mocha = r###"name = "catppuccin-mocha"
background = "#1e1e2e"
foreground = "#cdd6f4"
cursor = "#f5e0dc"
selection = "#45475a"

[ansi]
black = "#1e1e2e"
red = "#f38ba8"
green = "#a6e3a1"
yellow = "#f9e2af"
blue = "#89b4fa"
magenta = "#f5c2e7"
cyan = "#94e2d5"
white = "#bac2de"
bright_black = "#585b70"
bright_red = "#f38ba8"
bright_green = "#a6e3a1"
bright_yellow = "#f9e2af"
bright_blue = "#89b4fa"
bright_magenta = "#f5c2e7"
bright_cyan = "#94e2d5"
bright_white = "#a6adc8"
"###;

    let catppuccin_path = themes_dir.join("catppuccin-mocha.toml");
    if !catppuccin_path.exists() {
        std::fs::write(&catppuccin_path, catppuccin_mocha)?;
    }

    // Tokyo Night theme
    let tokyo_night = r###"name = "tokyo-night"
background = "#1a1b26"
foreground = "#a9b1d6"
cursor = "#7aa2f7"
selection = "#33467c"

[ansi]
black = "#15161e"
red = "#f7768e"
green = "#9ece6a"
yellow = "#e0af68"
blue = "#7aa2f7"
magenta = "#bb9af7"
cyan = "#7dcfff"
white = "#a9b1d6"
bright_black = "#414868"
bright_red = "#f7768e"
bright_green = "#9ece6a"
bright_yellow = "#e0af68"
bright_blue = "#7aa2f7"
bright_magenta = "#bb9af7"
bright_cyan = "#7dcfff"
bright_white = "#c0caf5"
"###;

    let tokyo_path = themes_dir.join("tokyo-night.toml");
    if !tokyo_path.exists() {
        std::fs::write(&tokyo_path, tokyo_night)?;
    }

    Ok(())
}

/// Get the list of available theme names.
pub fn list_available_themes() -> ConfigResult<Vec<String>> {
    let themes_dir = themes_dir()?;
    if !themes_dir.exists() {
        return Ok(vec![]);
    }
    let mut themes = Vec::new();
    for entry in std::fs::read_dir(themes_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "toml") {
            if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                themes.push(name.to_string());
            }
        }
    }
    themes.sort();
    Ok(themes)
}

/// Resolve colors using priority: explicit config > theme > defaults.
impl ColorsConfig {
    /// Resolve final colors, applying theme if configured.
    pub fn resolve(&self, theme_config: &ThemeConfig) -> Self {
        if let Some(theme) = Theme::load(&theme_config.name).ok() {
            let mut resolved = self.clone();
            // Only override if user didn't explicitly set (we can't easily detect this,
            // so theme provides fallback when ansi is None and user uses default colors)
            if resolved.ansi.is_none() {
                resolved.ansi = Some(theme.ansi.clone());
            }
            // For main colors, if they match defaults, use theme values
            let defaults = ColorsConfig::default();
            if resolved.background == defaults.background {
                resolved.background = theme.background;
            }
            if resolved.foreground == defaults.foreground {
                resolved.foreground = theme.foreground;
            }
            if resolved.cursor == defaults.cursor {
                resolved.cursor = theme.cursor;
            }
            if resolved.selection == defaults.selection {
                resolved.selection = theme.selection;
            }
            resolved
        } else {
            self.clone()
        }
    }
}

/// Load configuration from the default config file location.
///
/// Path: `~/.config/comet/config.toml` (Linux)
///       `~/Library/Application Support/comet/config.toml` (macOS)
///       `C:\Users\<user>\AppData\Roaming\comet\config.toml` (Windows)
pub fn load_config() -> ConfigResult<Config> {
    let config_path = config_path()?;
    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        let mut config: Config = toml::from_str(&content)?;
        config.validate()?;
        // Resolve colors with theme
        config.colors = config.colors.resolve(&config.theme);
        Ok(config)
    } else {
        Ok(Config::default())
    }
}

/// Load configuration from a specific file path.
pub fn load_config_from(path: &std::path::Path) -> ConfigResult<Config> {
    let content = std::fs::read_to_string(path)?;
    let mut config: Config = toml::from_str(&content)?;
    config.validate()?;
    // Resolve colors with theme
    config.colors = config.colors.resolve(&config.theme);
    Ok(config)
}

/// Get the default config file path.
pub fn config_path() -> ConfigResult<PathBuf> {
    let proj = ProjectDirs::from("com", "comet-terminal", "comet").ok_or_else(|| {
        ConfigError::ConfigDir("Could not determine config directory".to_string())
    })?;
    Ok(proj.config_dir().join("config.toml"))
}

/// Get the config directory path.
pub fn config_dir() -> ConfigResult<PathBuf> {
    let proj = ProjectDirs::from("com", "comet-terminal", "comet").ok_or_else(|| {
        ConfigError::ConfigDir("Could not determine config directory".to_string())
    })?;
    Ok(proj.config_dir().to_path_buf())
}

impl Config {
    /// Validate the configuration values.
    fn validate(&mut self) -> ConfigResult<()> {
        // Font size bounds
        if self.font.size == 0 {
            return Err(ConfigError::Validation("font.size must be > 0".to_string()));
        }
        if self.font.size > 200 {
            return Err(ConfigError::Validation(
                "font.size must be <= 200".to_string(),
            ));
        }

        // Opacity bounds
        if !(0.0..=1.0).contains(&self.appearance.opacity) {
            return Err(ConfigError::Validation(
                "appearance.opacity must be between 0.0 and 1.0".to_string(),
            ));
        }

        // Validate color hex format
        self.validate_color("colors.background", &self.colors.background)?;
        self.validate_color("colors.foreground", &self.colors.foreground)?;
        self.validate_color("colors.cursor", &self.colors.cursor)?;
        self.validate_color("colors.selection", &self.colors.selection)?;

        if let Some(ansi) = &self.colors.ansi {
            self.validate_color("colors.ansi.black", &ansi.black)?;
            self.validate_color("colors.ansi.red", &ansi.red)?;
            self.validate_color("colors.ansi.green", &ansi.green)?;
            self.validate_color("colors.ansi.yellow", &ansi.yellow)?;
            self.validate_color("colors.ansi.blue", &ansi.blue)?;
            self.validate_color("colors.ansi.magenta", &ansi.magenta)?;
            self.validate_color("colors.ansi.cyan", &ansi.cyan)?;
            self.validate_color("colors.ansi.white", &ansi.white)?;
            self.validate_color("colors.ansi.bright_black", &ansi.bright_black)?;
            self.validate_color("colors.ansi.bright_red", &ansi.bright_red)?;
            self.validate_color("colors.ansi.bright_green", &ansi.bright_green)?;
            self.validate_color("colors.ansi.bright_yellow", &ansi.bright_yellow)?;
            self.validate_color("colors.ansi.bright_blue", &ansi.bright_blue)?;
            self.validate_color("colors.ansi.bright_magenta", &ansi.bright_magenta)?;
            self.validate_color("colors.ansi.bright_cyan", &ansi.bright_cyan)?;
            self.validate_color("colors.ansi.bright_white", &ansi.bright_white)?;
        }

        // Cursor style validation
        let valid_styles = ["block", "beam", "underline", "hollow_block", "bar"];
        if !valid_styles.contains(&self.cursor.style.as_str()) {
            return Err(ConfigError::Validation(format!(
                "cursor.style must be one of: {}",
                valid_styles.join(", ")
            )));
        }

        // Terminal config validation
        self.validate_terminal()?;

        Ok(())
    }

    /// Validate terminal configuration.
    fn validate_terminal(&self) -> ConfigResult<()> {
        if self.terminal.scrollback == 0 {
            return Err(ConfigError::Validation(
                "terminal.scrollback must be > 0".to_string(),
            ));
        }
        if self.terminal.scrollback > 1000000 {
            return Err(ConfigError::Validation(
                "terminal.scrollback must be <= 1000000".to_string(),
            ));
        }
        Ok(())
    }

    fn validate_color(&self, name: &str, color: &str) -> ConfigResult<()> {
        if !color.starts_with('#') || color.len() != 7 {
            return Err(ConfigError::Validation(format!(
                "{} must be a 6-digit hex color (e.g., '#rrggbb')",
                name
            )));
        }
        // Validate hex digits
        if !color[1..].chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ConfigError::Validation(format!(
                "{} contains invalid hex digits",
                name
            )));
        }
        Ok(())
    }
}

/// Generate a default config.toml content for documentation.
pub fn default_config_toml() -> String {
    let config = Config::default();
    toml::to_string_pretty(&config).expect("Default config should serialize")
}

// ── Config hot-reload watcher ─────────────────────────────────────────────────

/// Watches the config file for changes and signals via a channel.
pub struct ConfigWatcher {
    _watcher: RecommendedWatcher,
    rx: Receiver<()>,
}

impl ConfigWatcher {
    /// Start watching the config file for changes.
    pub fn new() -> ConfigResult<Self> {
        let config_path = config_path()?;
        let (tx, rx) = mpsc::channel();

        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    match event.kind {
                        EventKind::Modify(_) | EventKind::Create(_) => {
                            let _ = tx.send(());
                        }
                        _ => {}
                    }
                }
            },
            NotifyConfig::default(),
        )
        .map_err(|e| ConfigError::Io(std::io::Error::other(e)))?;

        if let Some(parent) = config_path.parent() {
            watcher
                .watch(parent, RecursiveMode::NonRecursive)
                .map_err(|e| ConfigError::Io(std::io::Error::other(e)))?;
        }

        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }

    /// Check if the config file has changed since the last check.
    /// Returns true if a change was detected (non-blocking).
    pub fn check(&self) -> bool {
        self.rx.try_recv().is_ok()
    }

    /// Create a no-op watcher (useful when file watching is unavailable).
    pub fn dummy() -> Self {
        let (tx, rx) = mpsc::channel();
        // Drop the sender immediately so check() always returns false
        drop(tx);
        Self {
            _watcher: RecommendedWatcher::new(move |_| {}, NotifyConfig::default())
                .expect("Failed to create dummy watcher"),
            rx,
        }
    }
}

// ── Session save/restore ──────────────────────────────────────────────────────

/// Saved session state (window geometry, font, theme).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub window_width: u32,
    pub window_height: u32,
    pub window_x: Option<i32>,
    pub window_y: Option<i32>,
    pub font_family: String,
    pub font_size: u16,
    pub theme: String,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            window_width: 800,
            window_height: 600,
            window_x: None,
            window_y: None,
            font_family: "JetBrains Mono".to_string(),
            font_size: 14,
            theme: "mochi-galaxy".to_string(),
        }
    }
}

/// Get the session file path.
fn session_path() -> ConfigResult<PathBuf> {
    let dir = config_dir()?;
    Ok(dir.join("session.toml"))
}

/// Load session state from disk.
pub fn load_session() -> Session {
    session_path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

/// Save session state to disk.
pub fn save_session(session: &Session) {
    if let Ok(path) = session_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(content) = toml::to_string_pretty(session) {
            let _ = std::fs::write(&path, content);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn default_config_is_valid() {
        let mut config = Config::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn default_config_toml_parses() {
        let toml = default_config_toml();
        let mut config: Config = toml::from_str(&toml).expect("Default TOML should parse");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn invalid_font_size_rejected() {
        let mut config = Config::default();
        config.font.size = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn invalid_opacity_rejected() {
        let mut config = Config::default();
        config.appearance.opacity = 1.5;
        assert!(config.validate().is_err());
    }

    #[test]
    fn invalid_color_format_rejected() {
        let mut config = Config::default();
        config.colors.background = "not-a-color".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn invalid_cursor_style_rejected() {
        let mut config = Config::default();
        config.cursor.style = "invalid".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn load_from_file() {
        let mut file = NamedTempFile::new().unwrap();
        let toml = r##"
[font]
family = "Fira Code"
size = 16

[colors]
background = "#000000"
foreground = "#ffffff"
cursor = "#ff0000"
selection = "#333333"
"##;
        file.write_all(toml.as_bytes()).unwrap();
        file.flush().unwrap();

        let config = load_config_from(file.path()).unwrap();
        assert_eq!(config.font.family, "Fira Code");
        assert_eq!(config.font.size, 16);
        assert_eq!(config.colors.background, "#000000");
    }
}
