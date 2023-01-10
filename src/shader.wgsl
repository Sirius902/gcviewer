@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var s_diffuse: sampler;

@group(1) @binding(0)
var<uniform> resolution: vec2<f32>;

struct CameraUniform {
    view_proj: mat4x4<f32>,
}
@group(2) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

// SRGB -> RGB
fn color_correct(v: f32) -> f32 {
    return pow((v + 0.055) / 1.055, 2.4);
}

fn color_correct4(v: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(
        color_correct(v.r),
        color_correct(v.g),
        color_correct(v.b),
        v.a,
    );
}

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(model.position, 1.0);
    out.position = model.position;
    out.tex_coords = model.tex_coords;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dist = textureSample(t_diffuse, s_diffuse, in.tex_coords).r;
    if dist > 0.5 {
        return color_correct4(vec4<f32>(0.3, 0.0, 0.69, 1.0));
    } else {
        discard;
    }
}
