struct Screen {
    size: vec2<f32>,
    clip_count: u32,
    _padding: u32,
    clip_rects: array<vec4<f32>, 8>,
    clip_radii: array<vec4<f32>, 2>,
};

@group(0) @binding(0)
var<uniform> screen: Screen;

struct ShapeVertex {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct ShapeOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) world_position: vec2<f32>,
};

@vertex
fn shape_vs(vertex: ShapeVertex) -> ShapeOutput {
    var out: ShapeOutput;
    let ndc = vec2<f32>(
        vertex.position.x / screen.size.x * 2.0 - 1.0,
        1.0 - vertex.position.y / screen.size.y * 2.0,
    );
    let world_position = vertex.position;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.color = vertex.color;
    out.world_position = world_position;
    return out;
}

fn inside_rounded_rect(position: vec2<f32>, rect: vec4<f32>, radius: f32) -> bool {
    if (position.x < rect.x || position.y < rect.y
        || position.x > rect.x + rect.z || position.y > rect.y + rect.w) {
        return false;
    }
    let r = min(radius, min(rect.z, rect.w) * 0.5);
    if (r <= 0.0) {
        return true;
    }
    let local = position - rect.xy;
    let dx = max(r - local.x, max(0.0, local.x - (rect.z - r)));
    let dy = max(r - local.y, max(0.0, local.y - (rect.w - r)));
    return dx * dx + dy * dy <= r * r;
}

fn inside_clips(position: vec2<f32>) -> bool {
    for (var index: u32 = 0u; index < 8u; index = index + 1u) {
        if (index >= screen.clip_count) {
            break;
        }
        let radius = screen.clip_radii[index / 4u][index % 4u];
        if (!inside_rounded_rect(position, screen.clip_rects[index], radius)) {
            return false;
        }
    }
    return true;
}

@fragment
fn shape_fs(input: ShapeOutput) -> @location(0) vec4<f32> {
    if (!inside_clips(input.world_position)) {
        discard;
    }
    return input.color;
}

struct TextVertex {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct TextOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world_position: vec2<f32>,
};

@group(1) @binding(0)
var text_texture: texture_2d<f32>;
@group(1) @binding(1)
var text_sampler: sampler;

@vertex
fn text_vs(vertex: TextVertex) -> TextOutput {
    var out: TextOutput;
    let ndc = vec2<f32>(
        vertex.position.x / screen.size.x * 2.0 - 1.0,
        1.0 - vertex.position.y / screen.size.y * 2.0,
    );
    let world_position = vertex.position;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = vertex.uv;
    out.world_position = world_position;
    return out;
}

@fragment
fn text_fs(input: TextOutput) -> @location(0) vec4<f32> {
    if (!inside_clips(input.world_position)) {
        discard;
    }
    return textureSample(text_texture, text_sampler, input.uv);
}
