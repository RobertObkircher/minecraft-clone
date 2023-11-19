extern crate core;

use bytemuck::{Contiguous, Pod, Zeroable};
use std::borrow::Cow;
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};
use std::mem::size_of;
use std::time::Duration;

use glam::{IVec3, Mat4, Vec3};
use log::info;
use wgpu::util::{BufferInitDescriptor, DeviceExt};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferBindingType, BufferSize,
    BufferUsages, Color, ColorTargetState, CommandEncoderDescriptor, CompareFunction,
    DepthStencilState, Device, DeviceDescriptor, Extent3d, Face, Features, FragmentState,
    IndexFormat, Instance, Limits, LoadOp, MultisampleState, Operations, PipelineLayout,
    PipelineLayoutDescriptor, PowerPreference, PresentMode, PrimitiveState, Queue,
    RenderPassColorAttachment, RenderPassDepthStencilAttachment, RenderPassDescriptor,
    RenderPipeline, RenderPipelineDescriptor, RequestAdapterOptions, SamplerBindingType,
    ShaderModuleDescriptor, ShaderSource, ShaderStages, StoreOp, Surface, SurfaceConfiguration,
    Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages,
    TextureView, TextureViewDescriptor, TextureViewDimension, VertexState,
};
use winit::dpi::PhysicalPosition;
use winit::event::{
    DeviceEvent, DeviceId, ElementState, Event, MouseButton, Touch, TouchPhase, WindowEvent,
};
use winit::event_loop::EventLoopWindowTarget;
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorGrabMode, Window};

use crate::camera::Camera;
use crate::chunk::{Block, Chunk};
use crate::mesh::ChunkMesh;
use crate::position::{BlockPosition, ChunkPosition};
use crate::statistics::{ChunkInfo, FrameInfo, Statistics};
use crate::terrain::{TerrainGenerator, WorldSeed};
use crate::texture::BlockTexture;
use crate::timer::Timer;
use crate::worker::{MessageTag, Worker, WorkerId, WorkerMessage};
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
mod timer;
#[cfg(target_arch = "wasm32")]
mod wasm;
pub mod worker;
mod world;

fn generate_matrix(aspect_ratio: f32, camera: &Camera) -> Mat4 {
    let fov_y_radians = PI / 4.0;
    let projection = Mat4::perspective_rh(fov_y_radians, aspect_ratio, 0.1, 1000.0);

    let vs = camera.computed_vectors();
    let view = Mat4::look_to_rh(camera.position, vs.direction, vs.up);

    projection * view
}

struct SimulationState {
    seed: WorldSeed,
    world: World,
    workers: Vec<WorkerId>,
    worker_task_count: usize,
    next_worker_index: usize,
}

impl SimulationState {
    pub fn initialize<W: Worker>(
        worker: &mut W,
        message: WorkerMessage,
    ) -> (Self, Option<Duration>) {
        let seed = *bytemuck::from_bytes(&message.bytes[0..8]);

        let world = World::new(12, 16);

        let workers = (0..W::available_parallelism().get())
            .map(|_| worker.spawn_child())
            .collect::<Vec<_>>();

        workers.iter().for_each(|&w| {
            worker.send_message(w, {
                let mut message = [0u8; 17];
                message[0..8].copy_from_slice(bytemuck::bytes_of(&seed));
                message[8..12].copy_from_slice(bytemuck::bytes_of(&world.highest_generated_chunk));
                message[12..16].copy_from_slice(bytemuck::bytes_of(&world.lowest_generated_chunk));
                *message.last_mut().unwrap() = MessageTag::InitGenerator as u8;
                Box::new(message)
            })
        });

        let mut state = SimulationState {
            seed,
            world,
            workers,
            worker_task_count: 0,
            next_worker_index: 0,
        };

        state.send_commands_to_workers(worker);

        (state, None)
    }

    fn send_commands_to_workers(&mut self, worker: &mut impl Worker) {
        while let Some((x, z)) = self.world.next_column_to_generate() {
            let mut message = [0u8; 9];
            message[0..4].copy_from_slice(&x.to_ne_bytes());
            message[4..8].copy_from_slice(&z.to_ne_bytes());
            *message.last_mut().unwrap() = MessageTag::GenerateColumn as u8;

            worker.send_message(self.workers[self.next_worker_index], Box::new(message));

            self.worker_task_count += 1;

            self.next_worker_index += 1;
            if self.next_worker_index >= self.workers.len() {
                self.next_worker_index = 0;
            }
        }
    }

    pub fn update(
        &mut self,
        _worker: &mut impl Worker,
        message: Option<WorkerMessage>,
    ) -> Option<Duration> {
        let tag = message.as_ref().map(WorkerMessage::tag);

        if tag == Some(MessageTag::GenerateColumnReply) {
            let message = message.unwrap();
            let x = *bytemuck::from_bytes::<i32>(&message.bytes[0..4]);
            let z = *bytemuck::from_bytes::<i32>(&message.bytes[4..8]);

            let mut remainder = &message.bytes[8..];

            for y in self.world.lowest_generated_chunk..=self.world.highest_generated_chunk {
                let is_some = remainder[0];
                let position = ChunkPosition::from_chunk_index(IVec3::new(x, y, z));

                if is_some == 1 {
                    let element = bytemuck::from_bytes::<ChunkColumnElement>(
                        &remainder[0..size_of::<ChunkColumnElement>()],
                    );
                    let chunk = Chunk {
                        blocks: element
                            .blocks
                            .map(|it| it.map(|it| it.map(|it| Block::from_integer(it).unwrap()))),
                        transparency: element.transparency,
                        in_mesh_queue: false,
                        non_air_block_count: element.non_air_block_count,
                    };
                    self.world.add_chunk(position, chunk);
                    remainder = &remainder[size_of::<ChunkColumnElement>()..];
                } else {
                    self.world.add_air_chunk(position);
                    remainder = &remainder[2..];
                }
            }
        }

        None
    }
}

struct GeneratorState {
    generator: TerrainGenerator,
    highest_generated_chunk: i32,
    lowest_generated_chunk: i32,
}

#[repr(C)]
#[derive(Zeroable, Pod, Copy, Clone)]
struct ChunkColumnElement {
    is_some: u8,
    transparency: u8,
    non_air_block_count: u16,
    blocks: [[[u8; Chunk::SIZE]; Chunk::SIZE]; Chunk::SIZE],
}

impl GeneratorState {
    pub fn initialize<W: Worker>(_worker: &mut W, message: WorkerMessage) -> Self {
        let seed = *bytemuck::from_bytes(&message.bytes[0..8]);
        let highest_generated_chunk = *bytemuck::from_bytes(&message.bytes[8..12]);
        let lowest_generated_chunk = *bytemuck::from_bytes(&message.bytes[12..16]);

        GeneratorState {
            generator: TerrainGenerator::new(seed),
            highest_generated_chunk,
            lowest_generated_chunk,
        }
    }
    pub fn update(
        &mut self,
        worker: &mut impl Worker,
        message: Option<WorkerMessage>,
    ) -> Option<Duration> {
        let tag = message.as_ref().map(WorkerMessage::tag);

        if tag == Some(MessageTag::GenerateColumn) {
            let message = message.unwrap();
            let x = *bytemuck::from_bytes::<i32>(&message.bytes[0..4]);
            let z = *bytemuck::from_bytes::<i32>(&message.bytes[4..8]);

            let count = (self.highest_generated_chunk - self.lowest_generated_chunk) as usize + 1;

            let mut message =
                Vec::<u8>::with_capacity(8 + count * size_of::<ChunkColumnElement>() + 1);

            message.extend_from_slice(bytemuck::bytes_of(&x));
            message.extend_from_slice(bytemuck::bytes_of(&z));

            for y in self.lowest_generated_chunk..=self.highest_generated_chunk {
                let (chunk, info) = self
                    .generator
                    .fill_chunk(ChunkPosition::from_chunk_index(IVec3::new(x, y, z)));

                // TODO send chunk infos to render thread

                if let Some(chunk) = chunk {
                    message.extend_from_slice(bytemuck::bytes_of(&ChunkColumnElement {
                        is_some: 1,
                        transparency: chunk.transparency,
                        non_air_block_count: chunk.non_air_block_count,
                        blocks: chunk.blocks.map(|it| it.map(|it| it.map(|it| it as u8))),
                    }));
                } else {
                    message.push(0); // false
                    message.push(0); // alignment
                }
            }

            message.push(MessageTag::GenerateColumnReply as u8);
            let message = message.into_boxed_slice();

            worker.send_message(WorkerId::Parent, message);
        }

        None
    }
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

/// wgpu wants this to be non-zero and chromium 4x4
const MIN_SURFACE_SIZE: u32 = 4;

pub struct RendererState {
    config: SurfaceConfiguration,
    surface: Surface,
    depth: (Texture, TextureView),
    device: Device,
    queue: Queue,
    #[cfg(not(feature = "reload"))]
    render_pipeline: RenderPipeline,
    #[cfg(feature = "reload")]
    render_pipeline_reloader: reload::Reloader,
    #[cfg(feature = "reload")]
    render_pipeline: Option<RenderPipeline>,
    pipeline_layout: PipelineLayout,
    swapchain_format: TextureFormat,
    bind_group: BindGroup,
    chunk_bind_group_layout: BindGroupLayout,
    statistics: Statistics,
    world: World,
    terrain: TerrainGenerator,
    camera: Camera,
    projection_view_matrix_uniform_buffer: Buffer,
    player_chunk: ChunkPosition,
    player_chunk_uniform_buffer: Buffer,
    start: Timer,
    delta_time: f32,
    fingers: HashMap<(DeviceId, u64), PhysicalPosition<f64>>,
    is_locked: bool,
    print_statistics: bool,

    /// WARNING: order matters. This must be dropped last!
    window: Window,
}

impl RendererState {
    pub fn update(&mut self, _worker: &mut impl Worker, _message: Option<WorkerMessage>) {}
}

impl RendererState {
    pub async fn new<W: Worker>(window: Window, worker: &mut W) -> Self {
        let simulation = worker.spawn_child();
        let seed = WorldSeed(42);
        {
            let mut message = [0u8; 9];
            message[0..8].copy_from_slice(bytemuck::bytes_of(&seed));
            *message.last_mut().unwrap() = MessageTag::InitSimulation as u8;
            worker.send_message(simulation, Box::new(message));
        }

        let statistics = Statistics::new();

        let instance = Instance::default();
        let surface = unsafe {
            // SAFETY:
            // * window is a valid object to create a surface upon.
            // * window remains valid until after the Surface is dropped
            //   because it is stored as the last field in RendererState,
            //   so it is dropped last.
            instance.create_surface(&window)
        }
        .unwrap();

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
        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: swapchain_format,
            width: size.width.max(MIN_SURFACE_SIZE),
            height: size.height.max(MIN_SURFACE_SIZE),
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
                        min_binding_size: BufferSize::new(16), // Actually 12, but that isn't supported by webgl
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

        let projection_view_matrix_uniform_buffer =
            device.create_buffer_init(&BufferInitDescriptor {
                label: Some("Uniform Buffer"),
                contents: bytemuck::cast_slice(
                    generate_matrix(config.width as f32 / config.height as f32, &camera).as_ref(),
                ),
                usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            });
        let player_chunk = ChunkPosition::from_chunk_index(IVec3::ZERO);
        let player_chunk_uniform_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Uniform Buffer"),
            contents: bytemuck::cast_slice(player_chunk.block().index().extend(0).as_ref()),
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

        let depth = create_depth_texture(&device, &config);

        #[cfg(not(feature = "reload"))]
        let render_pipeline = create_chunk_shader_and_render_pipeline(
            &device,
            &pipeline_layout,
            swapchain_format.into(),
            include_str!("shader.wgsl"),
        );

        #[cfg(feature = "reload")]
        let render_pipeline_reloader = reload::Reloader::new(file!(), "shader.wgsl");
        #[cfg(feature = "reload")]
        let render_pipeline = None;

        let world = World::new(12, 16);
        let terrain = TerrainGenerator::new(seed);

        let start = Timer::now();
        // TODO compute delta_time
        let delta_time = Duration::from_millis(16).as_secs_f32();

        Self {
            config,
            surface,
            depth,
            device,
            queue,
            render_pipeline,
            #[cfg(feature = "reload")]
            render_pipeline_reloader,
            pipeline_layout,
            swapchain_format,
            bind_group,
            chunk_bind_group_layout,
            statistics,
            world,
            terrain,
            camera,
            projection_view_matrix_uniform_buffer,
            player_chunk,
            player_chunk_uniform_buffer,
            start,
            delta_time,
            fingers: HashMap::new(),
            is_locked: false,
            print_statistics: true,
            window,
        }
    }

    pub fn process_event(&mut self, event: Event<()>, target: &EventLoopWindowTarget<()>) {
        let id = self.window.id();

        match event {
            Event::WindowEvent { event, window_id } if window_id == id => {
                match event {
                    WindowEvent::Resized(new_size) => {
                        self.config.width = new_size.width.max(MIN_SURFACE_SIZE);
                        self.config.height = new_size.height.max(MIN_SURFACE_SIZE);
                        self.surface.configure(&self.device, &self.config);
                        self.depth = create_depth_texture(&self.device, &self.config);

                        // necessary on macos, according to hello triangle example
                        self.window.request_redraw();
                    }
                    WindowEvent::CloseRequested => {
                        target.exit();
                    }
                    WindowEvent::RedrawRequested => {
                        self.window.request_redraw();

                        #[cfg(target_arch = "wasm32")]
                        if self.is_locked
                            && web_sys::window()
                                .unwrap()
                                .document()
                                .unwrap()
                                .pointer_lock_element()
                                .is_none()
                        {
                            // without this we would have to hit esc twice
                            info!("Lost pointer grab");
                            self.window.set_cursor_grab(CursorGrabMode::None).unwrap();
                            self.is_locked = false;
                        }

                        #[cfg(feature = "reload")]
                        if let Some(changed) = self.render_pipeline_reloader.get_changed_content() {
                            self.render_pipeline = match reload::validate_shader(
                                changed,
                                &self.device.features(),
                                &self.device.limits(),
                                "shader.wgsl",
                                &["vs_main", "fs_main"],
                            ) {
                                Ok(source) => Some(create_chunk_shader_and_render_pipeline(
                                    &self.device,
                                    &self.pipeline_layout,
                                    self.swapchain_format.into(),
                                    source,
                                )),
                                Err(e) => {
                                    log::error!("Error while re-loading shader: {e}");
                                    None
                                }
                            }
                        }
                        #[cfg(feature = "reload")]
                        let render_pipeline = if let Some(pipeline) = &self.render_pipeline {
                            pipeline
                        } else {
                            return;
                        };
                        #[cfg(not(feature = "reload"))]
                        let render_pipeline = &self.render_pipeline;

                        self.world.generate_chunks(
                            &mut self.terrain,
                            &mut self.statistics,
                            self.player_chunk,
                        );
                        self.world.update_meshes(
                            &self.device,
                            &self.queue,
                            &self.chunk_bind_group_layout,
                            &mut self.statistics,
                        );

                        let frame = self
                            .surface
                            .get_current_texture()
                            .expect("Failed to acquire next swap chain texture");
                        let view = frame.texture.create_view(&TextureViewDescriptor::default());
                        let mut encoder = self
                            .device
                            .create_command_encoder(&CommandEncoderDescriptor { label: None });

                        let chunk_offset =
                            BlockPosition::new(self.camera.position.floor().as_ivec3())
                                .chunk()
                                .index();
                        if chunk_offset != IVec3::ZERO {
                            self.player_chunk = self.player_chunk.plus(chunk_offset);
                            self.camera.position -= (chunk_offset * Chunk::SIZE as i32).as_vec3();
                            self.queue.write_buffer(
                                &self.player_chunk_uniform_buffer,
                                0,
                                &bytemuck::cast_slice(
                                    self.player_chunk.block().index().extend(0).as_ref(),
                                ),
                            );
                        }
                        // must happen after the player chunk uniform update to avoid one invalid frame
                        let projection_view_matrix = generate_matrix(
                            self.config.width as f32 / self.config.height as f32,
                            &self.camera,
                        );
                        self.queue.write_buffer(
                            &self.projection_view_matrix_uniform_buffer,
                            0,
                            &bytemuck::cast_slice(projection_view_matrix.as_ref()),
                        );

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
                                    view: &self.depth.1,
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
                            pass.set_pipeline(render_pipeline);
                            pass.set_bind_group(0, &self.bind_group, &[]);
                            pass.pop_debug_group();
                            pass.insert_debug_marker("before chunks");

                            for (position, mesh) in self.world.iter_chunk_meshes() {
                                pass.push_debug_group(&format!("Blocks of chunk {position:?}"));
                                pass.set_index_buffer(
                                    mesh.index_buffer.slice(..),
                                    IndexFormat::Uint16,
                                );
                                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                                pass.set_bind_group(1, &mesh.bind_group, &[]);
                                pass.pop_debug_group();
                                pass.insert_debug_marker(&format!("Drawing chunk {position:?}"));
                                pass.draw_indexed(0..mesh.index_count as u32, 0, 0..1);
                            }
                        }

                        self.queue.submit(Some(encoder.finish()));
                        frame.present();

                        let frame_time = self.start.elapsed();
                        self.start += frame_time;

                        self.statistics.end_frame(FrameInfo {
                            player_position: self.player_chunk.block().index().as_vec3()
                                + self.camera.position,
                            player_orientation: self.camera.computed_vectors().direction,
                            frame_time,
                            chunk_info_count: self.statistics.chunk_infos.len(),
                            chunk_mesh_info_count: self.statistics.chunk_mesh_infos.len(),
                        });

                        if self.print_statistics {
                            #[cfg(target_arch = "wasm32")]
                            wasm::display_statistics(&self.statistics);
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                println!();
                                self.statistics
                                    .print_last_frame(&mut std::io::stdout().lock())
                                    .unwrap();
                            }
                        }
                    }
                    WindowEvent::Focused(_) => {
                        // TODO winit bug? changing cursor grab mode here didn't work. the cursor gets stuck when reentering after alt-tab
                    }
                    WindowEvent::MouseInput { state, button, .. } => {
                        if self.is_locked
                            && state == ElementState::Pressed
                            && button == MouseButton::Left
                        {
                            let vs = self.camera.computed_vectors();
                            if let (_, Some(position)) = self.world.find_nearest_block_on_ray(
                                self.player_chunk,
                                self.camera.position,
                                vs.direction,
                                200,
                            ) {
                                info!("set_block {position:?}");
                                self.world.set_block(position, Block::Air);
                            }
                        }
                        if self.is_locked
                            && state == ElementState::Pressed
                            && button == MouseButton::Right
                        {
                            let vs = self.camera.computed_vectors();
                            if let (Some(position), _) = self.world.find_nearest_block_on_ray(
                                self.player_chunk,
                                self.camera.position,
                                vs.direction,
                                200,
                            ) {
                                info!("set_block {position:?}");
                                self.world.set_block(position, Block::Dirt);
                            }
                        }

                        // TODO account for device_id
                        if !self.is_locked
                            && state == ElementState::Pressed
                            && (button == MouseButton::Left || button == MouseButton::Right)
                        {
                            info!("Locking cursor");
                            match self.window.set_cursor_grab(CursorGrabMode::Locked) {
                                Ok(()) => {
                                    self.is_locked = true;
                                }
                                Err(e) => todo!("Lock cursor manually with set_position for x11 and windows? {e}")
                            }
                        }
                    }
                    WindowEvent::Touch(Touch {
                        device_id,
                        phase,
                        location,
                        force: _,
                        id,
                    }) => {
                        let id = (device_id, id);
                        match phase {
                            TouchPhase::Started => {
                                self.fingers.insert(id, location);
                                if self.fingers.len() == 2 {
                                    let vs = self.camera.computed_vectors();
                                    if let (_, Some(position)) =
                                        self.world.find_nearest_block_on_ray(
                                            self.player_chunk,
                                            self.camera.position,
                                            vs.direction,
                                            200,
                                        )
                                    {
                                        info!("explode {position:?}");
                                        let r = 10;
                                        for x in 0..2 * r {
                                            for y in 0..2 * r {
                                                for z in 0..2 * r {
                                                    let delta = IVec3::new(x, y, z) - r;
                                                    if delta.length_squared() <= r * r {
                                                        self.world.set_block(
                                                            position.plus(delta),
                                                            Block::Air,
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            TouchPhase::Moved => {
                                let old = self.fingers.insert(id, location).unwrap();
                                let dx = location.x - old.x;
                                let dy = location.y - old.y;

                                let speed = self.delta_time * 0.1;
                                self.camera.turn_right(-dx as f32 * speed);
                                self.camera.turn_up(dy as f32 * speed);
                            }
                            TouchPhase::Ended | TouchPhase::Cancelled => {
                                self.fingers.remove(&id).unwrap();
                            }
                        }
                    }
                    WindowEvent::KeyboardInput { event, .. } => {
                        if self.is_locked && event.logical_key == Key::Named(NamedKey::Escape) {
                            info!("Unlocking cursor");
                            self.window.set_cursor_grab(CursorGrabMode::None).unwrap();
                            self.is_locked = false;
                        }
                        let speed = self.delta_time * 100.0;
                        if let Key::Character(str) = event.logical_key {
                            let vectors = self.camera.computed_vectors();
                            match str.as_str() {
                                "w" => self.camera.position += vectors.direction * speed,
                                "a" => self.camera.position -= vectors.right * speed,
                                "s" => self.camera.position -= vectors.direction * speed,
                                "d" => self.camera.position += vectors.right * speed,
                                "p" => {
                                    if event.state.is_pressed() {
                                        self.print_statistics ^= true;
                                        #[cfg(target_arch = "wasm32")]
                                        if !self.print_statistics {
                                            wasm::hide_statistics();
                                        }
                                    }
                                }
                                "q" => {
                                    if event.state.is_pressed() {
                                        let vs = self.camera.computed_vectors();
                                        if let (_, Some(position)) =
                                            self.world.find_nearest_block_on_ray(
                                                self.player_chunk,
                                                self.camera.position,
                                                vs.direction,
                                                200,
                                            )
                                        {
                                            info!("explode {position:?}");
                                            let r = 10;
                                            for x in 0..2 * r {
                                                for y in 0..2 * r {
                                                    for z in 0..2 * r {
                                                        let delta = IVec3::new(x, y, z) - r;
                                                        if delta.length_squared() <= r * r {
                                                            self.world.set_block(
                                                                position.plus(delta),
                                                                Block::Air,
                                                            );
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                "e" => {
                                    if event.state.is_pressed() {
                                        let vs = self.camera.computed_vectors();
                                        if let (Some(position), _) =
                                            self.world.find_nearest_block_on_ray(
                                                self.player_chunk,
                                                self.camera.position,
                                                vs.direction,
                                                200,
                                            )
                                        {
                                            info!("anti-explode {position:?}");
                                            let r = 10;
                                            for x in 0..2 * r {
                                                for y in 0..2 * r {
                                                    for z in 0..2 * r {
                                                        let delta = IVec3::new(x, y, z) - r;
                                                        if delta.length_squared() <= r * r {
                                                            self.world.set_block(
                                                                position.plus(delta),
                                                                Block::Dirt,
                                                            );
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::DeviceEvent { event, .. } => match event {
                DeviceEvent::MouseMotion { delta } => {
                    if self.is_locked && self.window.has_focus() {
                        let speed = self.delta_time * 0.1;
                        self.camera.turn_right(delta.0 as f32 * speed);
                        self.camera.turn_up(-delta.1 as f32 * speed);
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
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
