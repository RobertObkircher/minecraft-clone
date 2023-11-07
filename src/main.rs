use std::borrow::Cow;
use std::f32::consts::{PI, TAU};
use std::mem;
use std::time::Duration;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use log::info;
use pollster;
use wgpu::{Color, CompareFunction, DepthStencilState, Device, Face, FragmentState, LoadOp, MultisampleState, Operations, PrimitiveState, RenderPassColorAttachment, RenderPassDepthStencilAttachment, RenderPassDescriptor, RenderPipelineDescriptor, StoreOp, SurfaceConfiguration, Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureView, TextureViewDescriptor, VertexState};
use wgpu::util::DeviceExt;
use winit::event::{DeviceEvent, ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::EventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorGrabMode, Window, WindowBuilder};

use crate::camera::Camera;

mod camera;

fn main() {
    env_logger::init();
    info!("Hello, paper-world!");

    let event_loop = EventLoop::new().unwrap();

    let window = WindowBuilder::new()
        .with_title("Paper world!")
        .with_inner_size(winit::dpi::LogicalSize::new(800.0, 600.0))
        .build(&event_loop)
        .unwrap();

    pollster::block_on(run(event_loop, window));
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 4],
    tex_coord: [f32; 2],
}

fn vertex(pos: [i8; 3], tc: [i8; 2]) -> Vertex {
    Vertex {
        pos: [pos[0] as f32, pos[1] as f32, pos[2] as f32, 1.0],
        tex_coord: [tc[0] as f32, tc[1] as f32],
    }
}

fn create_vertices() -> (Vec<Vertex>, Vec<u16>) {
    let vertex_data = [
        // top (0, 0, 1)
        vertex([-1, -1, 1], [0, 0]),
        vertex([1, -1, 1], [1, 0]),
        vertex([1, 1, 1], [1, 1]),
        vertex([-1, 1, 1], [0, 1]),
        // bottom (0, 0, -1)
        vertex([-1, 1, -1], [1, 0]),
        vertex([1, 1, -1], [0, 0]),
        vertex([1, -1, -1], [0, 1]),
        vertex([-1, -1, -1], [1, 1]),
        // right (1, 0, 0)
        vertex([1, -1, -1], [0, 0]),
        vertex([1, 1, -1], [1, 0]),
        vertex([1, 1, 1], [1, 1]),
        vertex([1, -1, 1], [0, 1]),
        // left (-1, 0, 0)
        vertex([-1, -1, 1], [1, 0]),
        vertex([-1, 1, 1], [0, 0]),
        vertex([-1, 1, -1], [0, 1]),
        vertex([-1, -1, -1], [1, 1]),
        // front (0, 1, 0)
        vertex([1, 1, -1], [1, 0]),
        vertex([-1, 1, -1], [0, 0]),
        vertex([-1, 1, 1], [0, 1]),
        vertex([1, 1, 1], [1, 1]),
        // back (0, -1, 0)
        vertex([1, -1, 1], [0, 0]),
        vertex([-1, -1, 1], [1, 0]),
        vertex([-1, -1, -1], [1, 1]),
        vertex([1, -1, -1], [0, 1]),
    ];

    let index_data: &[u16] = &[
        0, 1, 2, 2, 3, 0, // top
        4, 5, 6, 6, 7, 4, // bottom
        8, 9, 10, 10, 11, 8, // right
        12, 13, 14, 14, 15, 12, // left
        16, 17, 18, 18, 19, 16, // front
        20, 21, 22, 22, 23, 20, // back
    ];

    let mut vd = Vec::with_capacity(vertex_data.len() * 4);
    vd.extend_from_slice(&vertex_data);
    vd.extend(vertex_data.iter().cloned().map(|mut it| {
        it.pos[0] += 2.2;
        it
    }));
    vd.extend(vertex_data.iter().cloned().map(|mut it| {
        it.pos[1] += 2.2;
        it
    }));
    vd.extend(vertex_data.iter().cloned().map(|mut it| {
        it.pos[2] += 2.2;
        it
    }));

    let mut id = Vec::with_capacity(index_data.len() * 4);
    id.extend_from_slice(&index_data);
    id.extend(index_data.iter().map(|it| it + vertex_data.len() as u16));
    id.extend(index_data.iter().map(|it| it + (vertex_data.len() * 2) as u16));
    id.extend(index_data.iter().map(|it| it + (vertex_data.len() * 3) as u16));

    (vd, id)
}

fn generate_matrix(aspect_ratio: f32, camera: &Camera) -> Mat4 {
    let fov_y_radians = PI / 4.0;
    let projection = Mat4::perspective_rh(fov_y_radians, aspect_ratio, 0.1, 100.0);

    let vs = camera.computed_vectors();
    let view = Mat4::look_to_rh(camera.position, vs.direction, vs.up);

    projection * view
}

const DEPTH_TEXTURE_FORMAT: TextureFormat = TextureFormat::Depth32Float;

pub fn create_depth_texture(device: &Device, config: &SurfaceConfiguration) -> (Texture, TextureView) {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: DEPTH_TEXTURE_FORMAT,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let depth_view = texture.create_view(&TextureViewDescriptor::default());

    (texture, depth_view)
}


async fn run(event_loop: EventLoop<()>, window: Window) {
    let instance = wgpu::Instance::default();
    let surface = unsafe { instance.create_surface(&window) }.unwrap();

    let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        force_fallback_adapter: false,
        compatible_surface: Some(&surface),
    }).await.expect("Failed to find an appropriate adapter");

    let (device, queue) = adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: None,
            features: wgpu::Features::empty(),
            // Make sure we use the texture resolution limits from the adapter, so we can support images the size of the swapchain.
            limits: wgpu::Limits::downlevel_webgl2_defaults()
                .using_resolution(adapter.limits()),
        },
        None,
    ).await.expect("Failed to create device");

    let swapchain_capabilities = surface.get_capabilities(&adapter);
    let swapchain_format = swapchain_capabilities.formats[0];
    let size = window.inner_size();
    let mut config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: swapchain_format,
        width: size.width,
        height: size.height,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: swapchain_capabilities.alpha_modes[0],
        view_formats: vec![],
    };

    surface.configure(&device, &config);

    let vertex_size = mem::size_of::<Vertex>();
    let (vertex_data, index_data) = create_vertices();

    let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Vertex Buffer"),
        contents: bytemuck::cast_slice(&vertex_data),
        usage: wgpu::BufferUsages::VERTEX,
    });

    let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Index Buffer"),
        contents: bytemuck::cast_slice(&index_data),
        usage: wgpu::BufferUsages::INDEX,
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(64),
                },
                count: None,
            }
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let mut camera = Camera::new(Vec3::new(6.0, 6.0, 6.0));
    camera.turn_right(-TAU / 3.0);
    camera.turn_up(-PI / 2.0 / 3.0);

    let mx_total = generate_matrix(config.width as f32 / config.height as f32, &camera);
    let mx_ref: &[f32; 16] = mx_total.as_ref();
    let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Uniform Buffer"),
        contents: bytemuck::cast_slice(mx_ref),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }
        ],
        label: None,
    });


    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shader.wgsl"))),
    });

    let vertex_buffers = [wgpu::VertexBufferLayout {
        array_stride: vertex_size as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 4 * 4,
                shader_location: 1,
            },
        ],
    }];

    let mut depth = create_depth_texture(&device, &config);

    let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &vertex_buffers,
        },
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(swapchain_format.into())],
        }),
        primitive: PrimitiveState {
            cull_mode: Some(Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(DepthStencilState {
            format: DEPTH_TEXTURE_FORMAT,
            depth_write_enabled: true,
            depth_compare: CompareFunction::Less,
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: MultisampleState::default(),
        multiview: None,
    });

    let delta_time = Duration::from_millis(16).as_secs_f32();

    let mut is_locked = false;
    event_loop.run(move |event, target| {
        let id = window.id();
        match event {
            Event::WindowEvent { event, window_id } if window_id == id => {
                match event {
                    WindowEvent::Resized(new_size) => {
                        config.width = new_size.width;
                        config.height = new_size.height;
                        surface.configure(&device, &config);
                        depth = create_depth_texture(&device, &config);

                        // necessary on macos, according to hello triangle example
                        window.request_redraw();
                    }
                    WindowEvent::CloseRequested => {
                        target.exit();
                    }
                    WindowEvent::RedrawRequested => {
                        let frame = surface
                            .get_current_texture()
                            .expect("Failed to acquire next swap chain texture");
                        let view = frame
                            .texture
                            .create_view(&wgpu::TextureViewDescriptor::default());
                        let mut encoder =
                            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: None,
                            });

                        let mx_total = generate_matrix(config.width as f32 / config.height as f32, &camera);
                        let mx_ref: &[f32; 16] = mx_total.as_ref();
                        queue.write_buffer(&uniform_buf, 0, &bytemuck::cast_slice(mx_ref));

                        {
                            let mut rpass =
                                encoder.begin_render_pass(&RenderPassDescriptor {
                                    label: None,
                                    color_attachments: &[Some(RenderPassColorAttachment {
                                        view: &view,
                                        resolve_target: None,
                                        ops: Operations {
                                            load: LoadOp::Clear(Color {
                                                r: 238.0 / 255.0,
                                                g: 238.0 / 255.0,
                                                b: 238.0 / 255.0,
                                                a: 1.0,
                                            }),
                                            store: StoreOp::Store,
                                        },
                                    })],
                                    depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                                        view: &depth.1,
                                        depth_ops: Some(Operations {
                                            load: LoadOp::Clear(1.0),
                                            store: StoreOp::Store,
                                        }),
                                        stencil_ops: None,
                                    }),
                                    timestamp_writes: None,
                                    occlusion_query_set: None,
                                });

                            rpass.push_debug_group("Prepare data for draw.");
                            rpass.set_pipeline(&render_pipeline);
                            rpass.set_bind_group(0, &bind_group, &[]);
                            rpass.set_index_buffer(index_buf.slice(..), wgpu::IndexFormat::Uint16);
                            rpass.set_vertex_buffer(0, vertex_buf.slice(..));
                            rpass.pop_debug_group();
                            rpass.insert_debug_marker("Draw!");
                            rpass.draw_indexed(0..index_data.len() as u32, 0, 0..1);
                        }

                        queue.submit(Some(encoder.finish()));
                        frame.present();

                        window.request_redraw();
                    }
                    WindowEvent::Focused(_) => {
                        // TODO winit bug? changing cursor grab mode here didn't work
                    }
                    WindowEvent::MouseInput { state, button, .. } => {
                        // TODO account for device_id
                        if !is_locked && state == ElementState::Pressed && (button == MouseButton::Left || button == MouseButton::Right) {
                            info!("Locking cursor");
                            match window.set_cursor_grab(CursorGrabMode::Locked) {
                                Ok(()) => {
                                    is_locked = true;
                                }
                                Err(e) => todo!("Lock cursor manually with set_position for x11 and windows? {e}")
                            }
                        }
                    }
                    WindowEvent::KeyboardInput { event, .. } => {
                        if is_locked && event.logical_key == Key::Named(NamedKey::Escape) {
                            info!("Unlocking cursor");
                            window.set_cursor_grab(CursorGrabMode::None).unwrap();
                            is_locked = false;
                        }
                        let speed = delta_time * 10.0;
                        if let Key::Character(str) = event.logical_key {
                            let vectors = camera.computed_vectors();
                            match str.as_str() {
                                "w" => camera.position += vectors.direction * speed,
                                "a" => camera.position -= vectors.right * speed,
                                "s" => camera.position -= vectors.direction * speed,
                                "d" => camera.position += vectors.right * speed,
                                _ => {}
                            }
                            dbg!(&camera);
                        }
                    }
                    _ => {}
                }
            }
            Event::DeviceEvent { event, .. } => {
                match event {
                    DeviceEvent::MouseMotion { delta } => {
                        if is_locked && window.has_focus() {
                            let speed = delta_time * 0.1;
                            camera.turn_right(delta.0 as f32 * speed);
                            camera.turn_up(-delta.1 as f32 * speed);
                            dbg!(&camera);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }).unwrap();
}
