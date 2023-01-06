use crate::OPENGL_TO_WGPU_MATRIX;

pub struct Camera {
    pub aspect: f32,
    pub znear: f32,
    pub zfar: f32,
}

impl Camera {
    pub fn build_projection_view_matrix(&self) -> cgmath::Matrix4<f32> {
        let view = cgmath::Matrix4::from_translation(cgmath::Vector3::new(0.0, 0.0, -1.0));

        let tw = 2.0f32;
        let th = 1.0f32;
        let taspect = tw / th;

        let proj = if self.aspect > taspect {
            cgmath::ortho(
                -self.aspect / taspect * tw / 2.0,
                self.aspect / taspect * tw / 2.0,
                -th / 2.0,
                th / 2.0,
                self.znear,
                self.zfar,
            )
        } else {
            cgmath::ortho(
                -tw / 2.0,
                tw / 2.0,
                -taspect / self.aspect * th / 2.0,
                taspect / self.aspect * th / 2.0,
                self.znear,
                self.zfar,
            )
        };

        OPENGL_TO_WGPU_MATRIX * proj * view
    }

    pub fn update(&mut self, config: &wgpu::SurfaceConfiguration) {
        self.aspect = config.width as f32 / config.height as f32;
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
}

impl CameraUniform {
    pub fn update_view_proj(&mut self, camera: &Camera) {
        self.view_proj = camera.build_projection_view_matrix().into();
    }
}

impl Default for CameraUniform {
    fn default() -> Self {
        use cgmath::SquareMatrix;
        Self {
            view_proj: cgmath::Matrix4::identity().into(),
        }
    }
}
