use std::mem;

#[derive(Debug)]
pub struct Instance {
    pub control: Control,
    pub position: cgmath::Vector2<f32>,
    pub rotation: cgmath::Deg<f32>,
    pub scale: Scale,
}

impl Instance {
    pub fn to_raw(&self) -> InstanceRaw {
        let (which, which_texture, pressed, fill, position) = match &self.control {
            Control::Button { button, pressed } => {
                let which_texture = match button {
                    Button::Z => 1,
                    _ => 0,
                };
                (*button as u32, which_texture, *pressed, 0.0, [0.0, 0.0])
            }
            Control::Stick { stick, position } => {
                (*stick as u32, 0, false, 0.0, (*position).into())
            }
            Control::Trigger {
                trigger,
                fill,
                pressed,
            } => (*trigger as u32, 0, *pressed, *fill, [0.0, 0.0]),
            Control::Misc(m) => (*m as u32, 0, false, 0.0, [0.0, 0.0]),
        };

        let (scale_x, scale_y, uniform_scale) = match self.scale {
            Scale::Uniform(s) => (s, s, s),
            Scale::NonUniform(x, y) => (x, y, 1.0),
        };

        let rotate = cgmath::Matrix4::from_angle_z(self.rotation);
        let scale = cgmath::Matrix4::from_nonuniform_scale(scale_x, scale_y, 1.0);
        let translate =
            cgmath::Matrix4::from_translation(cgmath::vec3(self.position.x, self.position.y, 0.0));

        InstanceRaw {
            model_matrix: (translate * rotate * scale).into(),
            scale: uniform_scale,
            which,
            which_texture,
            button_pressed: pressed.into(),
            trigger_fill: fill,
            stick_position: position,
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceRaw {
    pub model_matrix: [[f32; 4]; 4],
    pub scale: f32,
    pub which: u32,
    pub which_texture: u32,
    pub button_pressed: u32,
    pub trigger_fill: f32,
    pub stick_position: [f32; 2],
}

impl InstanceRaw {
    const ATTRIBS: [wgpu::VertexAttribute; 10] = wgpu::vertex_attr_array![
        5 => Float32x4,
        6 => Float32x4,
        7 => Float32x4,
        8 => Float32x4,
        9 => Float32,
        10 => Uint32,
        11 => Uint32,
        12 => Uint32,
        13 => Float32,
        14 => Float32x2,
    ];

    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum Scale {
    Uniform(f32),
    NonUniform(f32, f32),
}

#[derive(Debug, Copy, Clone)]
pub enum Button {
    A = 0,
    B = 1,
    X = 2,
    Y = 3,
    Start = 4,
    Z = 5,
    Up = 10,
    Down = 11,
    Left = 12,
    Right = 13,
}

#[derive(Debug, Copy, Clone)]
pub enum Stick {
    Main = 6,
    C = 7,
}

#[derive(Debug, Copy, Clone)]
pub enum Trigger {
    Left = 8,
    Right = 9,
}

#[derive(Debug, Copy, Clone)]
pub enum Misc {
    Background = 14,
}

#[derive(Debug)]
pub enum Control {
    Button {
        button: Button,
        pressed: bool,
    },
    Stick {
        stick: Stick,
        position: cgmath::Vector2<f32>,
    },
    Trigger {
        trigger: Trigger,
        fill: f32,
        pressed: bool,
    },
    Misc(Misc),
}
