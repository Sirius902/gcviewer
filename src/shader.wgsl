@group(0) @binding(0)
var s_diffuse: sampler;
@group(0) @binding(1)
var bean_t_diffuse: texture_2d<f32>;
@group(0) @binding(2)
var z_t_diffuse: texture_2d<f32>;
@group(0) @binding(3)
var octagon_t_diffuse: texture_2d<f32>;

struct CameraUniform {
    view_proj: mat4x4<f32>,
}
@group(1) @binding(0)
var<uniform> camera: CameraUniform;
@group(1) @binding(1)
var<uniform> resolution: vec2<f32>;
@group(1) @binding(2)
var<uniform> time: f32;

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
11 -> Dpad Left
12 -> Dpad Right
13 -> Dpad Down
*/

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct InstanceInput {
    @location(5) model_matrix_0: vec4<f32>,
    @location(6) model_matrix_1: vec4<f32>,
    @location(7) model_matrix_2: vec4<f32>,
    @location(8) model_matrix_3: vec4<f32>,
    @location(9) scale: f32,
    @location(10) which: u32,
    @location(11) which_texture: u32,
    @location(12) button_pressed: u32,
    @location(13) trigger_fill: f32,
    @location(14) stick_position: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) scale: f32,
    @location(3) which: u32,
    @location(4) which_texture: u32,
    @location(5) button_pressed: u32,
    @location(6) trigger_fill: f32,
    @location(7) stick_position: vec2<f32>,
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

fn border_width(in: VertexOutput) -> f32 {
    return 0.1 / in.scale;
}

fn clip_circle_button(in: VertexOutput) {
    let r = length(in.position);
    // TODO: Make border width more accurate.
    if r > 0.5 || ((in.button_pressed == 0u) && r < 0.5 - (0.75 * r) * border_width(in)) {
        discard;
    }
}

fn clip_sdf_button(in: VertexOutput) {
    let bean_dist = textureSample(bean_t_diffuse, s_diffuse, in.tex_coords).r;
    let z_dist = textureSample(z_t_diffuse, s_diffuse, in.tex_coords).r;
    let octagon_dist = textureSample(octagon_t_diffuse, s_diffuse, in.tex_coords).r;

    var dist: f32;
    switch in.which_texture {
        case 0u {
            dist = bean_dist;
        }
        case 1u {
            dist = z_dist;
        }
        case 2u {
            dist = octagon_dist;
        }
        default {
            dist = 0.0;
        }
    }

    if dist < 0.5 - border_width(in) || ((in.button_pressed == 0u) && dist > 0.5) {
        discard;
    }
}

@vertex
fn vs_main(
    model: VertexInput,
    instance: InstanceInput,
) -> VertexOutput {
    let model_matrix = mat4x4<f32>(
        instance.model_matrix_0,
        instance.model_matrix_1,
        instance.model_matrix_2,
        instance.model_matrix_3,
    );

    var out: VertexOutput;
    out.clip_position = camera.view_proj * model_matrix * vec4<f32>(model.position, 1.0);
    out.position = model.position;
    out.tex_coords = model.tex_coords;
    out.scale = instance.scale;
    out.which = instance.which;
    out.which_texture = instance.which_texture;
    out.button_pressed = instance.button_pressed;
    out.trigger_fill = instance.trigger_fill;
    out.stick_position = instance.stick_position;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    switch in.which {
        case 0u, 1u, 4u, 10u, 11u, 12u, 13u { // A, B, Start, Dpad
            clip_circle_button(in);
        }
        case 2u, 3u, 5u { // X, Y, Z
            clip_sdf_button(in);
        }
        default {
            // TODO: Implement drawing sticks and triggers.
            clip_circle_button(in);
        }
    }

    var color: vec4<f32>;
    switch in.which {
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
