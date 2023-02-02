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

const BUTTON_A = 0u;
const BUTTON_B = 1u;
const BUTTON_X = 2u;
const BUTTON_Y = 3u;
const BUTTON_START = 4u;
const BUTTON_Z = 5u;
const STICK_MAIN = 6u;
const STICK_C = 7u;
const TRIGGER_LEFT = 8u;
const TRIGGER_RIGHT = 9u;
const BUTTON_UP = 10u;
const BUTTON_LEFT = 11u;
const BUTTON_RIGHT = 12u;
const BUTTON_DOWN = 13u;
const BACKGROUND = 14u;

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
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) scale: f32,
    @location(3) which: u32,
    @location(4) which_texture: u32,
    @location(5) button_pressed: u32,
    @location(6) trigger_fill: f32,
    @location(7) stick_position: vec2<f32>,
}

fn border_width(in: VertexOutput) -> f32 {
    return 0.095 / in.scale;
}

fn clip_circle_button(in: VertexOutput) {
    let r = length(in.position);
    // TODO: Make border width more accurate.
    if r > 0.5 || ((in.button_pressed == 0u)
        && r < 0.5 - (0.725 * r) * border_width(in)) {
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

fn clip_stick(in: VertexOutput) {
    let bw = border_width(in);
    let is_c_stick = in.which == 7u;
    var radius = 0.225;
    if is_c_stick {
        radius *= 0.8;
    }

    let center = in.position + in.stick_position;
    let dist = radius - length(center);

    let scaled_uv = (in.tex_coords - 0.5) / 0.85 + 0.5;
    let sdf_dist = textureSample(octagon_t_diffuse, s_diffuse, scaled_uv).r;

    if (dist < 0.0 && (sdf_dist < 0.5 - bw || sdf_dist > 0.5))
        || (!is_c_stick && dist > radius * bw) {
        discard;
    }
}

fn clip_trigger(in: VertexOutput) {
    let bw = border_width(in);
    let fill = in.trigger_fill;
    let radius = 0.725 * bw;
    // fixes border width being too thin
    let rbw = 1.5 * radius * bw;

    let threshold = 0.75;
    let scale = 1.0 / threshold;

    if in.position.x <= radius - 0.5 {
        // clip left
        let pos = in.position + vec2<f32>(0.5 - radius, 0.0);
        let dist = radius - length(pos);

        if dist < 0.0 || ((in.position.x + 0.5 > clamp(fill, 0.0, threshold) * scale)
            && (dist >= rbw)) {
            discard;
        }
    } else if in.position.x >= 0.5 - radius {
        // clip right
        let pos = in.position - vec2<f32>(0.5 - radius, 0.0);
        let dist = radius - length(pos);

        if dist < 0.0 || ((in.position.x + 0.5 > clamp(fill, 0.0, threshold) * scale)
            && (dist >= rbw)) {
            discard;
        }
    } else {
        // clip middle
        if (abs(in.position.y) > radius) || ((abs(in.position.y) <= radius - rbw)
            && (in.position.x + 0.5 > clamp(fill, 0.0, threshold) * scale)) {
            discard;
        }
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
    out.position = model.position.xy;
    out.tex_coords = model.tex_coords;
    out.scale = instance.scale;
    out.which = instance.which;
    out.which_texture = instance.which_texture;
    out.button_pressed = instance.button_pressed;
    out.trigger_fill = instance.trigger_fill;
    out.stick_position = instance.stick_position;
    return out;
}

const minv = mat3x3<f32>(
    vec3<f32>( 3.24100323, -0.96922425,  0.05563942),
    vec3<f32>(-1.53739897,  1.87592998, -0.20401121),
    vec3<f32>(-0.49861588,  0.04155423,  1.05714898),
);

fn gamma(u: f32) -> f32 {
    if u <= 0.0031308 {
        return 12.92 * u;
    } else {
        return (1.055 * pow(u, 1.0 / 2.4)) - 0.055;
    }
}

fn get_d65() -> vec3<f32> {
    let d65_cie = vec2<f32>(0.31271, 0.32902);
    return vec3<f32>(
        (1.0 / d65_cie.y) * d65_cie.x,
        1.0,
        (1.0 / d65_cie.y) * (1.0 - d65_cie.x - d65_cie.y),
    );
}

fn inversef(t: f32) -> f32 {
    let delta = 6.0 / 29.0;

    if t > delta {
        return pow(t, 3.0);
    } else {
        return 3.0 * pow(delta, 2.0) * (t - (4.0 / 29.0));
    }
}

fn rgb_to_srgb(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(gamma(c.x), gamma(c.y), gamma(c.z));
}

fn xyz_to_rgb(c: vec3<f32>) -> vec3<f32> {
    return minv * c;
}

fn lab_to_xyz(c: vec3<f32>) -> vec3<f32> {
    let d65 = get_d65();
    let x = d65.x * inversef(((c.x + 16.0) / 116.0) + (c.y / 500.0));
    let y = d65.y * inversef((c.x + 16.0) / 116.0);
    let z = d65.z * inversef(((c.x + 16.0) / 116.0) - (c.z / 200.0));
    return vec3<f32>(x, y, z);
}

fn lch_to_lab(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(c.x, c.y * cos(c.z), c.y * sin(c.z));
}

fn wave_color(in: VertexOutput, lum: f32, chrom: f32) -> vec4<f32> {
    let screen_pos = (in.clip_position.xy / resolution) * vec2<f32>(1.0, -1.0)
        + vec2<f32>(0.0, 1.0);

    return vec4<f32>(rgb_to_srgb(xyz_to_rgb(lab_to_xyz(lch_to_lab(
        vec3<f32>(lum, chrom, time + 2.0 * screen_pos.x)
    )))), 1.0);
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
        case 6u, 7u { // Main Stick, C Stick
            clip_stick(in);
        }
        case 8u, 9u { // Left Trigger, Right Trigger
            clip_trigger(in);
        }
        default {}
    }

    var color: vec4<f32>;
    switch in.which {
        case 14u { // Background
            color = vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }
        default {
            color = wave_color(in, 80.0, 100.0);
        }
    }

    return color;
}
