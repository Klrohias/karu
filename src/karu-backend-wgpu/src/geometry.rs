use super::*;
use karu::{Brush, Color, GradientStop, Offset, Rect, RenderCommand, TextWrap};
use std::hash::{DefaultHasher, Hash, Hasher};
use winit::dpi::PhysicalSize;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct ScreenUniform {
    size: [f32; 2],
    clip_count: u32,
    _padding: u32,
    clip_rects: [[f32; 4]; MAX_CLIPS],
    clip_radii: [[f32; 4]; 2],
}

pub(crate) const MAX_CLIPS: usize = 8;

#[derive(Clone, Copy)]
pub(crate) struct ClipRect {
    pub(crate) rect: Rect,
    pub(crate) radius: f32,
}

pub(crate) fn screen_uniform(size: [f32; 2], clips: &[ClipRect]) -> ScreenUniform {
    let mut clip_rects = [[0.0; 4]; MAX_CLIPS];
    let mut clip_radii = [[0.0; 4]; 2];
    for (index, clip) in clips.iter().take(MAX_CLIPS).enumerate() {
        clip_rects[index] = [
            clip.rect.origin.x,
            clip.rect.origin.y,
            clip.rect.size.width.max(0.0),
            clip.rect.size.height.max(0.0),
        ];
        clip_radii[index / 4][index % 4] = clip.radius.max(0.0);
    }
    ScreenUniform {
        size,
        clip_count: clips.len().min(MAX_CLIPS) as u32,
        _padding: 0,
        clip_rects,
        clip_radii,
    }
}

pub(crate) fn clip_uniforms_for_commands(
    commands: &[RenderCommand],
    logical_size: [f32; 2],
) -> Vec<ScreenUniform> {
    let mut clips = Vec::new();
    let mut uniforms = Vec::with_capacity(commands.len().max(1));
    for command in commands {
        uniforms.push(screen_uniform(logical_size, &clips));
        match command {
            RenderCommand::PushClip { rect, radius, .. } => {
                let rect = clips
                    .last()
                    .copied()
                    .map(|parent: ClipRect| intersect_rect(parent.rect, *rect))
                    .unwrap_or(*rect);
                clips.push(ClipRect {
                    rect,
                    radius: *radius,
                });
            }
            RenderCommand::PopClip => {
                clips.pop();
            }
            _ => {}
        }
    }
    if uniforms.is_empty() {
        uniforms.push(screen_uniform(logical_size, &clips));
    }
    uniforms
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, Debug)]
pub(crate) struct ShapeVertex {
    pub(crate) position: [f32; 2],
    pub(crate) color: [f32; 4],
}

impl ShapeVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];

    pub(crate) fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct TextVertex {
    pub(crate) position: [f32; 2],
    pub(crate) uv: [f32; 2],
}

impl TextVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];

    pub(crate) fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

pub(crate) fn primitive_state() -> wgpu::PrimitiveState {
    wgpu::PrimitiveState {
        topology: wgpu::PrimitiveTopology::TriangleList,
        strip_index_format: None,
        front_face: wgpu::FrontFace::Ccw,
        cull_mode: None,
        unclipped_depth: false,
        polygon_mode: wgpu::PolygonMode::Fill,
        conservative: false,
    }
}

pub(crate) fn draw_shape<'a>(
    pass: &mut wgpu::RenderPass<'a>,
    shape_buffer: &'a wgpu::Buffer,
    vertex_count: u32,
    shape_pipeline: &wgpu::RenderPipeline,
    screen_bind_group: &wgpu::BindGroup,
    screen_offset: u32,
) {
    pass.set_pipeline(shape_pipeline);
    pass.set_bind_group(0, screen_bind_group, &[screen_offset]);
    pass.set_vertex_buffer(0, shape_buffer.slice(..));
    pass.draw(0..vertex_count, 0..1);
}

pub(crate) fn draw_debug_info<'a>(
    pass: &mut wgpu::RenderPass<'a>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    shape_pipeline: &wgpu::RenderPipeline,
    text_pipeline: &wgpu::RenderPipeline,
    screen_bind_group: &wgpu::BindGroup,
    screen_offset: u32,
    text_bind_group_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    text_cache: &mut TextResourceCache,
    text_rasterizer: &mut TextRasterizer,
    background_buffer: &'a wgpu::Buffer,
    fps: f32,
    scale_factor: f32,
) {
    let rect = Rect::new(8.0, 8.0, 86.0, 28.0);
    draw_shape(
        pass,
        background_buffer,
        6,
        shape_pipeline,
        screen_bind_group,
        screen_offset,
    );

    let label = format!("FPS {:.0}", fps);
    text_cache.draw(
        pass,
        device,
        queue,
        text_pipeline,
        screen_bind_group,
        screen_offset,
        text_bind_group_layout,
        sampler,
        text_rasterizer,
        rect,
        &label,
        13.0,
        Color::WHITE,
        TextWrap::NoWrap,
        Offset::new(16.0, 14.0),
        scale_factor,
    );
}

pub(crate) fn shape_vertices(command: &RenderCommand) -> Option<Vec<ShapeVertex>> {
    let vertices = match command {
        RenderCommand::FillRect {
            rect,
            color,
            radius,
            ..
        } => rounded_rect_vertices(*rect, &Brush::Solid(*color), *radius),
        RenderCommand::FillBrush {
            rect,
            brush,
            radius,
            ..
        } => rounded_rect_vertices(*rect, brush, *radius),
        RenderCommand::StrokeRect {
            rect,
            brush,
            width,
            radius,
            ..
        } => stroke_vertices(*rect, brush, *width, *radius),
        RenderCommand::DrawSelection { rect, color, .. }
        | RenderCommand::DrawCursor { rect, color, .. }
        | RenderCommand::DrawComposition { rect, color, .. } => rect_vertices(*rect, *color),
        _ => return None,
    };
    (!vertices.is_empty()).then_some(vertices)
}

pub(crate) fn shape_fingerprint(vertices: &[ShapeVertex]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for vertex in vertices {
        for value in vertex.position {
            value.to_bits().hash(&mut hasher);
        }
        for value in vertex.color {
            value.to_bits().hash(&mut hasher);
        }
    }
    hasher.finish()
}

pub(crate) fn rect_vertices(rect: Rect, color: Color) -> Vec<ShapeVertex> {
    let c = rgba(color);
    let x = rect.origin.x;
    let y = rect.origin.y;
    let r = x + rect.size.width;
    let b = y + rect.size.height;
    vec![
        vertex(x, y, c),
        vertex(r, y, c),
        vertex(r, b, c),
        vertex(x, y, c),
        vertex(r, b, c),
        vertex(x, b, c),
    ]
}

pub(crate) fn brush_vertices(rect: Rect, brush: &Brush) -> Vec<ShapeVertex> {
    match brush {
        Brush::Solid(color) => rect_vertices(rect, *color),
        Brush::LinearGradient { start, end, stops } => {
            let color = |x: f32, y: f32| {
                let dx = end.x - start.x;
                let dy = end.y - start.y;
                let denominator = dx * dx + dy * dy;
                let t = if denominator > 0.0 {
                    ((x - start.x) * dx + (y - start.y) * dy) / denominator
                } else {
                    0.0
                };
                gradient_color(stops, t)
            };
            let x = rect.origin.x;
            let y = rect.origin.y;
            let r = x + rect.size.width;
            let b = y + rect.size.height;
            vec![
                vertex(x, y, rgba(color(x, y))),
                vertex(r, y, rgba(color(r, y))),
                vertex(r, b, rgba(color(r, b))),
                vertex(x, y, rgba(color(x, y))),
                vertex(r, b, rgba(color(r, b))),
                vertex(x, b, rgba(color(x, b))),
            ]
        }
    }
}

pub(crate) fn rounded_rect_vertices(rect: Rect, brush: &Brush, radius: f32) -> Vec<ShapeVertex> {
    if radius <= 0.0 {
        return brush_vertices(rect, brush);
    }
    let points = rounded_points(rect, radius);
    if points.is_empty() {
        return Vec::new();
    }
    let center = [
        rect.origin.x + rect.size.width * 0.5,
        rect.origin.y + rect.size.height * 0.5,
    ];
    let mut vertices = Vec::with_capacity(points.len() * 3);
    for index in 0..points.len() {
        let next = (index + 1) % points.len();
        vertices.extend([
            brush_vertex(brush, center[0], center[1]),
            brush_vertex(brush, points[index][0], points[index][1]),
            brush_vertex(brush, points[next][0], points[next][1]),
        ]);
    }
    vertices
}

pub(crate) fn rounded_points(rect: Rect, radius: f32) -> Vec<[f32; 2]> {
    let radius = radius
        .max(0.0)
        .min(rect.size.width.min(rect.size.height) * 0.5);
    if rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return Vec::new();
    }
    let x = rect.origin.x;
    let y = rect.origin.y;
    let right = x + rect.size.width;
    let bottom = y + rect.size.height;
    let segments = rounded_corner_segments(radius);
    let corners = [
        (x + radius, y + radius, std::f32::consts::PI),
        (right - radius, y + radius, 1.5 * std::f32::consts::PI),
        (right - radius, bottom - radius, 0.0),
        (x + radius, bottom - radius, 0.5 * std::f32::consts::PI),
    ];
    let mut points = Vec::with_capacity(segments * 4);
    for (cx, cy, start) in corners {
        for step in 0..segments {
            let angle = start + step as f32 * std::f32::consts::FRAC_PI_2 / segments as f32;
            points.push([cx + radius * angle.cos(), cy + radius * angle.sin()]);
        }
    }
    points
}

pub(crate) fn rounded_corner_segments(radius: f32) -> usize {
    const MIN_SEGMENTS: usize = 8;
    const MAX_SEGMENTS: usize = 24;
    const MAX_CHORD_ERROR: f32 = 0.25;

    if radius <= 0.0 {
        return MIN_SEGMENTS;
    }
    let angle = (1.0 - MAX_CHORD_ERROR / radius).clamp(-1.0, 1.0).acos();
    if angle <= f32::EPSILON {
        return MAX_SEGMENTS;
    }
    (std::f32::consts::FRAC_PI_2 / angle)
        .ceil()
        .clamp(MIN_SEGMENTS as f32, MAX_SEGMENTS as f32) as usize
}

pub(crate) fn brush_vertex(brush: &Brush, x: f32, y: f32) -> ShapeVertex {
    let color = match brush {
        Brush::Solid(color) => *color,
        Brush::LinearGradient { start, end, stops } => {
            let dx = end.x - start.x;
            let dy = end.y - start.y;
            let denominator = dx * dx + dy * dy;
            let position = if denominator > 0.0 {
                ((x - start.x) * dx + (y - start.y) * dy) / denominator
            } else {
                0.0
            };
            gradient_color(stops, position)
        }
    };
    vertex(x, y, rgba(color))
}

pub(crate) fn stroke_vertices(
    rect: Rect,
    brush: &Brush,
    width: f32,
    radius: f32,
) -> Vec<ShapeVertex> {
    let x = rect.origin.x;
    let y = rect.origin.y;
    let r = x + rect.size.width;
    let b = y + rect.size.height;
    let min_dimension = rect.size.width.min(rect.size.height);
    let width = width.max(0.0).min(min_dimension * 0.5);
    if width == 0.0 || min_dimension <= 0.0 {
        return Vec::new();
    }
    let radius = radius.max(0.0).min(min_dimension * 0.5);
    if radius == 0.0 {
        return vec![
            brush_vertex(brush, x, y),
            brush_vertex(brush, r, y),
            brush_vertex(brush, r, y + width),
            brush_vertex(brush, x, y),
            brush_vertex(brush, r, y + width),
            brush_vertex(brush, x, y + width),
            brush_vertex(brush, x, b - width),
            brush_vertex(brush, r, b - width),
            brush_vertex(brush, r, b),
            brush_vertex(brush, x, b - width),
            brush_vertex(brush, r, b),
            brush_vertex(brush, x, b),
            brush_vertex(brush, x, y + width),
            brush_vertex(brush, x + width, y + width),
            brush_vertex(brush, x + width, b - width),
            brush_vertex(brush, x, y + width),
            brush_vertex(brush, x + width, b - width),
            brush_vertex(brush, x, b - width),
            brush_vertex(brush, r - width, y + width),
            brush_vertex(brush, r, y + width),
            brush_vertex(brush, r, b - width),
            brush_vertex(brush, r - width, y + width),
            brush_vertex(brush, r, b - width),
            brush_vertex(brush, r - width, b - width),
        ];
    }
    let mut vertices = Vec::new();
    let inner = (radius - width).max(0.0);
    let segments = rounded_corner_segments(radius);

    extend_quad(
        &mut vertices,
        [x + radius, y],
        [r - radius, y],
        [r - inner, y + width],
        [x + inner, y + width],
        brush,
    );
    extend_quad(
        &mut vertices,
        [r, y + radius],
        [r, b - radius],
        [r - width, b - inner],
        [r - width, y + inner],
        brush,
    );
    extend_quad(
        &mut vertices,
        [x + radius, b],
        [r - radius, b],
        [r - inner, b - width],
        [x + inner, b - width],
        brush,
    );
    extend_quad(
        &mut vertices,
        [x, b - radius],
        [x, y + radius],
        [x + width, y + inner],
        [x + width, b - inner],
        brush,
    );

    for (cx, cy, start) in [
        (x + radius, y + radius, std::f32::consts::PI),
        (r - radius, y + radius, 1.5 * std::f32::consts::PI),
        (r - radius, b - radius, 0.0),
        (x + radius, b - radius, 0.5 * std::f32::consts::PI),
    ] {
        for step in 0..segments {
            let a0 = start + step as f32 * std::f32::consts::FRAC_PI_2 / segments as f32;
            let a1 = start + (step + 1) as f32 * std::f32::consts::FRAC_PI_2 / segments as f32;
            let outer0 = [cx + radius * a0.cos(), cy + radius * a0.sin()];
            let outer1 = [cx + radius * a1.cos(), cy + radius * a1.sin()];
            let inner0 = [cx + inner * a0.cos(), cy + inner * a0.sin()];
            let inner1 = [cx + inner * a1.cos(), cy + inner * a1.sin()];
            vertices.extend([
                brush_vertex(brush, outer0[0], outer0[1]),
                brush_vertex(brush, outer1[0], outer1[1]),
                brush_vertex(brush, inner1[0], inner1[1]),
                brush_vertex(brush, outer0[0], outer0[1]),
                brush_vertex(brush, inner1[0], inner1[1]),
                brush_vertex(brush, inner0[0], inner0[1]),
            ]);
        }
    }
    vertices
}

pub(crate) fn extend_quad(
    vertices: &mut Vec<ShapeVertex>,
    outer0: [f32; 2],
    outer1: [f32; 2],
    inner1: [f32; 2],
    inner0: [f32; 2],
    brush: &Brush,
) {
    vertices.extend([
        brush_vertex(brush, outer0[0], outer0[1]),
        brush_vertex(brush, outer1[0], outer1[1]),
        brush_vertex(brush, inner1[0], inner1[1]),
        brush_vertex(brush, outer0[0], outer0[1]),
        brush_vertex(brush, inner1[0], inner1[1]),
        brush_vertex(brush, inner0[0], inner0[1]),
    ]);
}

pub(crate) fn vertex(x: f32, y: f32, color: [f32; 4]) -> ShapeVertex {
    ShapeVertex {
        position: [x, y],
        color,
    }
}

pub(crate) fn rgba(color: Color) -> [f32; 4] {
    [
        color.red.clamp(0.0, 1.0),
        color.green.clamp(0.0, 1.0),
        color.blue.clamp(0.0, 1.0),
        color.alpha.clamp(0.0, 1.0),
    ]
}

pub(crate) fn to_wgpu_color(color: Color) -> wgpu::Color {
    wgpu::Color {
        r: color.red.clamp(0.0, 1.0) as f64,
        g: color.green.clamp(0.0, 1.0) as f64,
        b: color.blue.clamp(0.0, 1.0) as f64,
        a: color.alpha.clamp(0.0, 1.0) as f64,
    }
}

pub(crate) fn logical_surface_size(size: PhysicalSize<u32>, scale_factor: f32) -> [f32; 2] {
    let scale_factor = normalized_scale(scale_factor);
    [
        size.width as f32 / scale_factor,
        size.height as f32 / scale_factor,
    ]
}

pub(crate) fn gradient_color(stops: &[GradientStop], position: f32) -> Color {
    let Some(first) = stops.first() else {
        return Color::TRANSPARENT;
    };
    let position = position.clamp(0.0, 1.0);
    if position <= first.position {
        return first.color;
    }
    for pair in stops.windows(2) {
        if position <= pair[1].position {
            let span = (pair[1].position - pair[0].position).max(f32::EPSILON);
            return pair[0]
                .color
                .lerp(pair[1].color, (position - pair[0].position) / span);
        }
    }
    stops.last().map_or(first.color, |stop| stop.color)
}

pub(crate) fn intersect_rect(left: Rect, right: Rect) -> Rect {
    let x = left.origin.x.max(right.origin.x);
    let y = left.origin.y.max(right.origin.y);
    let right_edge = (left.origin.x + left.size.width).min(right.origin.x + right.size.width);
    let bottom = (left.origin.y + left.size.height).min(right.origin.y + right.size.height);
    Rect::new(x, y, (right_edge - x).max(0.0), (bottom - y).max(0.0))
}

pub(crate) fn scissor_rect(
    rect: Rect,
    scale: f32,
    surface_width: u32,
    surface_height: u32,
) -> (u32, u32, u32, u32) {
    let scale = normalized_scale(scale);
    let viewport = Rect::new(
        0.0,
        0.0,
        surface_width as f32 / scale,
        surface_height as f32 / scale,
    );
    let rect = intersect_rect(viewport, rect);
    let x = (rect.origin.x * scale)
        .floor()
        .clamp(0.0, surface_width as f32) as u32;
    let y = (rect.origin.y * scale)
        .floor()
        .clamp(0.0, surface_height as f32) as u32;
    let right = ((rect.origin.x + rect.size.width) * scale)
        .ceil()
        .clamp(x as f32, surface_width as f32) as u32;
    let bottom = ((rect.origin.y + rect.size.height) * scale)
        .ceil()
        .clamp(y as f32, surface_height as f32) as u32;
    (x, y, right.saturating_sub(x), bottom.saturating_sub(y))
}
