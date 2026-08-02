mod app;
mod config;
mod geometry;
mod input;
mod renderer;
#[cfg(test)]
mod tests;
mod text;

pub(crate) use app::WgpuApp;
pub(crate) use geometry::*;
pub(crate) use input::*;
pub(crate) use renderer::TextResourceCache;
pub(crate) use text::TextRasterizer;
pub(crate) use text::{aligned_uniform_stride, normalized_scale};

pub use config::{ConfiguredWgpu, Wgpu};
pub use renderer::{WgpuBackend, WgpuClipboard};
pub use text::CosmicTextLayout;
