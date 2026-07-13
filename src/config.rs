use std::path::PathBuf;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_font_family")]
    pub font_family: String,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    #[serde(default)]
    pub colors: Colors,
    #[serde(default)]
    pub window: WindowConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Colors {
    #[serde(default = "default_background")]
    pub background: String,
    #[serde(default = "default_foreground")]
    pub foreground: String,
    #[serde(default = "default_tab_bar")]
    pub tab_bar: String,
    #[serde(default = "default_tab_active")]
    pub tab_active: String,
    #[serde(default = "default_tab_inactive")]
    pub tab_inactive: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WindowConfig {
    #[serde(default = "default_tab_height")]
    pub tab_height: f64,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            background: default_background(),
            foreground: default_foreground(),
            tab_bar: default_tab_bar(),
            tab_active: default_tab_active(),
            tab_inactive: default_tab_inactive(),
        }
    }
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            tab_height: default_tab_height(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            font_family: default_font_family(),
            font_size: default_font_size(),
            colors: Colors::default(),
            window: WindowConfig::default(),
        }
    }
}

fn default_font_family() -> String { "monospace".into() }
fn default_font_size() -> f32 { 12.0 }
fn default_background() -> String { "#1a1b1e".into() }
fn default_foreground() -> String { "#cdd6f4".into() }
fn default_tab_bar() -> String { "#11111b".into() }
fn default_tab_active() -> String { "#313244".into() }
fn default_tab_inactive() -> String { "#1e1e2e".into() }
fn default_tab_height() -> f64 { 32.0 }

fn config_path() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
            PathBuf::from(home).join(".config")
        });
    base.join("comet").join("comet.toml")
}

pub fn load() -> Config {
    let path = config_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => {
            log::info!("No config found at {:?}, using defaults", path);
            return Config::default();
        }
    };
    match toml::from_str(&content) {
        Ok(config) => config,
        Err(e) => {
            log::warn!("Failed to parse config: {e}, using defaults");
            Config::default()
        }
    }
}
