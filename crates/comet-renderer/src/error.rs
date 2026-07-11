//! Error types for the renderer.

use std::fmt;
use thiserror::Error;

/// Result type for renderer operations.
pub type RendererResult<T> = Result<T, RendererError>;

/// Errors that can occur during rendering.
#[derive(Debug, Error)]
pub enum RendererError {
    /// Failed to create render pipeline.
    #[error("Failed to create render pipeline: {0}")]
    PipelineCreation(String),

    /// Failed to create bind group.
    #[error("Failed to create bind group: {0}")]
    BindGroupCreation(String),

    /// Failed to create buffer.
    #[error("Failed to create buffer: {0}")]
    BufferCreation(String),

    /// Failed to create texture.
    #[error("Failed to create texture: {0}")]
    TextureCreation(String),

    /// Failed to write buffer.
    #[error("Failed to write buffer: {0}")]
    BufferWrite(String),

    /// Failed to submit command buffer.
    #[error("Failed to submit command buffer: {0}")]
    CommandSubmission(String),

    /// Surface configuration error.
    #[error("Surface configuration error: {0}")]
    SurfaceConfig(String),

    /// Font loading error.
    #[error("Font error: {0}")]
    Font(String),

    /// Glyph not found in cache.
    #[error("Glyph not found: {0}")]
    GlyphNotFound(char),

    /// Atlas is full and cannot grow.
    #[error("Texture atlas full: cannot fit glyph {0}")]
    AtlasFull(char),

    /// Invalid dimensions.
    #[error("Invalid dimensions: {0}")]
    InvalidDimensions(String),

    /// Backend-specific error.
    #[error("Backend error: {0}")]
    Backend(String),

    /// Shader compilation error.
    #[error("Shader error: {0}")]
    Shader(String),

    /// WGPU error.
    #[error("WGPU error: {0}")]
    Wgpu(#[from] wgpu::Error),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Generic error.
    #[error("{0}")]
    Other(String),
}

impl RendererError {
    pub fn pipeline_creation(msg: impl Into<String>) -> Self {
        Self::PipelineCreation(msg.into())
    }

    pub fn bind_group_creation(msg: impl Into<String>) -> Self {
        Self::BindGroupCreation(msg.into())
    }

    pub fn buffer_creation(msg: impl Into<String>) -> Self {
        Self::BufferCreation(msg.into())
    }

    pub fn texture_creation(msg: impl Into<String>) -> Self {
        Self::TextureCreation(msg.into())
    }

    pub fn buffer_write(msg: impl Into<String>) -> Self {
        Self::BufferWrite(msg.into())
    }

    pub fn command_submission(msg: impl Into<String>) -> Self {
        Self::CommandSubmission(msg.into())
    }

    pub fn surface_config(msg: impl Into<String>) -> Self {
        Self::SurfaceConfig(msg.into())
    }

    pub fn font(msg: impl Into<String>) -> Self {
        Self::Font(msg.into())
    }

    pub fn glyph_not_found(ch: char) -> Self {
        Self::GlyphNotFound(ch)
    }

    pub fn atlas_full(ch: char) -> Self {
        Self::AtlasFull(ch)
    }

    pub fn invalid_dimensions(msg: impl Into<String>) -> Self {
        Self::InvalidDimensions(msg.into())
    }

    pub fn backend(msg: impl Into<String>) -> Self {
        Self::Backend(msg.into())
    }

    pub fn shader(msg: impl Into<String>) -> Self {
        Self::Shader(msg.into())
    }

    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }
}