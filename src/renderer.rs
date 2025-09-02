use std::borrow::Cow;
use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::time::Duration;

use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Vec3};
use log::info;
use wgpu::util::{BufferInitDescriptor, DeviceExt};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferBindingType, BufferSize,
    BufferUsages, Color, ColorTargetState, CommandEncoderDescriptor, CompareFunction,
    DepthStencilState, Device, DeviceDescriptor, Extent3d, Face, Features, FragmentState,
    IndexFormat, Instance, InstanceDescriptor, Limits, LoadOp, MultisampleState, Operations,
    PipelineLayout, PipelineLayoutDescriptor, PowerPreference, PresentMode, PrimitiveState, Queue,
    RenderPassColorAttachment, RenderPassDepthStencilAttachment, RenderPassDescriptor,
    RenderPipeline, RenderPipelineDescriptor, RequestAdapterOptions, SamplerBindingType,
    ShaderModuleDescriptor, ShaderSource, ShaderStages, StoreOp, Surface, SurfaceConfiguration,
    Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages,
    TextureView, TextureViewDescriptor, TextureViewDimension, VertexState,
};
use winit::event::{DeviceEvent, ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::EventLoopWindowTarget;
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorGrabMode, Window};

use camera::Camera;
use mesh::{ChunkMesh, Vertex};
use texture::BlockTexture;

use crate::generator::terrain::WorldSeed;
use crate::generator::ChunkInfoBytes;
use crate::renderer::gui::Gui;
use crate::renderer::input::Input;
use crate::renderer::mesh::GuiMesh;
use crate::simulation::position::ChunkPosition;
use crate::simulation::MovementCommandReply;
use crate::statistics::{ChunkInfo, FrameInfo, Statistics};
use crate::timer::Timer;
use crate::worker::{MessageTag, Worker, WorkerId, WorkerMessage};

mod camera;
mod gui;
mod input;
pub mod mesh;
#[cfg(feature = "reload")]
mod reload;
mod texture;

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

pub struct RendererState<'window> {
    config: SurfaceConfiguration,
    surface: Surface<'window>,
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
    chunk_bind_group: BindGroup,
    ui_bind_group: BindGroup,
    chunk_bind_group_layout: BindGroupLayout,
    statistics: Statistics,
    meshes: HashMap<ChunkPosition, ChunkMesh>,
    camera: Camera,
    ui_camera: Camera,
    chunk_projection_view_matrix_uniform_buffer: Buffer,
    ui_projection_view_matrix_uniform_buffer: Buffer,
    player_chunk: ChunkPosition,
    player_chunk_uniform_buffer: Buffer,
    start: Timer,
    delta_time: f32,
    input: Input,
    is_locked: bool,
    print_statistics: bool,
    simulation: WorkerId,
    gui: Gui,
    gui_mesh: Option<GuiMesh>,
    window: &'window Window,
}

#[repr(C)]
#[derive(Copy, Clone, Zeroable, Pod)]
pub struct MeshData {
    pub chunk: [i32; 3],
    pub vertex_count: u32,
    pub index_count: u32,
    pub is_full_and_invisible: u32,
}

impl<'window> RendererState<'window> {
    pub async fn new<W: Worker>(
        window: &'window Window,
        worker: &mut W,
        disable_webgpu: bool,
    ) -> RendererState<'window> {
        let simulation = worker.spawn_child();
        let seed = WorldSeed(42);
        {
            let mut message = [0u8; 9];
            message[0..8].copy_from_slice(bytemuck::bytes_of(&seed));
            *message.last_mut().unwrap() = MessageTag::InitSimulation as u8;
            worker.send_message(simulation, Box::new(message));
        }

        let statistics = Statistics::new();

        let instance = Instance::new(&InstanceDescriptor {
            backends: if disable_webgpu {
                // this is a workaround because chromium has navigator.gpu but requestAdapter returns null on linux
                wgpu::Backends::all() & !wgpu::Backends::BROWSER_WEBGPU
            } else {
                wgpu::Backends::all()
            },
            flags: Default::default(),
            memory_budget_thresholds: Default::default(),
            backend_options: Default::default(),
        });
        let surface = instance.create_surface(window).unwrap();

        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .expect("Failed to find an appropriate adapter");

        let (device, queue) = adapter
            .request_device(&DeviceDescriptor {
                label: None,
                required_features: Features::empty(),
                // Make sure we use the texture resolution limits from the adapter, so we can support images the size of the swapchain.
                required_limits: Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                memory_hints: Default::default(),
                trace: Default::default(),
            })
            .await
            .expect("Failed to create device");

        let swapchain_capabilities = surface.get_capabilities(&adapter);
        info!("{swapchain_capabilities:?}");
        let swapchain_format = swapchain_capabilities
            .formats
            .iter()
            .find(|it| it.is_srgb())
            .copied()
            .unwrap_or(swapchain_capabilities.formats[0]); // TODO fix colors in Chrome on Android

        let size = window.inner_size();
        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: swapchain_format,
            width: size.width.max(MIN_SURFACE_SIZE),
            height: size.height.max(MIN_SURFACE_SIZE),
            present_mode: PresentMode::Fifo,
            alpha_mode: swapchain_capabilities.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 3, // smoother but higher latency
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

        let mut camera = Camera::new(Vec3::new(0.0, 0.0, 0.0), Camera::DEFAULT_FOV_Y);
        camera.set_aspect_ratio(config.width, config.height);
        camera.turn_right(-TAU / 3.0);
        camera.turn_up(-PI / 2.0 / 3.0);

        let chunk_projection_view_matrix_uniform_buffer =
            device.create_buffer_init(&BufferInitDescriptor {
                label: Some("Uniform Buffer"),
                contents: bytemuck::cast_slice(camera.projection_view_matrix().as_ref()),
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
            include_bytes!("renderer/blocks.bmp"),
            "blocks.bmp",
        );

        let chunk_bind_group = device.create_bind_group(&BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: chunk_projection_view_matrix_uniform_buffer.as_entire_binding(),
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

        let mut ui_camera = Camera::new(Vec3::ZERO, PI / 20.0);
        ui_camera.set_aspect_ratio(config.width, config.height);
        ui_camera.turn_right(-FRAC_PI_2);
        let ui_projection_view_matrix_uniform_buffer =
            device.create_buffer_init(&BufferInitDescriptor {
                label: Some("Uniform Buffer"),
                contents: bytemuck::cast_slice(ui_camera.projection_view_matrix().as_ref()),
                usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            });
        let ui_bind_group = device.create_bind_group(&BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: ui_projection_view_matrix_uniform_buffer.as_entire_binding(),
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
            include_str!("renderer/shader.wgsl"),
        );

        #[cfg(feature = "reload")]
        let render_pipeline_reloader = reload::Reloader::new(file!(), "renderer/shader.wgsl");
        #[cfg(feature = "reload")]
        let render_pipeline = None;

        let start = Timer::now();
        // TODO compute delta_time
        let delta_time = Duration::from_millis(16).as_secs_f32();

        let gui = Gui::for_camera(&ui_camera);

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
            chunk_bind_group,
            ui_bind_group,
            chunk_bind_group_layout,
            statistics,
            meshes: HashMap::new(),
            camera,
            ui_camera,
            chunk_projection_view_matrix_uniform_buffer,
            ui_projection_view_matrix_uniform_buffer,
            player_chunk,
            player_chunk_uniform_buffer,
            start,
            delta_time,
            input: Input::default(),
            is_locked: false,
            print_statistics: true,
            simulation,
            gui,
            gui_mesh: None,
            window,
        }
    }

    pub fn process_event(
        &mut self,
        event: Event<()>,
        target: &EventLoopWindowTarget<()>,
        worker: &impl Worker,
    ) {
        let id = self.window.id();

        match event {
            Event::WindowEvent { event, window_id } if window_id == id => {
                match event {
                    WindowEvent::Resized(new_size) => {
                        self.config.width = new_size.width.max(MIN_SURFACE_SIZE);
                        self.config.height = new_size.height.max(MIN_SURFACE_SIZE);
                        self.camera
                            .set_aspect_ratio(self.config.width, self.config.height);
                        self.ui_camera
                            .set_aspect_ratio(self.config.width, self.config.height);

                        self.surface.configure(&self.device, &self.config);
                        self.depth = create_depth_texture(&self.device, &self.config);

                        self.gui = Gui::for_camera(&self.ui_camera);

                        self.queue.write_buffer(
                            &self.ui_projection_view_matrix_uniform_buffer,
                            0,
                            &bytemuck::cast_slice(self.ui_camera.projection_view_matrix().as_ref()),
                        );

                        // necessary on macos, according to hello triangle example
                        self.window.request_redraw();
                    }
                    WindowEvent::CloseRequested => {
                        target.exit();
                    }
                    WindowEvent::RedrawRequested => {
                        self.window.request_redraw();

                        self.input.start_of_frame(
                            worker,
                            self.simulation,
                            self.player_chunk,
                            &mut self.camera,
                            &self.gui,
                            self.delta_time,
                        );

                        self.gui
                            .update_touch_element_visibility(self.input.seconds_without_touch());

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

                        let frame = self
                            .surface
                            .get_current_texture()
                            .expect("Failed to acquire next swap chain texture");
                        let view = frame.texture.create_view(&TextureViewDescriptor::default());
                        let mut encoder = self
                            .device
                            .create_command_encoder(&CommandEncoderDescriptor { label: None });

                        // must happen after the player chunk uniform update to avoid one invalid frame
                        self.queue.write_buffer(
                            &self.chunk_projection_view_matrix_uniform_buffer,
                            0,
                            &bytemuck::cast_slice(self.camera.projection_view_matrix().as_ref()),
                        );

                        {
                            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                                label: Some("render world"),
                                color_attachments: &[Some(RenderPassColorAttachment {
                                    view: &view,
                                    depth_slice: None,
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
                            pass.set_bind_group(0, &self.chunk_bind_group, &[]);
                            pass.pop_debug_group();
                            pass.insert_debug_marker("before chunks");

                            for (position, mesh) in self.meshes.iter() {
                                pass.push_debug_group(&format!("Blocks of chunk {position:?}"));
                                pass.set_index_buffer(
                                    mesh.index_buffer.slice(..),
                                    IndexFormat::Uint16,
                                );
                                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                                pass.set_bind_group(1, &mesh.bind_group, &[]);
                                pass.pop_debug_group();
                                pass.insert_debug_marker(&format!("Drawing chunk {position:?}"));
                                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
                            }
                        }

                        self.queue.submit(Some(encoder.finish()));

                        let mut encoder = self
                            .device
                            .create_command_encoder(&CommandEncoderDescriptor { label: None });
                        {
                            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                                label: Some("render GUI"),
                                color_attachments: &[Some(RenderPassColorAttachment {
                                    view: &view,
                                    depth_slice: None,
                                    resolve_target: None,
                                    ops: Operations {
                                        load: LoadOp::Load,
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

                            pass.push_debug_group("GUI setup");
                            pass.set_pipeline(render_pipeline);
                            pass.set_bind_group(0, &self.ui_bind_group, &[]);
                            pass.pop_debug_group();
                            pass.insert_debug_marker("before GUI");

                            let (vertices, indices) = GuiMesh::generate(&self.gui);
                            let mesh = self.gui_mesh.take();
                            self.gui_mesh = Some(GuiMesh::upload_to_gpu(
                                &self.device,
                                self.player_chunk,
                                &vertices,
                                &indices,
                                &self.chunk_bind_group_layout,
                                mesh.map(|it| (it, &self.queue)),
                            ));
                            let mesh = self.gui_mesh.as_ref().unwrap();

                            pass.push_debug_group("GUI");
                            pass.set_index_buffer(mesh.index_buffer.slice(..), IndexFormat::Uint16);
                            pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                            pass.set_bind_group(1, &mesh.bind_group, &[]);
                            pass.draw_indexed(0..mesh.index_count, 0, 0..1);
                            pass.pop_debug_group();
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
                            crate::wasm::display_statistics(&self.statistics);
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
                        if self.is_locked {
                            self.input.mouse(
                                worker,
                                self.simulation,
                                self.player_chunk,
                                &self.camera,
                                state,
                                button,
                            );
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
                    WindowEvent::Touch(t) => {
                        self.input.touch(
                            &self.window,
                            t,
                            worker,
                            self.simulation,
                            self.player_chunk,
                            &mut self.camera,
                            &self.gui,
                            self.delta_time,
                        );
                    }
                    WindowEvent::KeyboardInput { event, .. } => {
                        if self.is_locked && event.logical_key == Key::Named(NamedKey::Escape) {
                            info!("Unlocking cursor");
                            self.window.set_cursor_grab(CursorGrabMode::None).unwrap();
                            self.is_locked = false;
                        } else {
                            self.input.keyboard(
                                event,
                                worker,
                                self.simulation,
                                self.player_chunk,
                                &self.camera,
                                &mut self.print_statistics,
                            );
                        }
                    }
                    _ => {}
                }
            }
            Event::DeviceEvent { event, .. } => match event {
                DeviceEvent::MouseMotion { delta } => {
                    if self.is_locked && self.window.has_focus() {
                        self.input
                            .mouse_motion(&mut self.camera, self.delta_time, delta);
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    pub fn update(&mut self, _worker: &impl Worker, message: Option<WorkerMessage>) {
        let tag = message.as_ref().map(|it| it.tag());

        match tag {
            Some(MessageTag::MeshData) => {
                self.update_mesh_data(message.unwrap());
            }
            Some(MessageTag::ChunkRemoval) => {
                self.update_chunk_removal(message.unwrap());
            }
            Some(MessageTag::ChunkInfo) => {
                // TODO implement a shortcut in worker.js to avoid coping it in and out of wasm memory?
                self.update_chunk_info_statistics(message.unwrap());
            }
            Some(MessageTag::MovementCommandReply) => {
                let message = message.unwrap();
                let mut remainder = &message.bytes[..];
                let c = WorkerMessage::take::<MovementCommandReply>(&mut remainder).unwrap();
                assert_eq!(remainder.len(), 1);

                self.camera.position = Vec3::from(c.position);

                let chunk = ChunkPosition::from_chunk_index(IVec3::from(c.player_chunk));
                if self.player_chunk != chunk {
                    self.player_chunk = chunk;
                    self.queue.write_buffer(
                        &self.player_chunk_uniform_buffer,
                        0,
                        &bytemuck::cast_slice(self.player_chunk.block().index().extend(0).as_ref()),
                    );
                }
            }
            _ => unreachable!(),
        }
    }

    fn update_mesh_data(&mut self, message: WorkerMessage) {
        let mut remaining = &message.bytes[0..];

        while let Some(mesh_data) = WorkerMessage::take::<MeshData>(&mut remaining) {
            let position = ChunkPosition::from_chunk_index(IVec3::from(mesh_data.chunk));
            let vertices = WorkerMessage::take_slice::<Vertex>(
                &mut remaining,
                mesh_data.vertex_count as usize,
            )
            .unwrap();
            let indices =
                WorkerMessage::take_slice::<u16>(&mut remaining, mesh_data.index_count as usize)
                    .unwrap();
            if mesh_data.index_count % 2 != 0 {
                WorkerMessage::take::<u16>(&mut remaining).unwrap(); // alignment
            }

            let previous_mesh = self.meshes.remove(&position);
            self.statistics.replaced_meshes += previous_mesh.is_some() as usize;

            if mesh_data.is_full_and_invisible != 0 {
                debug_assert!(indices.len() == 0);
                self.statistics.full_invisible_chunks += 1;
            }

            if indices.len() == 0 {
                // TODO is it worth it to cache the previous_mesh so that it can be recycled later?
                continue;
            }

            let (mesh, info) = ChunkMesh::upload_to_gpu(
                &self.device,
                position,
                vertices,
                indices,
                &self.chunk_bind_group_layout,
                previous_mesh.map(|it| (it, &self.queue)),
            );

            self.statistics.chunk_mesh_generated(info);

            self.meshes.insert(position, mesh);
        }
    }

    fn update_chunk_removal(&mut self, message: WorkerMessage) {
        let mut remaining = &message.bytes[0..];

        while let Some(position) = WorkerMessage::take::<[i32; 3]>(&mut remaining) {
            let position = ChunkPosition::from_chunk_index(IVec3::from_array(*position));

            // TODO recycle mesh
            self.meshes.remove(&position);
        }
    }

    fn update_chunk_info_statistics(&mut self, message: WorkerMessage) {
        let mut remaining = &message.bytes[..];
        while let Some(info) = WorkerMessage::take::<ChunkInfoBytes>(&mut remaining) {
            self.statistics.chunk_generated(ChunkInfo {
                non_air_block_count: info.non_air_block_count,
                time: Duration::new(info.time_secs, info.time_subsec_nanos),
            });
        }
        assert_eq!(remaining.len(), 1);
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
            entry_point: Some("vs_main"),
            buffers: &[ChunkMesh::VERTEX_BUFFER_LAYOUT],
            compilation_options: Default::default(),
        },
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(target)],
            compilation_options: Default::default(),
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
        cache: None,
    })
}
