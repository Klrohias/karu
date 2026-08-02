use super::*;
use karu::{Brush, Clipboard, ClipboardError, Rect, RenderBackend, RenderCommand};
use macroquad::prelude::{Material, gl_use_default_material, gl_use_material};
use std::time::{Duration, Instant};

pub(crate) struct QuadFrameStats {
    enabled: bool,
    started: Instant,
    frames: u64,
    recompositions: u64,
    commands: usize,
}

pub(crate) const MAX_QUAD_CLIPS: usize = 8;

#[derive(Clone, Copy)]
pub(crate) struct QuadClip {
    pub(crate) rect: Rect,
    pub(crate) radius: f32,
}

impl QuadFrameStats {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            started: Instant::now(),
            frames: 0,
            recompositions: 0,
            commands: 0,
        }
    }

    pub(crate) fn record(&mut self, recomposed: bool, result: Option<&karu::CompositionResult>) {
        if !self.enabled {
            return;
        }

        self.frames += 1;
        self.recompositions += u64::from(recomposed);
        self.commands = result.map_or(0, |result| result.commands.len());

        let elapsed = self.started.elapsed();
        if elapsed >= Duration::from_secs(1) {
            eprintln!(
                "[karu][quad] frames={} recompositions={} commands={} elapsed={:?}",
                self.frames, self.recompositions, self.commands, elapsed,
            );
            self.started = Instant::now();
            self.frames = 0;
            self.recompositions = 0;
        }
    }
}

pub(crate) fn draw_command(
    command: &RenderCommand,
    text_layout: &mut CosmicTextLayout,
    clips: &mut Vec<QuadClip>,
) {
    match command {
        RenderCommand::FillRect {
            rect,
            color,
            radius,
            ..
        } => draw_fill(*rect, &Brush::Solid(*color), *radius),
        RenderCommand::FillBrush {
            rect,
            brush,
            radius,
            ..
        } => draw_fill(*rect, brush, *radius),
        RenderCommand::StrokeRect {
            rect,
            brush,
            width,
            radius,
            ..
        } => draw_stroke(*rect, brush, *width, *radius),
        RenderCommand::DrawText {
            rect,
            text,
            style,
            wrap,
            offset,
            ..
        } => text_layout.draw_text(rect, text, style.font_size, style.color, *wrap, *offset),
        RenderCommand::DrawSelection { rect, color, .. }
        | RenderCommand::DrawCursor { rect, color, .. }
        | RenderCommand::DrawComposition { rect, color, .. } => draw_rect(*rect, *color),
        RenderCommand::PushClip { rect, radius, .. } => {
            let rect = snap_rect_to_physical_pixels(*rect);
            let rect = clips
                .last()
                .copied()
                .map(|parent| intersect_rect(parent.rect, rect))
                .unwrap_or(rect);
            clips.push(QuadClip {
                rect,
                radius: *radius,
            });
            set_scissor(rect);
        }
        RenderCommand::PopClip => {
            clips.pop();
            if let Some(clip) = clips.last().copied() {
                set_scissor(clip.rect);
            } else {
                unsafe { macroquad::window::get_internal_gl() }
                    .quad_gl
                    .scissor(None);
            }
        }
        RenderCommand::DrawImage { .. } => {}
    }
}

pub(crate) fn set_scissor(rect: Rect) {
    let scale = macroquad::miniquad::window::dpi_scale();
    let scissor = scissor_rect(rect, scale);
    unsafe { macroquad::window::get_internal_gl() }
        .quad_gl
        .scissor(Some(scissor));
}

pub(crate) fn scissor_rect(rect: Rect, scale: f32) -> (i32, i32, i32, i32) {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    (
        (rect.origin.x * scale).round() as i32,
        (rect.origin.y * scale).round() as i32,
        (rect.size.width.max(0.0) * scale).round() as i32,
        (rect.size.height.max(0.0) * scale).round() as i32,
    )
}

pub(crate) fn intersect_rect(left: Rect, right: Rect) -> Rect {
    let x = left.origin.x.max(right.origin.x);
    let y = left.origin.y.max(right.origin.y);
    let right_edge = (left.origin.x + left.size.width).min(right.origin.x + right.size.width);
    let bottom = (left.origin.y + left.size.height).min(right.origin.y + right.size.height);
    Rect::new(x, y, (right_edge - x).max(0.0), (bottom - y).max(0.0))
}

pub struct QuadBackend {
    text_layout: CosmicTextLayout,
    clipboard: QuadClipboard,
    clip_material: Material,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct QuadClipboard;

impl Clipboard for QuadClipboard {
    fn get_text(&mut self) -> Result<Option<String>, ClipboardError> {
        Ok(macroquad::miniquad::window::clipboard_get())
    }

    fn set_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        macroquad::miniquad::window::clipboard_set(text);
        Ok(())
    }
}

impl QuadBackend {
    pub fn new(family: Option<String>) -> Self {
        Self {
            text_layout: CosmicTextLayout::new(family),
            clipboard: QuadClipboard,
            clip_material: create_quad_clip_material(),
        }
    }
}

impl RenderBackend for QuadBackend {
    type Output = ();
    type Error = std::convert::Infallible;
    type Clipboard = QuadClipboard;

    fn render(
        &mut self,
        _tree: &karu::RenderTree,
        commands: &[RenderCommand],
    ) -> Result<Self::Output, Self::Error> {
        let mut clips = Vec::new();
        gl_use_material(&self.clip_material);
        set_quad_clip_material(&self.clip_material, &clips);
        for command in commands {
            draw_command(command, &mut self.text_layout, &mut clips);
            set_quad_clip_material(&self.clip_material, &clips);
        }
        gl_use_default_material();
        Ok(())
    }

    fn clipboard(&mut self) -> &mut Self::Clipboard {
        &mut self.clipboard
    }
}
