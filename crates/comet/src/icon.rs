//! Application icon loading.
//!
//! Placeholder for the terminal application icon.
//!
//! When Mochi mascot is ready, replace `comet-icon.png` in the `resources/`
//! directory and add the `image` crate to decode the PNG via
//! `winit::window::Icon::from_rgba`.
//!
//! Currently returns `None` — the window manager falls back to its default
//! generic icon.

use winit::window::Icon;

/// Loads the application icon for the window.
///
/// Returns `None` if no icon is available.
pub fn load_app_icon() -> Option<Icon> {
    None
}
