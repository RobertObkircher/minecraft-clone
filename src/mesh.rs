use std::mem;
use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use wgpu::{BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, Buffer, BufferAddress, BufferBindingType, BufferSize, BufferUsages, Device, ShaderStages, VertexAttribute, VertexBufferLayout, VertexFormat, VertexStepMode};
use wgpu::util::{BufferInitDescriptor, DeviceExt};

use crate::chunk::{Block, Chunk};
use crate::position::ChunkPosition;
use crate::statistics::ChunkMeshInfo;
use crate::world::ChunkNeighbours;

pub struct ChunkMesh {
    pub vertex_buffer: Buffer,
    pub index_buffer: Buffer,
    pub uniform_buffer: Buffer,
    pub bind_group: BindGroup,
    pub index_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 4],
    tex_coord: [f32; 2],
    face_index: u32,
}

impl ChunkMesh {
    pub fn generate(device: &Device, position: ChunkPosition, chunk: &Chunk, neighbours: ChunkNeighbours, bind_group_layout: &BindGroupLayout) -> (ChunkMesh, ChunkMeshInfo) {
        let start = Instant::now();

        let mut vertices = vec![];
        let mut indices: Vec<u16> = vec![];

        for x in 0..Chunk::SIZE {
            for y in 0..Chunk::SIZE {
                for z in 0..Chunk::SIZE {
                    if let Block::Air = chunk.blocks[x][y][z] {
                        continue;
                    }

                    let mut add_face = |is: [u16; 6], texture: [u16; 2], face_index: u32, visible: bool| {
                        if visible {
                            let offset = u16::try_from(vertices.len()).unwrap();
                            indices.extend((0..6).map(|i| i + offset));
                            vertices.extend(is.iter().map(|i| {
                                let (mut pos, mut tex_coord) = VERTICES[*i as usize];
                                pos[0] += x as f32;
                                pos[1] += y as f32;
                                pos[2] += z as f32;

                                let u_tiles = 2.0;
                                let v_tiles = 2.0;
                                tex_coord[0] += texture[0] as f32;
                                tex_coord[1] += texture[1] as f32;
                                tex_coord[0] /= u_tiles;
                                tex_coord[1] /= v_tiles;
                                Vertex {
                                    pos,
                                    tex_coord,
                                    face_index,
                                }
                            }));
                        }
                    };
                    let last = Chunk::SIZE - 1;

                    add_face([0, 1, 2, 2, 3, 0], [1, 0], 4, z == last || chunk.blocks[x][y][z + 1].transparent());
                    add_face([4, 5, 6, 6, 7, 4], [1, 1], 5, z == 0 || chunk.blocks[x][y][z - 1].transparent());

                    add_face([8, 9, 10, 10, 11, 8], [1, 0], 0, x == last || chunk.blocks[x + 1][y][z].transparent());
                    add_face([12, 13, 14, 14, 15, 12], [1, 0], 1, x == 0 || chunk.blocks[x - 1][y][z].transparent());

                    add_face([16, 17, 18, 18, 19, 16], [0, 0], 2, y == last || chunk.blocks[x][y + 1][z].transparent());
                    add_face([20, 21, 22, 22, 23, 20], [0, 1], 3, y == 0 || chunk.blocks[x][y - 1][z].transparent());
                }
            }
        }
        let vertex_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Chunk Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Chunk Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: BufferUsages::INDEX,
        });

        let uniform_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Chunk Uniform Buffer"),
            contents: bytemuck::cast_slice(position.block().index().as_ref()),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                }
            ],
            label: None,
        });

        (Self {
            vertex_buffer,
            index_buffer,
            uniform_buffer,
            bind_group,
            index_count: indices.len().try_into().unwrap(),
        }, ChunkMeshInfo {
            time: start.elapsed(),
            face_count: indices.len() / 6,
        })
    }

    pub const VERTEX_BUFFER_LAYOUT: VertexBufferLayout<'static> = VertexBufferLayout {
        array_stride: mem::size_of::<Vertex>() as BufferAddress,
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
            VertexAttribute {
                format: VertexFormat::Uint32,
                offset: 4 * 4 + 2 * 4,
                shader_location: 2,
            },
        ],
    };

    pub const BIND_GROUP_LAYOUT_DESCRIPTOR: BindGroupLayoutDescriptor<'static> = BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: BufferSize::new(12),
                },
                count: None,
            }
        ],
    };
}

const fn vertex(pos: [i8; 3], tc: [i8; 2]) -> ([f32; 4], [f32; 2]) {
    ([pos[0] as f32, pos[1] as f32, pos[2] as f32, 1.0], [tc[0] as f32, tc[1] as f32])
}

const VERTICES: [([f32; 4], [f32; 2]); 24] = [
    // texture: for sides v = !y
    // POS_Z u=x
    vertex([0, 0, 1], [0, 1]),
    vertex([1, 0, 1], [1, 1]),
    vertex([1, 1, 1], [1, 0]),
    vertex([0, 1, 1], [0, 0]),
    // NEG_Z u=!x
    vertex([0, 1, 0], [1, 0]),
    vertex([1, 1, 0], [0, 0]),
    vertex([1, 0, 0], [0, 1]),
    vertex([0, 0, 0], [1, 1]),
    // POS_X u=!z
    vertex([1, 0, 0], [1, 1]),
    vertex([1, 1, 0], [1, 0]),
    vertex([1, 1, 1], [0, 0]),
    vertex([1, 0, 1], [0, 1]),
    // NEG_X u=z
    vertex([0, 0, 1], [1, 1]),
    vertex([0, 1, 1], [1, 0]),
    vertex([0, 1, 0], [0, 0]),
    vertex([0, 0, 0], [0, 1]),
    // POS_Y uv = xz
    vertex([1, 1, 0], [1, 0]),
    vertex([0, 1, 0], [0, 0]),
    vertex([0, 1, 1], [0, 1]),
    vertex([1, 1, 1], [1, 1]),
    // NEG_Y uv=!x!z
    vertex([1, 0, 1], [0, 0]),
    vertex([0, 0, 1], [1, 0]),
    vertex([0, 0, 0], [1, 1]),
    vertex([1, 0, 0], [0, 1]),
];
