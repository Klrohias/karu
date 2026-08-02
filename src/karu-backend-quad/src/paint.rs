use super::*;
use karu::{Brush, Color, GradientStop, Rect};
use macroquad::prelude::{
    Color as QuadColor, Material, MaterialParams, Mesh, ShaderSource, UniformDesc, UniformType,
    Vertex, draw_mesh, draw_rectangle,
};
use macroquad::window::miniquad::{Backend, BlendFactor, BlendState, BlendValue, Equation};

pub(crate) fn draw_rect(rect: Rect, color: Color) {
    let rect = snap_rect_to_physical_pixels(rect);

    draw_rectangle(
        rect.origin.x,
        rect.origin.y,
        rect.size.width,
        rect.size.height,
        to_quad_color(color),
    );
}

pub(crate) fn set_quad_clip_material(material: &Material, clips: &[QuadClip]) {
    let mut rects = [macroquad::math::Vec4::ZERO; MAX_QUAD_CLIPS];
    let mut radii = [macroquad::math::Vec4::ZERO; 2];
    for (index, clip) in clips.iter().take(MAX_QUAD_CLIPS).enumerate() {
        rects[index] = macroquad::math::vec4(
            clip.rect.origin.x,
            clip.rect.origin.y,
            clip.rect.size.width.max(0.0),
            clip.rect.size.height.max(0.0),
        );
        radii[index / 4][index % 4] = clip.radius.max(0.0);
    }
    material.set_uniform_array("clip_rects", &rects);
    material.set_uniform_array("clip_radii", &radii);
    material.set_uniform("clip_count", clips.len().min(MAX_QUAD_CLIPS) as f32);
}

pub(crate) fn create_quad_clip_material() -> Material {
    let context = unsafe { macroquad::window::get_internal_gl().quad_context };
    let shader = match context.info().backend {
        Backend::OpenGl => ShaderSource::Glsl {
            vertex: QUAD_CLIP_VERTEX,
            fragment: QUAD_CLIP_FRAGMENT,
        },
        Backend::Metal => ShaderSource::Msl {
            program: QUAD_CLIP_METAL,
        },
    };
    let pipeline_params = macroquad::prelude::PipelineParams {
        color_blend: Some(BlendState::new(
            Equation::Add,
            BlendFactor::Value(BlendValue::SourceAlpha),
            BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
        )),
        ..Default::default()
    };
    macroquad::prelude::load_material(
        shader,
        MaterialParams {
            pipeline_params,
            uniforms: vec![
                UniformDesc::array(
                    UniformDesc::new("clip_rects", UniformType::Float4),
                    MAX_QUAD_CLIPS,
                ),
                UniformDesc::array(UniformDesc::new("clip_radii", UniformType::Float4), 2),
                UniformDesc::new("clip_count", UniformType::Float1),
            ],
            ..Default::default()
        },
    )
    .expect("quad rounded clip material compiles")
}

pub(crate) const QUAD_CLIP_VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;

varying lowp vec4 color;
varying highp vec2 world_position;
varying lowp vec2 uv;

uniform mat4 Model;
uniform mat4 Projection;

void main() {
    vec4 world = Model * vec4(position, 1.0);
    gl_Position = Projection * world;
    color = color0 / 255.0;
    world_position = world.xy;
    uv = texcoord;
}"#;

pub(crate) const QUAD_CLIP_FRAGMENT: &str = r#"#version 100
varying lowp vec4 color;
varying highp vec2 world_position;
varying lowp vec2 uv;

uniform sampler2D Texture;
uniform highp vec4 clip_rects[8];
uniform highp vec4 clip_radii[2];
uniform highp float clip_count;

bool inside_rounded_rect(highp vec2 position, highp vec4 rect, highp float radius) {
    if (position.x < rect.x || position.y < rect.y
        || position.x > rect.x + rect.z || position.y > rect.y + rect.w) {
        return false;
    }
    highp float r = min(radius, min(rect.z, rect.w) * 0.5);
    if (r <= 0.0) {
        return true;
    }
    highp vec2 local = position - rect.xy;
    highp float dx = max(r - local.x, max(0.0, local.x - (rect.z - r)));
    highp float dy = max(r - local.y, max(0.0, local.y - (rect.w - r)));
    return dx * dx + dy * dy <= r * r;
}

void main() {
    for (int index = 0; index < 8; index++) {
        if (float(index) >= clip_count) {
            break;
        }
        highp float radius = clip_radii[index / 4][index - (index / 4) * 4];
        if (!inside_rounded_rect(world_position, clip_rects[index], radius)) {
            discard;
        }
    }
    gl_FragColor = color * texture2D(Texture, uv);
}"#;

pub(crate) const QUAD_CLIP_METAL: &str = r#"#include <metal_stdlib>
using namespace metal;

pub(crate) struct Vertex {
    float3 position [[attribute(0)]];
    float2 texcoord [[attribute(1)]];
    float4 color0 [[attribute(2)]];
};

pub(crate) struct Uniforms {
    float4x4 Model;
    float4x4 Projection;
    float4 _Time;
    float4 clip_rects[8];
    float4 clip_radii[2];
    float clip_count;
};

pub(crate) struct RasterizerData {
    float4 position [[position]];
    float4 color [[user(locn0)]];
    float2 world_position [[user(locn1)]];
    float2 uv [[user(locn2)]];
};

vertex RasterizerData vertexShader(Vertex vertex [[stage_in]], constant Uniforms& uniforms [[buffer(0)]]) {
    RasterizerData out;
    float4 world = uniforms.Model * float4(vertex.position, 1.0);
    out.position = uniforms.Projection * world;
    out.color = vertex.color0 / 255.0;
    out.world_position = world.xy;
    out.uv = vertex.texcoord;
    return out;
}

bool inside_rounded_rect(float2 position, float4 rect, float radius) {
    if (position.x < rect.x || position.y < rect.y
        || position.x > rect.x + rect.z || position.y > rect.y + rect.w) {
        return false;
    }
    float r = min(radius, min(rect.z, rect.w) * 0.5);
    if (r <= 0.0) {
        return true;
    }
    float2 local = position - rect.xy;
    float dx = max(r - local.x, max(0.0, local.x - (rect.z - r)));
    float dy = max(r - local.y, max(0.0, local.y - (rect.w - r)));
    return dx * dx + dy * dy <= r * r;
}

fragment float4 fragmentShader(RasterizerData input [[stage_in]], constant Uniforms& uniforms [[buffer(0)]], texture2d<float> texture [[texture(0)]], sampler sampler [[sampler(0)]]) {
    for (int index = 0; index < 8; index++) {
        if (float(index) >= uniforms.clip_count) {
            break;
        }
        float radius = uniforms.clip_radii[index / 4][index - (index / 4) * 4];
        if (!inside_rounded_rect(input.world_position, uniforms.clip_rects[index], radius)) {
            discard_fragment();
        }
    }
    return input.color * texture.sample(sampler, input.uv);
}"#;

pub(crate) fn draw_fill(rect: Rect, brush: &Brush, radius: f32) {
    let rect = snap_rect_to_physical_pixels(rect);
    if rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return;
    }
    if radius <= 0.0 {
        match brush {
            Brush::Solid(color) => draw_rect(rect, *color),
            Brush::LinearGradient { stops, .. } if stops.is_empty() => {}
            Brush::LinearGradient { .. } => draw_mesh(&fill_mesh(rect, brush, 0.0)),
        }
    } else {
        draw_mesh(&fill_mesh(rect, brush, radius));
    }
}

pub(crate) fn draw_stroke(rect: Rect, brush: &Brush, width: f32, radius: f32) {
    let rect = snap_rect_to_physical_pixels(rect);
    let vertices = stroke_vertices(rect, brush, width, radius);
    if vertices.is_empty() {
        return;
    }
    let vertex_count = vertices.len() as u16;
    draw_mesh(&Mesh {
        vertices,
        indices: (0..vertex_count).collect(),
        texture: None,
    });
}

pub(crate) fn fill_mesh(rect: Rect, brush: &Brush, radius: f32) -> Mesh {
    let radius = radius
        .max(0.0)
        .min(rect.size.width.min(rect.size.height) * 0.5);
    let points = rounded_points(rect, radius);
    let center = [
        rect.origin.x + rect.size.width * 0.5,
        rect.origin.y + rect.size.height * 0.5,
    ];
    let mut vertices = Vec::with_capacity(points.len() + 1);
    vertices.push(Vertex::new(
        center[0],
        center[1],
        0.0,
        0.0,
        0.0,
        brush_color(brush, center[0], center[1]),
    ));
    for point in &points {
        vertices.push(Vertex::new(
            point[0],
            point[1],
            0.0,
            0.0,
            0.0,
            brush_color(brush, point[0], point[1]),
        ));
    }
    let mut indices = Vec::with_capacity(points.len() * 3);
    for index in 0..points.len() {
        indices.extend([
            0,
            (index + 1) as u16,
            ((index + 1) % points.len() + 1) as u16,
        ]);
    }
    Mesh {
        vertices,
        indices,
        texture: None,
    }
}

pub(crate) fn stroke_vertices(rect: Rect, brush: &Brush, width: f32, radius: f32) -> Vec<Vertex> {
    let min_dimension = rect.size.width.min(rect.size.height);
    let width = width.max(0.0).min(min_dimension * 0.5);
    if width == 0.0 || min_dimension <= 0.0 {
        return Vec::new();
    }
    let radius = radius.max(0.0).min(min_dimension * 0.5);
    if radius == 0.0 {
        let x = rect.origin.x;
        let y = rect.origin.y;
        let right = x + rect.size.width;
        let bottom = y + rect.size.height;
        return vec![
            quad_vertex(brush, [x, y]),
            quad_vertex(brush, [right, y]),
            quad_vertex(brush, [right, y + width]),
            quad_vertex(brush, [x, y]),
            quad_vertex(brush, [right, y + width]),
            quad_vertex(brush, [x, y + width]),
            quad_vertex(brush, [x, bottom - width]),
            quad_vertex(brush, [right, bottom - width]),
            quad_vertex(brush, [right, bottom]),
            quad_vertex(brush, [x, bottom - width]),
            quad_vertex(brush, [right, bottom]),
            quad_vertex(brush, [x, bottom]),
            quad_vertex(brush, [x, y + width]),
            quad_vertex(brush, [x + width, y + width]),
            quad_vertex(brush, [x + width, bottom - width]),
            quad_vertex(brush, [x, y + width]),
            quad_vertex(brush, [x + width, bottom - width]),
            quad_vertex(brush, [x, bottom - width]),
            quad_vertex(brush, [right - width, y + width]),
            quad_vertex(brush, [right, y + width]),
            quad_vertex(brush, [right, bottom - width]),
            quad_vertex(brush, [right - width, y + width]),
            quad_vertex(brush, [right, bottom - width]),
            quad_vertex(brush, [right - width, bottom - width]),
        ];
    }
    let outer = rounded_points(rect, radius);
    let inner_rect = Rect::new(
        rect.origin.x + width,
        rect.origin.y + width,
        (rect.size.width - width * 2.0).max(0.0),
        (rect.size.height - width * 2.0).max(0.0),
    );
    let inner = rounded_points(inner_rect, (radius - width).max(0.0));
    let mut vertices = Vec::with_capacity(outer.len() * 6);
    for index in 0..outer.len() {
        let next = (index + 1) % outer.len();
        let outer0 = outer[index];
        let outer1 = outer[next];
        let inner0 = inner[index];
        let inner1 = inner[next];
        vertices.extend([
            quad_vertex(brush, outer0),
            quad_vertex(brush, outer1),
            quad_vertex(brush, inner1),
            quad_vertex(brush, outer0),
            quad_vertex(brush, inner1),
            quad_vertex(brush, inner0),
        ]);
    }
    vertices
}

pub(crate) fn rounded_points(rect: Rect, radius: f32) -> Vec<[f32; 2]> {
    const SEGMENTS: usize = 8;
    let radius = radius
        .max(0.0)
        .min(rect.size.width.min(rect.size.height) * 0.5);
    let x = rect.origin.x;
    let y = rect.origin.y;
    let right = x + rect.size.width;
    let bottom = y + rect.size.height;
    let corners = [
        (x + radius, y + radius, std::f32::consts::PI),
        (right - radius, y + radius, 1.5 * std::f32::consts::PI),
        (right - radius, bottom - radius, 0.0),
        (x + radius, bottom - radius, 0.5 * std::f32::consts::PI),
    ];
    let mut points = Vec::with_capacity(SEGMENTS * 4);
    for (cx, cy, start) in corners {
        for step in 0..SEGMENTS {
            let angle = start + step as f32 * std::f32::consts::FRAC_PI_2 / SEGMENTS as f32;
            points.push([cx + radius * angle.cos(), cy + radius * angle.sin()]);
        }
    }
    points
}

pub(crate) fn quad_vertex(brush: &Brush, point: [f32; 2]) -> Vertex {
    Vertex::new(
        point[0],
        point[1],
        0.0,
        0.0,
        0.0,
        brush_color(brush, point[0], point[1]),
    )
}

pub(crate) fn brush_color(brush: &Brush, x: f32, y: f32) -> QuadColor {
    to_quad_color(match brush {
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
    })
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
        let from = &pair[0];
        let to = &pair[1];
        if position <= to.position {
            let span = (to.position - from.position).max(f32::EPSILON);
            return from.color.lerp(to.color, (position - from.position) / span);
        }
    }
    stops.last().map_or(first.color, |stop| stop.color)
}

pub(crate) fn to_quad_color(color: Color) -> QuadColor {
    QuadColor::new(color.red, color.green, color.blue, color.alpha)
}

pub(crate) fn snap_rect_to_physical_pixels(rect: Rect) -> Rect {
    let x = snap_to_physical_pixel(rect.origin.x);
    let y = snap_to_physical_pixel(rect.origin.y);
    let right = snap_to_physical_pixel(rect.origin.x + rect.size.width);
    let bottom = snap_to_physical_pixel(rect.origin.y + rect.size.height);

    Rect::new(x, y, (right - x).max(0.0), (bottom - y).max(0.0))
}

pub(crate) fn snap_to_physical_pixel(value: f32) -> f32 {
    snap_value_to_scale(value, macroquad::miniquad::window::dpi_scale())
}

pub(crate) fn snap_value_to_scale(value: f32, scale: f32) -> f32 {
    if scale.is_finite() && scale > 0.0 {
        (value * scale).round() / scale
    } else {
        value
    }
}

#[allow(dead_code)]
pub(crate) fn font_size(size: f32) -> u16 {
    size.round().clamp(1.0, f32::from(u16::MAX)) as u16
}
