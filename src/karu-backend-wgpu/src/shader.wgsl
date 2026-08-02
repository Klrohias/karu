struct Screen {
    size: vec2<f32>,
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
};

@vertex
fn shape_vs(vertex: ShapeVertex) -> ShapeOutput {
    var out: ShapeOutput;
    let ndc = vec2<f32>(
        vertex.position.x / screen.size.x * 2.0 - 1.0,
        1.0 - vertex.position.y / screen.size.y * 2.0,
    );
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.color = vertex.color;
    return out;
}

@fragment
fn shape_fs(input: ShapeOutput) -> @location(0) vec4<f32> {
    return input.color;
}

struct TextVertex {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct TextOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
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
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = vertex.uv;
    return out;
}

@fragment
fn text_fs(input: TextOutput) -> @location(0) vec4<f32> {
    return textureSample(text_texture, text_sampler, input.uv);
}
