mod app;
mod config;
mod input;
mod paint;
mod renderer;
#[cfg(test)]
mod tests;
mod text;

pub(crate) use app::run_quad;
pub(crate) use app::window_config;
pub(crate) use input::{
    edit_command, handle_text_result, keyboard_keys, shortcut_keys, update_ime,
};
pub(crate) use paint::{
    create_quad_clip_material, draw_fill, draw_rect, draw_stroke, set_quad_clip_material,
    snap_rect_to_physical_pixels, snap_to_physical_pixel, to_quad_color,
};
pub(crate) use renderer::{MAX_QUAD_CLIPS, QuadClip, QuadFrameStats};

pub use config::{ConfiguredQuad, Quad, QuadFrameMode};
pub use renderer::{QuadBackend, QuadClipboard};
pub use text::CosmicTextLayout;
