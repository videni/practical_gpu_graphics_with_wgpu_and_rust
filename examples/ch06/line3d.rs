use bytemuck::{cast_slice, Pod, Zeroable};
use cgmath::*;
use std::{iter, mem};
use wgpu::util::DeviceExt;
use winit::{
    event::{ElementState, Event, KeyEvent, WindowEvent},
    event_loop::EventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowAttributes},
};

#[path = "../common/transforms.rs"]
mod transforms;

const IS_PERSPECTIVE: bool = true;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
}

fn create_vertices() -> [Vertex; 300] {
    let mut vertices = [Vertex {
        position: [0.0, 0.0, 0.0],
    }; 300];

    for i in 0..300 {
        let t = 0.1 * (i as f32) / 30.0;
        let x = (-t).exp() * (30.0 * t).sin();
        let z = (-t).exp() * (30.0 * t).cos();
        let y = 2.0 * t - 1.0;

        vertices[i] = Vertex {
            position: [x, y, z],
        };
    }
    vertices
}

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0=>Float32x3];
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}
struct State<'a> {
    init: transforms::InitWgpu<'a>,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    model_mat: Matrix4<f32>,
    view_mat: Matrix4<f32>,
    project_mat: Matrix4<f32>,
}

impl<'a> State<'a> {
    async fn new(window: &'a Window) -> Self {
        let init = transforms::InitWgpu::init_wgpu(window).await;

        let shader = init
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("line3d.wgsl").into()),
            });
        // uniform data
        let camera_position = (1.5, 1.0, 3.0).into();

        let look_direction = (0.0, 0.0, 0.0).into();

        let up_direction = cgmath::Vector3::unit_y();

        let model_mat =
            transforms::create_transforms([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);

        let (view_mat, project_mat, view_project_mat) = transforms::create_view_projection(
            camera_position,
            look_direction,
            up_direction,
            init.config.width as f32 / init.config.height as f32,
            false,
        );

        let mvp_mat = view_project_mat * model_mat;

        let mvp_ref: &[f32; 16] = mvp_mat.as_ref();

        let uniform_buffer = init
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Uniform Buffer"),
                contents: bytemuck::cast_slice(mvp_ref),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let uniform_bind_group_layout =
            init.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                    label: Some("Uniform Bind Group Layout"),
                });

        let uniform_bind_group = init.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
            label: Some("Uniform Bind Group"),
        });

        let pipeline_layout = init
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&uniform_bind_group_layout],
                push_constant_ranges: &[],
            });

        let pipeline = init
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Render Pipeline"),
                cache: None,
                multiview: None,
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: "vs_main",
                    buffers: &[Vertex::desc()],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: "fs_main",
                    targets: &[Some(wgpu::ColorTargetState {
                        format: init.config.format,
                        blend: Some(wgpu::BlendState {
                            color: wgpu::BlendComponent::REPLACE,
                            alpha: wgpu::BlendComponent::REPLACE,
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::LineStrip,
                    strip_index_format: Some(wgpu::IndexFormat::Uint32),
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
            });
        let vertex_buffer = init
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: cast_slice(&create_vertices()),
                usage: wgpu::BufferUsages::VERTEX,
            });

        Self {
            init,
            pipeline,
            vertex_buffer,
            uniform_buffer,
            uniform_bind_group,
            model_mat,
            view_mat,
            project_mat,
        }
    }
    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.init.size = new_size;
            self.init.config.width = new_size.width;
            self.init.config.height = new_size.height;
            self.init
                .surface
                .configure(&self.init.device, &self.init.config);
            self.project_mat = transforms::create_projection(
                new_size.width as f32 / new_size.height as f32,
                IS_PERSPECTIVE,
            );

            let mvp_mat = self.project_mat * self.view_mat * self.model_mat;

            let mvp_ref: &[f32; 16] = mvp_mat.as_ref();
            self.init
                .queue
                .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(mvp_ref));
        }
    }
    #[allow(unused_variables)]
    fn input(&mut self, event: &WindowEvent) -> bool {
        return false;
    }
    fn update(&mut self) {}
    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.init.surface.get_current_texture()?;
        print!("render");
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder =
            self.init
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Render Encoder"),
                });
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                timestamp_writes: None,
                occlusion_query_set: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.2,
                            g: 0.247,
                            b: 0.314,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
            });
            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            render_pass.draw(0..300, 0..1);
        }

        self.init.queue.submit(iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}
fn main() {
    env_logger::init();

    let event_loop = EventLoop::new().unwrap();
    let window: winit::window::Window = event_loop
        .create_window(WindowAttributes::default())
        .unwrap();
    window.set_title(&*format!("{}", "ch06-3d-line"));
    let current_window_id = window.id();
    let mut state = pollster::block_on(State::new(&window));
    event_loop
        .run(move |event: Event<()>, target| {
            if let Event::WindowEvent {
                window_id: _,
                event,
            } = event
            {
                match event {
                    WindowEvent::CloseRequested
                    | WindowEvent::KeyboardInput {
                        event:
                            KeyEvent {
                                state: ElementState::Pressed,
                                physical_key: PhysicalKey::Code(KeyCode::Escape),
                                ..
                            },
                        ..
                    } => target.exit(),
                    WindowEvent::Resized(physical_size) => {
                        state.resize(physical_size);
                        state.render();
                    }
                    WindowEvent::RedrawRequested => {
                        dbg!("{:?}", &event);
                        state.update();
                        match state.render() {
                            Ok(_) => {}
                            Err(wgpu::SurfaceError::Lost) => state.resize(state.init.size),
                            Err(wgpu::SurfaceError::OutOfMemory) => target.exit(),
                            Err(e) => eprintln!("{:?}", e),
                        }
                    }
                    // WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                    //     state.resize(**new_inner_size);
                    // }
                    _ => {}
                }
            }
        })
        .unwrap();
}
