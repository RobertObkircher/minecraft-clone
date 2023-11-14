use std::mem;
use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use wgpu::util::{BufferInitDescriptor, DeviceExt};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingType, Buffer, BufferAddress, BufferBindingType, BufferSize,
    BufferUsages, Device, Queue, ShaderStages, VertexAttribute, VertexBufferLayout, VertexFormat,
    VertexStepMode,
};

use crate::chunk::{Block, Chunk, Transparency};
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
    #[rustfmt::skip]
    pub fn generate(device: &Device, position: ChunkPosition, chunk: &Chunk, neighbours: ChunkNeighbours, bind_group_layout: &BindGroupLayout, recycle_mesh: Option<(ChunkMesh, &Queue)>) -> (ChunkMesh, ChunkMeshInfo) {
        let start = Instant::now();

        let mut vertices = vec![];
        let mut indices: Vec<u16> = vec![];

        let mut add_face = |xyz: (usize, usize, usize), face_index: u32, neighbour: &Block| {
            if neighbour.transparent() {
                let (is, texture) = [
                    ([8, 9, 10, 10, 11, 8], [1, 0]),
                    ([12, 13, 14, 14, 15, 12], [1, 0]),
                    ([16, 17, 18, 18, 19, 16], [0, 0]),
                    ([20, 21, 22, 22, 23, 20], [0, 1]),
                    ([0, 1, 2, 2, 3, 0], [1, 0]),
                    ([4, 5, 6, 6, 7, 4], [1, 0])
                ][face_index as usize];

                let offset = u16::try_from(vertices.len()).unwrap();
                indices.extend((0..6).map(|i| i + offset));
                vertices.extend(is.iter().map(|i| {
                    let (mut pos, mut tex_coord) = VERTICES[*i as usize];
                    pos[0] += xyz.0 as f32;
                    pos[1] += xyz.1 as f32;
                    pos[2] += xyz.2 as f32;

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

        const S: usize = Chunk::SIZE;
        const E: usize = S - 1; // end

        for x in 0..S {
            for y in 0..S {
                for z in 0..S {
                    if let Block::Air = chunk.blocks[x][y][z] {
                        continue;
                    }
                    let xyz = (x, y, z);

                    if x != E { add_face(xyz, 0, &chunk.blocks[x + 1][y][z]); }
                    if x != 0 { add_face(xyz, 1, &chunk.blocks[x - 1][y][z]); }
                    if y != E { add_face(xyz, 2, &chunk.blocks[x][y + 1][z]); }
                    if y != 0 { add_face(xyz, 3, &chunk.blocks[x][y - 1][z]); }
                    if z != E { add_face(xyz, 4, &chunk.blocks[x][y][z + 1]); }
                    if z != 0 { add_face(xyz, 5, &chunk.blocks[x][y][z - 1]); }
                }
            }
        }

        let mut make_face = |offset: (usize, usize, usize), step: (usize, usize, usize), face_index: u32, neighbour: &Chunk, transparency: Transparency| {
            if neighbour.get_transparency(transparency) {
                for x in (offset.0..S).step_by(step.0) {
                    for y in (offset.1..S).step_by(step.1) {
                        for z in (offset.2..S).step_by(step.2) {
                            if let Block::Air = chunk.blocks[x][y][z] {
                                continue;
                            }
                            let ix = if step.0 == 1 { x } else if offset.0 == 0 { E } else { 0 };
                            let iy = if step.1 == 1 { y } else if offset.1 == 0 { E } else { 0 };
                            let iz = if step.2 == 1 { z } else if offset.2 == 0 { E } else { 0 };

                            add_face((x, y, z), face_index, &neighbour.blocks[ix][iy][iz]);
                        }
                    }
                }
            };
        };

        make_face((E, 0, 0), (S, 1, 1), 0, neighbours.pos_x, Transparency::NegX);
        make_face((0, 0, 0), (S, 1, 1), 1, neighbours.neg_x, Transparency::PosX);

        make_face((0, E, 0), (1, S, 1), 2, neighbours.pos_y, Transparency::NegY);
        make_face((0, 0, 0), (1, S, 1), 3, neighbours.neg_y, Transparency::PosY);

        make_face((0, 0, E), (1, 1, S), 4, neighbours.pos_z, Transparency::NegZ);
        make_face((0, 0, 0), (1, 1, S), 5, neighbours.neg_z, Transparency::PosZ);

        let vertex_bytes: &[u8] = bytemuck::cast_slice(&vertices);
        let index_bytes: &[u8] = bytemuck::cast_slice(&indices);
        let uniform_data = position.block().index();
        let uniform_bytes: &[u8] = bytemuck::cast_slice(uniform_data.as_ref());

        let (vertex_buffer, index_buffer, uniform_buffer, bind_group) = if let Some((recycle_mesh, queue)) = recycle_mesh {
            let vertex_buffer = Some(recycle_mesh.vertex_buffer)
                .filter(|it| it.size() >= vertex_bytes.len() as u64);
            vertex_buffer.iter().for_each(|it| queue.write_buffer(&it, 0, vertex_bytes));

            let index_buffer = Some(recycle_mesh.index_buffer)
                .filter(|it| it.size() >= index_bytes.len() as u64);
            index_buffer.iter().for_each(|it| queue.write_buffer(&it, 0, index_bytes));

            let uniform_buffer = recycle_mesh.uniform_buffer;
            queue.write_buffer(&uniform_buffer, 0, uniform_bytes);

            (vertex_buffer, index_buffer, uniform_buffer, recycle_mesh.bind_group)
        } else {
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
            (None, None, uniform_buffer, bind_group)
        };

        let recycled_vertex_buffer = vertex_buffer.is_some();
        let vertex_buffer = vertex_buffer.unwrap_or_else(|| device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Chunk Vertex Buffer"),
            contents: vertex_bytes,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        }));

        let recycled_index_buffer = index_buffer.is_some();
        let index_buffer = index_buffer.unwrap_or_else(|| device.create_buffer_init(&BufferInitDescriptor {
            label: Some("Chunk Index Buffer"),
            contents: index_bytes,
            usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
        }));

        (Self {
            vertex_buffer,
            index_buffer,
            uniform_buffer,
            bind_group,
            index_count: indices.len().try_into().unwrap(),
        }, ChunkMeshInfo {
            time: start.elapsed(),
            face_count: indices.len() / 6,
            recycled_index_buffer,
            recycled_vertex_buffer,
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

    pub const BIND_GROUP_LAYOUT_DESCRIPTOR: BindGroupLayoutDescriptor<'static> =
        BindGroupLayoutDescriptor {
            label: None,
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: BufferSize::new(12),
                },
                count: None,
            }],
        };
}

const fn vertex(pos: [i8; 3], tc: [i8; 2]) -> ([f32; 4], [f32; 2]) {
    (
        [pos[0] as f32, pos[1] as f32, pos[2] as f32, 1.0],
        [tc[0] as f32, tc[1] as f32],
    )
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
