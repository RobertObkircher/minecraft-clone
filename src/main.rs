extern crate core;

use std::borrow::Cow;
use std::f32::consts::{PI, TAU};
use std::io;
use std::time::{Duration, Instant};

use glam::{IVec3, Mat4, Vec3};
use log::info;
use pollster;
use wgpu::util::{BufferInitDescriptor, DeviceExt};
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
    BindingResource, BindingType, BufferBindingType, BufferSize, BufferUsages, Color,
    ColorTargetState, CommandEncoderDescriptor, CompareFunction, DepthStencilState, Device,
    DeviceDescriptor, Extent3d, Face, Features, FragmentState, IndexFormat, Instance, Limits,
    LoadOp, MultisampleState, Operations, PipelineLayout, PipelineLayoutDescriptor,
    PowerPreference, PresentMode, PrimitiveState, RenderPassColorAttachment,
    RenderPassDepthStencilAttachment, RenderPassDescriptor, RenderPipeline,
    RenderPipelineDescriptor, RequestAdapterOptions, SamplerBindingType, ShaderModuleDescriptor,
    ShaderSource, ShaderStages, StoreOp, SurfaceConfiguration, Texture, TextureDescriptor,
    TextureDimension, TextureFormat, TextureSampleType, TextureUsages, TextureView,
    TextureViewDescriptor, TextureViewDimension, VertexState,
};
use winit::event::{DeviceEvent, ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::EventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorGrabMode, Window, WindowBuilder};

use crate::camera::Camera;
use crate::chunk::{Block, Chunk};
use crate::mesh::ChunkMesh;
use crate::position::{BlockPosition, ChunkPosition};
use crate::statistics::{FrameInfo, Statistics};
use crate::terrain::{TerrainGenerator, WorldSeed};
use crate::texture::BlockTexture;
use crate::world::World;

mod camera;
mod chunk;
mod mesh;
mod noise;
mod position;
#[cfg(feature = "reload")]
mod reload;
mod statistics;
mod terrain;
mod texture;
mod world;

fn main() {
    env_logger::init();
    info!("Hello, world!");

    let event_loop = EventLoop::new().unwrap();

    let window = WindowBuilder::new()
        .with_title("Hello, world!")
        .with_inner_size(winit::dpi::LogicalSize::new(800.0, 600.0))
        .build(&event_loop)
        .unwrap();

    pollster::block_on(run(event_loop, window));
}

fn generate_matrix(aspect_ratio: f32, camera: &Camera) -> Mat4 {
    let fov_y_radians = PI / 4.0;
    let projection = Mat4::perspective_rh(fov_y_radians, aspect_ratio, 0.1, 1000.0);

    let vs = camera.computed_vectors();
    let view = Mat4::look_to_rh(camera.position, vs.direction, vs.up);

    projection * view
}

const DEPTH_TEXTURE_FORMAT: TextureFormat = TextureFormat::Depth32Float;

pub fn create_depth_texture(
    device: &Device,
    config: &SurfaceConfiguration,
) -> (Texture, TextureView) {
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
    let mut statistics = Statistics::new();

    let instance = Instance::default();
    let surface = unsafe { instance.create_surface(&window) }.unwrap();

    let adapter = instance
        .request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        })
        .await
        .expect("Failed to find an appropriate adapter");

    let (device, queue) = adapter
        .request_device(
            &DeviceDescriptor {
                label: None,
                features: Features::empty(),
                // Make sure we use the texture resolution limits from the adapter, so we can support images the size of the swapchain.
                limits: Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            },
            None,
        )
        .await
        .expect("Failed to create device");

    let swapchain_capabilities = surface.get_capabilities(&adapter);
    let swapchain_format = swapchain_capabilities
        .formats
        .iter()
        .copied()
        .find(|it| it.is_srgb())
        .expect("Expected srgb surface");

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
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: BufferSize::new(12),
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 2,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 3,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::NonFiltering), // TODO filtering?
                count: None,
            },
        ],
    });
    let chunk_bind_group_layout =
        device.create_bind_group_layout(&ChunkMesh::BIND_GROUP_LAYOUT_DESCRIPTOR);

    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&bind_group_layout, &chunk_bind_group_layout],
        push_constant_ranges: &[],
    });

    let mut camera = Camera::new(Vec3::new(6.0, 6.0, 6.0));
    camera.turn_right(-TAU / 3.0);
    camera.turn_up(-PI / 2.0 / 3.0);

    let projection_view_matrix_uniform_buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: Some("Uniform Buffer"),
        contents: bytemuck::cast_slice(
            generate_matrix(config.width as f32 / config.height as f32, &camera).as_ref(),
        ),
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
    });
    let mut player_chunk = ChunkPosition::from_chunk_index(IVec3::ZERO);
    let player_chunk_uniform_buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: Some("Uniform Buffer"),
        contents: bytemuck::cast_slice(player_chunk.block().index().as_ref()),
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
    });

    let blocks = BlockTexture::from_bitmap_bytes(
        &device,
        &queue,
        include_bytes!("blocks.bmp"),
        "blocks.bmp",
    );

    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        layout: &bind_group_layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: projection_view_matrix_uniform_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: player_chunk_uniform_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::TextureView(&blocks.view),
            },
            BindGroupEntry {
                binding: 3,
                resource: BindingResource::Sampler(&blocks.sampler),
            },
        ],
        label: None,
    });

    let mut depth = create_depth_texture(&device, &config);

    #[cfg(not(feature = "reload"))]
    let render_pipeline = create_chunk_shader_and_render_pipeline(
        &device,
        &pipeline_layout,
        swapchain_format.into(),
        include_str!("shader.wgsl"),
    );

    #[cfg(feature = "reload")]
    let mut render_pipeline_reloader = reload::Reloader::new(file!(), "shader.wgsl");
    #[cfg(feature = "reload")]
    let mut render_pipeline = None;

    let delta_time = Duration::from_millis(16).as_secs_f32();

    let mut world = World::new(12);
    let mut terrain = TerrainGenerator::new(WorldSeed(42));

    let mut start = Instant::now();

    let mut is_locked = false;
    let mut print_statistics = false;
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
                        window.request_redraw();

                        #[cfg(feature = "reload")]
                        if let Some(changed) = render_pipeline_reloader.get_changed_content() {
                            render_pipeline = match reload::validate_shader(changed, &device.features(), &device.limits(), "shader.wgsl", &["vs_main", "fs_main"]) {
                                Ok(source) => Some(create_chunk_shader_and_render_pipeline(&device, &pipeline_layout, swapchain_format.into(), source)),
                                Err(e) => {
                                    log::error!("Error while re-loading shader: {e}");
                                    None
                                }
                            }
                        }
                        #[cfg(feature = "reload")]
                        let render_pipeline = if let Some(pipeline) = &render_pipeline {
                            pipeline
                        } else {
                            return;
                        };

                        world.generate_chunks(&mut terrain, &mut statistics, player_chunk);
                        world.update_meshes(&device, &queue, &chunk_bind_group_layout, &mut statistics);

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

                        let chunk_offset = BlockPosition::new(camera.position.floor().as_ivec3()).chunk().index();
                        if chunk_offset != IVec3::ZERO {
                            player_chunk = player_chunk.plus(chunk_offset);
                            camera.position -= (chunk_offset * Chunk::SIZE as i32).as_vec3();
                            queue.write_buffer(&player_chunk_uniform_buffer, 0, &bytemuck::cast_slice(player_chunk.block().index().as_ref()));
                        }
                        // must happen after the player chunk uniform update to avoid one invalid frame
                        let projection_view_matrix = generate_matrix(config.width as f32 / config.height as f32, &camera);
                        queue.write_buffer(&projection_view_matrix_uniform_buffer, 0, &bytemuck::cast_slice(projection_view_matrix.as_ref()));

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
                                pass.set_bind_group(1, &mesh.bind_group, &[]);
                                pass.pop_debug_group();
                                pass.insert_debug_marker(&format!("Drawing chunk {position:?}"));
                                pass.draw_indexed(0..mesh.index_count as u32, 0, 0..1);
                            }
                        }

                        queue.submit(Some(encoder.finish()));
                        frame.present();

                        let frame_time = start.elapsed();
                        start += frame_time;

                        statistics.end_frame(FrameInfo {
                            player_position: player_chunk.block().index().as_vec3() + camera.position,
                            player_orientation: camera.computed_vectors().direction,
                            frame_time,
                            chunk_info_count: statistics.chunk_infos.len(),
                            chunk_mesh_info_count: statistics.chunk_mesh_infos.len(),
                        });

                        if print_statistics {
                            statistics.print_last_frame(&mut io::stdout().lock()).unwrap();
                        }
                    }
                    WindowEvent::Focused(_) => {
                        // TODO winit bug? changing cursor grab mode here didn't work
                    }
                    WindowEvent::MouseInput { state, button, .. } => {
                        let max_distance = 20;
                        if is_locked && state == ElementState::Pressed && button == MouseButton::Left {
                            let vs = camera.computed_vectors();
                            if let (_, Some(position)) = world.find_nearest_block_on_ray(player_chunk, camera.position, vs.direction, max_distance) {
                                world.set_block(position, Block::Air);
                            }

                        }
                        if is_locked && state == ElementState::Pressed && button == MouseButton::Right {
                            let vs = camera.computed_vectors();
                            if let (Some(position), _) = world.find_nearest_block_on_ray(player_chunk, camera.position, vs.direction, max_distance) {
                                world.set_block(position, Block::Dirt);
                            }
                        }

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
                                "p" => print_statistics = event.state.is_pressed(),
                                _ => {}
                            }
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
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }).unwrap();
}

fn create_chunk_shader_and_render_pipeline(
    device: &Device,
    pipeline_layout: &PipelineLayout,
    target: ColorTargetState,
    source: &str,
) -> RenderPipeline {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("chunks shader"),
        source: ShaderSource::Wgsl(Cow::Borrowed(source)),
    });

    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: None,
        layout: Some(pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[ChunkMesh::VERTEX_BUFFER_LAYOUT],
        },
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(target)],
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
    })
}
