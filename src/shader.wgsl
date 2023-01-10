@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var s_diffuse: sampler;

struct CameraUniform {
    view_proj: mat4x4<f32>,
}
@group(1) @binding(0)
var<uniform> camera: CameraUniform;
@group(1) @binding(1)
var<uniform> model_matrix: mat4x4<f32>;
@group(1) @binding(2)
var<uniform> resolution: vec2<f32>;
@group(1) @binding(3)
var<uniform> time: f32;
@group(1) @binding(4)
var<uniform> which: u32;

/*
Corresponding `which` values
0 -> A
1 -> B
2 -> X
3 -> Y
4 -> Start
5 -> Z
6 -> Main Stick
7 -> C Stick
8 -> Left Trigger
9 -> Right Trigger
10 -> Dpad Up
11 -> Dpad Down
12 -> Dpad Left
13 -> Dpad Right
*/

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

fn rgb_to_srgb(v: f32) -> f32 {
    return pow((v + 0.055) / 1.055, 2.4);
}

fn rgb_to_srgb4(v: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(
        rgb_to_srgb(v.r),
        rgb_to_srgb(v.g),
        rgb_to_srgb(v.b),
        v.a,
    );
}

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * model_matrix * vec4<f32>(model.position, 1.0);
    out.position = model.position;
    out.tex_coords = model.tex_coords;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let border = 0.15;
    // let r = dot(in.position, in.position);
    // if r > pow(0.5, 2.0) || r < pow(0.5 - r * border, 2.0) {
    //     discard;
    // }

    let dist = textureSample(t_diffuse, s_diffuse, in.tex_coords).r;
    if dist < 0.5 || dist > 0.5 + border {
        discard;
    }

    // let color = vec4<f32>((sin(time) + 1.0) / 2.0, in.position.y, 1.0, 1.0);
    var color: vec4<f32>;
    switch which {
        case 0u { // A
            color = vec4<f32>(0.0, 0.737, 0.556, 1.0);
        }
        case 1u { // B
            color = vec4<f32>(1.0, 0.0, 0.0, 1.0);
        }
        case 5u { // Z
            color = vec4<f32>(0.333, 0.0, 0.678, 1.0);
        }
        case 7u { // C Stick
            color = vec4<f32>(1.0, 0.894, 0.0, 1.0);
        }
        default {
            color = vec4<f32>(0.95, 0.95, 0.95, 1.0);
        }
    }

    return rgb_to_srgb4(color);
}
