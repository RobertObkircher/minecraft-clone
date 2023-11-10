use std::borrow::Cow;
use std::f32::consts::{PI, TAU};
use std::mem;
use std::time::Duration;

use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Mat4, Vec3};
use log::info;
use pollster;
use wgpu::{BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, BufferAddress, BufferBindingType, BufferSize, BufferUsages, Color, CommandEncoderDescriptor, CompareFunction, DepthStencilState, Device, DeviceDescriptor, Extent3d, Face, Features, FragmentState, IndexFormat, Instance, Limits, LoadOp, MultisampleState, Operations, PipelineLayoutDescriptor, PowerPreference, PresentMode, PrimitiveState, RenderPassColorAttachment, RenderPassDepthStencilAttachment, RenderPassDescriptor, RenderPipelineDescriptor, RequestAdapterOptions, ShaderModuleDescriptor, ShaderSource, ShaderStages, StoreOp, SurfaceConfiguration, Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureView, TextureViewDescriptor, VertexAttribute, VertexBufferLayout, VertexFormat, VertexState, VertexStepMode};
use wgpu::util::{BufferInitDescriptor, DeviceExt};
use winit::event::{DeviceEvent, ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::EventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorGrabMode, Window, WindowBuilder};

use crate::camera::Camera;
use crate::chunk::Transparency;
use crate::mesh::ChunkMesh;
use crate::terrain::{TerrainGenerator, WorldSeed};
use crate::world::{ChunkPosition, World};

mod camera;
mod world;
mod chunk;
mod mesh;
mod terrain;
mod noise;

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

fn generate_matrix(aspect_ratio: f32, camera: &Camera) -> Mat4 {
    let fov_y_radians = PI / 4.0;
    let projection = Mat4::perspective_rh(fov_y_radians, aspect_ratio, 0.1, 1000.0);

    let vs = camera.computed_vectors();
    let view = Mat4::look_to_rh(camera.position, vs.direction, vs.up);

    projection * view
}

const DEPTH_TEXTURE_FORMAT: TextureFormat = TextureFormat::Depth32Float;

pub fn create_depth_texture(device: &Device, config: &SurfaceConfiguration) -> (Texture, TextureView) {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("depth"),
        size: Extent3d {
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
    let instance = Instance::default();
    let surface = unsafe { instance.create_surface(&window) }.unwrap();

    let adapter = instance.request_adapter(&RequestAdapterOptions {
        power_preference: PowerPreference::default(),
        force_fallback_adapter: false,
        compatible_surface: Some(&surface),
    }).await.expect("Failed to find an appropriate adapter");

    let (device, queue) = adapter.request_device(&DeviceDescriptor {
        label: None,
        features: Features::empty(),
        // Make sure we use the texture resolution limits from the adapter, so we can support images the size of the swapchain.
        limits: Limits::downlevel_webgl2_defaults()
            .using_resolution(adapter.limits()),
    }, None).await.expect("Failed to create device");

    let swapchain_capabilities = surface.get_capabilities(&adapter);
    let swapchain_format = swapchain_capabilities.formats[0];
    let size = window.inner_size();
    let mut config = SurfaceConfiguration {
        usage: TextureUsages::RENDER_ATTACHMENT,
        format: swapchain_format,
        width: size.width,
        height: size.height,
        present_mode: PresentMode::Fifo,
        alpha_mode: swapchain_capabilities.alpha_modes[0],
        view_formats: vec![],
    };

    surface.configure(&device, &config);

    let vertex_size = mem::size_of::<Vertex>();

    let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: BufferSize::new(64),
                },
                count: None,
            }
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let mut camera = Camera::new(Vec3::new(6.0, 6.0, 6.0));
    camera.turn_right(-TAU / 3.0);
    camera.turn_up(-PI / 2.0 / 3.0);

    let mx_total = generate_matrix(config.width as f32 / config.height as f32, &camera);
    let mx_ref: &[f32; 16] = mx_total.as_ref();
    let uniform_buf = device.create_buffer_init(&BufferInitDescriptor {
        label: Some("Uniform Buffer"),
        contents: bytemuck::cast_slice(mx_ref),
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
    });

    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        layout: &bind_group_layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }
        ],
        label: None,
    });


    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: None,
        source: ShaderSource::Wgsl(Cow::Borrowed(include_str!("shader.wgsl"))),
    });

    let vertex_buffers = [VertexBufferLayout {
        array_stride: vertex_size as BufferAddress,
        step_mode: VertexStepMode::Vertex,
        attributes: &[
            VertexAttribute {
                format: VertexFormat::Float32x4,
                offset: 0,
                shader_location: 0,
            },
            VertexAttribute {
                format: VertexFormat::Float32x2,
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

    let mut world = World::new();
    let mut terrain = TerrainGenerator::new(WorldSeed(42));

    let view_distance = 10;
    for x in -view_distance..=view_distance {
        'next_z: for z in -view_distance..=view_distance {
            for y in -view_distance / 2..=view_distance / 2 {
                let y = -y;
                let position = ChunkPosition::from_chunk_index(IVec3::new(x, y, z));

                if let Some(above) = world.get_chunk(position.plus(IVec3::Y)) {
                    if above.get_transparency(Transparency::Computed) && !above.get_transparency(Transparency::NegY) {
                        continue 'next_z;
                    }
                }

                if let Some(chunk) = terrain.fill_chunk(position) {
                    world.add_mesh(position, ChunkMesh::new(&device, position, &chunk));
                    world.add_chunk(position, chunk);
                } else {
                    world.add_air_chunk(position);
                }
            }
        }
    }

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
                            .create_view(&TextureViewDescriptor::default());
                        let mut encoder =
                            device.create_command_encoder(&CommandEncoderDescriptor {
                                label: None,
                            });

                        let mx_total = generate_matrix(config.width as f32 / config.height as f32, &camera);
                        let mx_ref: &[f32; 16] = mx_total.as_ref();
                        queue.write_buffer(&uniform_buf, 0, &bytemuck::cast_slice(mx_ref));

                        {
                            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                                label: Some("render world"),
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

                            pass.push_debug_group("chunks setup");
                            pass.set_pipeline(&render_pipeline);
                            pass.set_bind_group(0, &bind_group, &[]);
                            pass.pop_debug_group();
                            pass.insert_debug_marker("before chunks");

                            for (position, mesh) in world.iter_chunk_meshes() {
                                pass.push_debug_group(&format!("Blocks of chunk {position:?}"));
                                pass.set_index_buffer(mesh.index_buffer.slice(..), IndexFormat::Uint16);
                                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                                pass.pop_debug_group();
                                pass.insert_debug_marker(&format!("Drawing chunk {position:?}"));
                                pass.draw_indexed(0..mesh.index_count as u32, 0, 0..1);
                            }
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
