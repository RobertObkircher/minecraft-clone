use std::time::Instant;

use wgpu::{BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, Buffer, BufferUsages, Device};
use wgpu::util::{BufferInitDescriptor, DeviceExt};

use crate::chunk::{Block, Chunk};
use crate::statistics::ChunkMeshInfo;
use crate::Vertex;
use crate::world::ChunkPosition;

pub struct ChunkMesh {
    pub vertex_buffer: Buffer,
    pub index_buffer: Buffer,
    pub uniform_buffer: Buffer,
    pub bind_group: BindGroup,
    pub index_count: u32,
}

impl ChunkMesh {
    pub fn new(device: &Device, position: ChunkPosition, chunk: &Chunk, bind_group_layout: &BindGroupLayout) -> (ChunkMesh, ChunkMeshInfo) {
        let start = Instant::now();
        let mut vertices = vec![];
        let mut indices: Vec<u16> = vec![];

        for x in 0..Chunk::SIZE {
            for y in 0..Chunk::SIZE {
                for z in 0..Chunk::SIZE {
                    if let Block::Air = chunk.blocks[x][y][z] {
                        continue;
                    }

                    let mut add_face = |is: [u16; 6], visible: bool| {
                        if visible {
                            let offset = u16::try_from(vertices.len()).unwrap();
                            indices.extend((0..6).map(|i| i + offset));
                            vertices.extend(is.iter().map(|i| {
                                let mut v = VERTICES[*i as usize];
                                v.pos[0] += x as f32;
                                v.pos[1] += y as f32;
                                v.pos[2] += z as f32;
                                v
                            }));
                        }
                    };
                    let last = Chunk::SIZE - 1;

                    add_face([0, 1, 2, 2, 3, 0], z == last || chunk.blocks[x][y][z + 1].transparent());
                    add_face([4, 5, 6, 6, 7, 4], z == 0 || chunk.blocks[x][y][z - 1].transparent());

                    add_face([8, 9, 10, 10, 11, 8], x == last || chunk.blocks[x + 1][y][z].transparent());
                    add_face([12, 13, 14, 14, 15, 12], x == 0 || chunk.blocks[x - 1][y][z].transparent());

                    add_face([16, 17, 18, 18, 19, 16], y == last || chunk.blocks[x][y + 1][z].transparent());
                    add_face([20, 21, 22, 22, 23, 20], y == 0 || chunk.blocks[x][y - 1][z].transparent());
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
}

const fn vertex(pos: [i8; 3], tc: [i8; 2]) -> Vertex {
    Vertex {
        pos: [pos[0] as f32, pos[1] as f32, pos[2] as f32, 1.0],
        tex_coord: [tc[0] as f32, tc[1] as f32],
    }
}

const VERTICES: [Vertex; 24] = [
    // POS_Z
    vertex([0, 0, 1], [0, 0]),
    vertex([1, 0, 1], [1, 0]),
    vertex([1, 1, 1], [1, 1]),
    vertex([0, 1, 1], [0, 1]),
    // NEG_Z
    vertex([0, 1, 0], [1, 0]),
    vertex([1, 1, 0], [0, 0]),
    vertex([1, 0, 0], [0, 1]),
    vertex([0, 0, 0], [1, 1]),
    // POS_X
    vertex([1, 0, 0], [0, 0]),
    vertex([1, 1, 0], [1, 0]),
    vertex([1, 1, 1], [1, 1]),
    vertex([1, 0, 1], [0, 1]),
    // NEG_X
    vertex([0, 0, 1], [1, 0]),
    vertex([0, 1, 1], [0, 0]),
    vertex([0, 1, 0], [0, 1]),
    vertex([0, 0, 0], [1, 1]),
    // POS_Y
    vertex([1, 1, 0], [1, 0]),
    vertex([0, 1, 0], [0, 0]),
    vertex([0, 1, 1], [0, 1]),
    vertex([1, 1, 1], [1, 1]),
    // NEG_Y
    vertex([1, 0, 1], [0, 0]),
    vertex([0, 0, 1], [1, 0]),
    vertex([0, 0, 0], [1, 1]),
    vertex([1, 0, 0], [0, 1]),
];
