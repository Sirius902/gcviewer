struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
};

// SRGB -> RGB
fn color_correct(v: f32) -> f32 {
    return pow((v + 0.055) / 1.055, 2.4);
}

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(model.position, 1.0);
    out.position = model.position;
    out.color = model.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let srgb_color = vec3<f32>(
        color_correct(in.color.r),
        color_correct(in.color.g),
        color_correct(in.color.b),
    );

    return vec4<f32>(srgb_color, 1.0);
}
