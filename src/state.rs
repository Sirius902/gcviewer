use std::time;

use gcinput::Input;
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::{
    camera::{Camera, CameraUniform},
    control::{Button, Control, Instance, InstanceRaw, Stick, Trigger},
    Vertex, INDICES, VERTICES,
};

const BEAN_SDF_IMAGE: &[u8] = include_bytes!("../resource/sdf/bean.png");
const Z_BUTTON_SDF_IMAGE: &[u8] = include_bytes!("../resource/sdf/z-button.png");
const OCTAGON_SDF_IMAGE: &[u8] = include_bytes!("../resource/sdf/octagon.png");

pub struct State {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    camera: Camera,
    diffuse_bind_group: wgpu::BindGroup,
    camera_uniform: CameraUniform,
    camera_buffer: wgpu::Buffer,
    resolution_buffer: wgpu::Buffer,
    time_buffer: wgpu::Buffer,
    main_bind_group: wgpu::BindGroup,
    start_time: time::Instant,
    instances: Vec<Instance>,
    instance_buffer: wgpu::Buffer,
}

impl State {
    pub async fn new(window: &Window) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::Backend::Vulkan.into());
        let surface = unsafe { instance.create_surface(window) };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::default(),
                    label: None,
                },
                None,
            )
            .await
            .unwrap();

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface.get_supported_formats(&adapter)[0],
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
        };
        surface.configure(&device, &config);

        let texture_views = [
            (BEAN_SDF_IMAGE, "bean_sdf"),
            (Z_BUTTON_SDF_IMAGE, "z_button_sdf"),
            (OCTAGON_SDF_IMAGE, "octagon_sdf"),
        ]
        .iter()
        .map(|(img_buf, name)| {
            let img = image::load_from_memory(img_buf).expect("failed to decode sdf image");
            let sdf = img.to_luma8();
            let dimensions = sdf.dimensions();

            let texture_size = wgpu::Extent3d {
                width: dimensions.0,
                height: dimensions.1,
                depth_or_array_layers: 1,
            };

            let diffuse_texture = device.create_texture(&wgpu::TextureDescriptor {
                size: texture_size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                label: Some(name),
            });

            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &diffuse_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &sdf,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: std::num::NonZeroU32::new(dimensions.0),
                    rows_per_image: std::num::NonZeroU32::new(dimensions.1),
                },
                texture_size,
            );

            diffuse_texture.create_view(&wgpu::TextureViewDescriptor::default())
        })
        .collect::<Vec<_>>();

        let diffuse_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        let camera = Camera {
            aspect: config.width as f32 / config.height as f32,
            znear: 0.1,
            zfar: 10.0,
        };

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                ],
                label: Some("texture_bind_group_layout"),
            });

        let diffuse_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&diffuse_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture_views[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&texture_views[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&texture_views[2]),
                },
            ],
            label: Some("diffuse_bind_group"),
        });

        let mut camera_uniform = CameraUniform::default();
        camera_uniform.update_view_proj(&camera);

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let resolution_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[config.width, config.height]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let start_time = time::Instant::now();
        let time_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Time Buffer"),
            contents: bytemuck::cast_slice(&[start_time.elapsed().as_secs_f32()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let main_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
                label: Some("main_bind_group_layout"),
            });

        let main_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &main_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: resolution_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: time_buffer.as_entire_binding(),
                },
            ],
            label: Some("main_bind_group"),
        });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&texture_bind_group_layout, &main_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc(), InstanceRaw::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let instances = Self::gen_instances(&Input::default());

        let instance_data = instances.iter().map(Instance::to_raw).collect::<Vec<_>>();
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Instance Buffer"),
            contents: bytemuck::cast_slice(&instance_data),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            surface,
            device,
            queue,
            config,
            size,
            render_pipeline,
            vertex_buffer,
            index_buffer,
            num_indices: INDICES.len() as u32,
            camera,
            diffuse_bind_group,
            camera_uniform,
            camera_buffer,
            resolution_buffer,
            time_buffer,
            main_bind_group,
            start_time,
            instances,
            instance_buffer,
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn update(&mut self, input: &Input) {
        self.instances = Self::gen_instances(input);
        let instance_data = self
            .instances
            .iter()
            .map(Instance::to_raw)
            .collect::<Vec<_>>();
        self.queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&instance_data),
        );

        self.queue.write_buffer(
            &self.resolution_buffer,
            0,
            bytemuck::cast_slice(&[self.config.width, self.config.height]),
        );

        self.camera.update(&self.config);
        self.camera_uniform.update_view_proj(&self.camera);
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[self.camera_uniform]),
        );
        self.queue.write_buffer(
            &self.time_buffer,
            0,
            bytemuck::cast_slice(&[self.start_time.elapsed().as_secs_f32()]),
        );
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: true,
                    },
                })],
                depth_stencil_attachment: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.diffuse_bind_group, &[]);
            render_pass.set_bind_group(1, &self.main_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..self.num_indices, 0, 0..self.instances.len() as _);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    fn stick_to_vec2(stick: &gcinput::Stick) -> cgmath::Vector2<f32> {
        let f = |n: u8| ((u8::MAX - n) as f32 / u8::MAX as f32 - 0.5) * 0.6;
        cgmath::vec2(f(stick.x), f(stick.y))
    }

    fn gen_instances(input: &Input) -> Vec<Instance> {
        vec![
            Instance {
                control: Control::Button {
                    button: Button::A,
                    pressed: input.button_a,
                },
                position: cgmath::vec2(0.5, -0.075),
                rotation: cgmath::Deg(0.0),
                scale: 0.302,
            },
            Instance {
                control: Control::Button {
                    button: Button::B,
                    pressed: input.button_b,
                },
                position: cgmath::vec2(0.275, -0.225),
                rotation: cgmath::Deg(0.0),
                scale: 0.17,
            },
            Instance {
                control: Control::Button {
                    button: Button::X,
                    pressed: input.button_x,
                },
                position: cgmath::vec2(0.75, -0.075),
                rotation: cgmath::Deg(225.0),
                scale: 0.275,
            },
            Instance {
                control: Control::Button {
                    button: Button::Y,
                    pressed: input.button_y,
                },
                position: cgmath::vec2(0.4, 0.15),
                rotation: cgmath::Deg(-20.0),
                scale: 0.275,
            },
            Instance {
                control: Control::Button {
                    button: Button::Start,
                    pressed: input.button_start,
                },
                position: cgmath::vec2(0.175, -0.025),
                rotation: cgmath::Deg(0.0),
                scale: 0.126,
            },
            Instance {
                control: Control::Button {
                    button: Button::Z,
                    pressed: input.button_z,
                },
                position: cgmath::vec2(0.685, 0.21),
                rotation: cgmath::Deg(-80.0),
                scale: 0.225,
            },
            Instance {
                control: Control::Stick {
                    stick: Stick::Main,
                    position: Self::stick_to_vec2(&input.main_stick),
                },
                position: cgmath::vec2(-0.65, 0.0),
                rotation: cgmath::Deg(0.0),
                scale: 0.504,
            },
            Instance {
                control: Control::Stick {
                    stick: Stick::C,
                    position: Self::stick_to_vec2(&input.c_stick),
                },
                position: cgmath::vec2(-0.15, 0.0),
                rotation: cgmath::Deg(0.0),
                scale: 0.504,
            },
            Instance {
                control: Control::Trigger {
                    trigger: Trigger::Left,
                    fill: input.left_trigger as f32 / u8::MAX as f32,
                    pressed: input.button_left,
                },
                position: cgmath::vec2(-0.65, 0.35),
                rotation: cgmath::Deg(0.0),
                scale: 0.375,
            },
            Instance {
                control: Control::Trigger {
                    trigger: Trigger::Right,
                    fill: input.right_trigger as f32 / u8::MAX as f32,
                    pressed: input.button_right,
                },
                position: cgmath::vec2(-0.15, 0.35),
                rotation: cgmath::Deg(0.0),
                scale: 0.375,
            },
            Instance {
                control: Control::Button {
                    button: Button::Up,
                    pressed: input.button_up,
                },
                position: cgmath::vec2(-0.4, -0.205),
                rotation: cgmath::Deg(0.0),
                scale: 0.109,
            },
            Instance {
                control: Control::Button {
                    button: Button::Down,
                    pressed: input.button_down,
                },
                position: cgmath::vec2(-0.4, -0.395),
                rotation: cgmath::Deg(0.0),
                scale: 0.109,
            },
            Instance {
                control: Control::Button {
                    button: Button::Left,
                    pressed: input.button_left,
                },
                position: cgmath::vec2(-0.495, -0.3),
                rotation: cgmath::Deg(0.0),
                scale: 0.109,
            },
            Instance {
                control: Control::Button {
                    button: Button::Right,
                    pressed: input.button_right,
                },
                position: cgmath::vec2(-0.305, -0.3),
                rotation: cgmath::Deg(0.0),
                scale: 0.109,
            },
        ]
    }
}
