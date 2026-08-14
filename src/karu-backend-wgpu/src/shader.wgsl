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

fn rounded_rect_distance(position: vec2<f32>, rect: vec4<f32>, radius: f32) -> f32 {
    let half_size = rect.zw * 0.5;
    let center = rect.xy + half_size;
    let r = min(radius, min(rect.z, rect.w) * 0.5);
    let local = abs(position - center) - (half_size - vec2<f32>(r, r));
    return length(max(local, vec2<f32>(0.0))) + min(max(local.x, local.y), 0.0) - r;
}

fn clip_coverage(position: vec2<f32>) -> f32 {
    var coverage = 1.0;
    for (var index: u32 = 0u; index < 8u; index = index + 1u) {
        if (index >= screen.clip_count) {
            break;
        }
        let radius = screen.clip_radii[index / 4u][index % 4u];
        let distance = rounded_rect_distance(position, screen.clip_rects[index], radius);
        let edge_width = max(fwidth(distance), 0.0001);
        let clip_alpha = 1.0 - smoothstep(-edge_width, edge_width, distance);
        coverage = min(coverage, clip_alpha);
    }
    return coverage;
}

@fragment
fn shape_fs(input: ShapeOutput) -> @location(0) vec4<f32> {
    let clip_alpha = clip_coverage(input.world_position);
    if (clip_alpha <= 0.0) {
        discard;
    }
    var color = input.color;
    color.a = color.a * clip_alpha;
    return color;
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
    let clip_alpha = clip_coverage(input.world_position);
    if (clip_alpha <= 0.0) {
        discard;
    }
    var color = textureSample(text_texture, text_sampler, input.uv);
    color.a = color.a * clip_alpha;
    return color;
}
