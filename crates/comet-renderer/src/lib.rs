pub mod atlas;
pub mod backend;
pub mod colors;
pub mod cursor;
pub mod damage;
pub mod diagnostics;
pub mod error;
pub mod font;
pub mod glyph_cache;
pub mod metrics;
pub mod renderer;

pub use backend::{BackendConfig, BackendType, HasWindowHandle, RenderBackend};
pub use colors::ColorPalette;
pub use cursor::{CursorConfig, CursorRenderer, CursorShape};
pub use damage::DamageTracker;
pub use error::RendererError;
pub use font::{FontSize, FontStyle, FontSystem};
pub use glyph_cache::GlyphCache;
pub use metrics::MetricsManager;
pub use renderer::{
    CustomColors, PaneRenderState, RenderContext, Renderer, RendererConfig, Viewport,
};
