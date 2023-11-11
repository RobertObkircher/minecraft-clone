use std::time::Instant;
use wgpu::{Buffer, BufferUsages, Device};
use wgpu::util::{BufferInitDescriptor, DeviceExt};

use crate::chunk::{Block, Chunk};
use crate::statistics::ChunkMeshInfo;
use crate::Vertex;
use crate::world::ChunkPosition;

pub struct ChunkMesh {
    pub vertex_buffer: Buffer,
    pub index_buffer: Buffer,
    pub index_count: u32,
}

impl ChunkMesh {
    pub fn new(device: &Device, position: ChunkPosition, chunk: &Chunk) -> (ChunkMesh, ChunkMeshInfo) {
        let start = Instant::now();
        let mut vs = vec![];
        let mut is: Vec<u16> = vec![];

        // TODO this should be a uniform
        let offset = position.block().index();
        for x in 0..Chunk::SIZE {
            for y in 0..Chunk::SIZE {
                for z in 0..Chunk::SIZE {
                    let visible = x == 0 || x == Chunk::SIZE - 1
                        || y == 0 || y == Chunk::SIZE - 1
                        || z == 0 || z == Chunk::SIZE - 1
                        || chunk.blocks[x - 1][y][z].transparent()
                        || chunk.blocks[x + 1][y][z].transparent()
                        || chunk.blocks[x][y - 1][z].transparent()
                        || chunk.blocks[x][y + 1][z].transparent()
                        || chunk.blocks[x][y][z - 1].transparent()
                        || chunk.blocks[x][y][z + 1].transparent();
                    if visible {
                        if let Block::Dirt = chunk.blocks[x][y][z] {
                            let (mut vertex_data, index_data) = create_vertices();

                            is.extend(index_data.iter().map(|i| i + u16::try_from(vs.len()).unwrap()));
                            for v in vertex_data.iter_mut() {
                                v.pos[0] += x as f32 + offset.x as f32;
                                v.pos[1] += y as f32 + offset.y as f32;
                                v.pos[2] += z as f32 + offset.z as f32;
                            }
                            vs.extend_from_slice(&vertex_data);
                        }
                    }
                }
            }
        }
        let vertex_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&vs),
            usage: BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(&is),
            usage: BufferUsages::INDEX,
        });

        (Self {
            vertex_buffer,
            index_buffer,
            index_count: is.len().try_into().unwrap(),
        }, ChunkMeshInfo {
            time: start.elapsed(),
            face_count: is.len() / 6,
        })
    }
}

fn vertex(pos: [i8; 3], tc: [i8; 2]) -> Vertex {
    Vertex {
        pos: [pos[0] as f32, pos[1] as f32, pos[2] as f32, 1.0],
        tex_coord: [tc[0] as f32, tc[1] as f32],
    }
}

fn create_vertices() -> (Vec<Vertex>, Vec<u16>) {
    let vertex_data = [
        vertex([0, 0, 1], [0, 0]),
        vertex([1, 0, 1], [1, 0]),
        vertex([1, 1, 1], [1, 1]),
        vertex([0, 1, 1], [0, 1]),
        vertex([0, 1, 0], [1, 0]),
        vertex([1, 1, 0], [0, 0]),
        vertex([1, 0, 0], [0, 1]),
        vertex([0, 0, 0], [1, 1]),
        vertex([1, 0, 0], [0, 0]),
        vertex([1, 1, 0], [1, 0]),
        vertex([1, 1, 1], [1, 1]),
        vertex([1, 0, 1], [0, 1]),
        vertex([0, 0, 1], [1, 0]),
        vertex([0, 1, 1], [0, 0]),
        vertex([0, 1, 0], [0, 1]),
        vertex([0, 0, 0], [1, 1]),
        vertex([1, 1, 0], [1, 0]),
        vertex([0, 1, 0], [0, 0]),
        vertex([0, 1, 1], [0, 1]),
        vertex([1, 1, 1], [1, 1]),
        vertex([1, 0, 1], [0, 0]),
        vertex([0, 0, 1], [1, 0]),
        vertex([0, 0, 0], [1, 1]),
        vertex([1, 0, 0], [0, 1]),
    ];

    let index_data: &[u16] = &[
        0, 1, 2, 2, 3, 0, // top
        4, 5, 6, 6, 7, 4, // bottom
        8, 9, 10, 10, 11, 8, // right
        12, 13, 14, 14, 15, 12, // left
        16, 17, 18, 18, 19, 16, // front
        20, 21, 22, 22, 23, 20, // back
    ];

    (vertex_data.to_vec(), index_data.to_vec())
}