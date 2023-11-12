use std::collections::{HashMap, VecDeque};

use glam::IVec3;
use wgpu::{BindGroupLayout, Device};

use crate::chunk::{Chunk, Transparency};
use crate::mesh::ChunkMesh;
use crate::position::ChunkPosition;
use crate::statistics::Statistics;
use crate::terrain::TerrainGenerator;

pub struct World {
    chunks: Vec<Chunk>,
    position_to_index: HashMap<ChunkPosition, ChunkIndex>,
    position_to_mesh: HashMap<ChunkPosition, ChunkMesh>,
    pub generation_queue: VecDeque<(i32, i32)>,
    pub mesh_queue: VecDeque<ChunkPosition>,
    //simulation_regions: Vec<SimulationRegion>,
}

impl World {
    pub fn new(view_distance: u16) -> Self {
        let mut chunk = Chunk::default();
        chunk.transparency = !0u8;
        chunk.has_valid_mesh = true;

        let mut generation_queue = Vec::<(i32, i32)>::new();
        let view_distance = view_distance as i32;
        for x in -view_distance..=view_distance {
            for z in -view_distance..=view_distance {
                generation_queue.push((x, z));
            }
        }
        generation_queue.sort_by_key(|(x, z)| x * x + z * z);

        Self {
            chunks: vec![chunk],
            position_to_index: Default::default(),
            position_to_mesh: Default::default(),
            generation_queue: VecDeque::from(generation_queue),
            mesh_queue: VecDeque::new(),
        }
    }

    pub fn generate_chunks(&mut self, terrain: &mut TerrainGenerator, statistics: &mut Statistics) {
        if let Some((x, z)) = self.generation_queue.pop_front() {
            let height = 16;
            for y in (0..height).into_iter().map(|it| it - height / 2).rev() {
                let position = ChunkPosition::from_chunk_index(IVec3::new(x, y, z));

                if self.get_chunk_mut(position).is_some() {
                    continue;
                }

                let (chunk, chunk_info) = terrain.fill_chunk(position);
                statistics.chunk_generated(chunk_info);
                if let Some(chunk) = chunk {
                    self.add_chunk(position, chunk);
                } else {
                    statistics.air_chunks += 1;
                    self.add_air_chunk(position);
                }

                if let Some(above) = self.get_chunk(position.plus(IVec3::Y)) {
                    if above.get_transparency(Transparency::Computed) && !above.get_transparency(Transparency::NegY) {
                        self.generation_queue.push_back((x, z));
                        break;
                    }
                }
            }
        }
    }

    pub fn generate_meshes(&mut self, device: &Device, chunk_bind_group_layout: &BindGroupLayout, statistics: &mut Statistics) {
        while let Some(position) = self.mesh_queue.pop_front() {
            let chunk = self.get_chunk(position).unwrap();
            let neighbours = self.neighbours(position).unwrap();
            if chunk.non_air_block_count == Chunk::MAX_BLOCK_COUNT &&
                !neighbours.pos_x.get_transparency(Transparency::NegX) &&
                !neighbours.neg_x.get_transparency(Transparency::PosX) &&
                !neighbours.pos_y.get_transparency(Transparency::NegY) &&
                !neighbours.neg_y.get_transparency(Transparency::PosY) &&
                !neighbours.pos_z.get_transparency(Transparency::NegZ) &&
                !neighbours.neg_z.get_transparency(Transparency::PosZ) {
                statistics.full_invisible_chunks += 1;
                continue;
            }
            let (mesh, info) = ChunkMesh::generate(&device, position, &chunk, neighbours, &chunk_bind_group_layout);
            statistics.chunk_mesh_generated(info);
            self.add_mesh(position, mesh);
        }
    }

    pub fn add_chunk(&mut self, position: ChunkPosition, chunk: Chunk) {
        let index = ChunkIndex(self.chunks.len().try_into().unwrap());
        self.chunks.push(chunk);
        self.position_to_index.insert(position, index);
        self.update_mesh(position);
        self.update_neighbour_meshes(position);
    }

    pub fn add_air_chunk(&mut self, position: ChunkPosition) {
        let index = ChunkIndex(0);
        self.position_to_index.insert(position, index);
        self.update_neighbour_meshes(position);
    }

    pub fn add_mesh(&mut self, position: ChunkPosition, mesh: ChunkMesh) {
        self.position_to_mesh.insert(position, mesh);
    }

    pub fn iter_chunk_meshes(&self) -> impl Iterator<Item=(&ChunkPosition, &ChunkMesh)> {
        self.position_to_mesh.iter()
    }

    pub fn get_chunk(&self, position: ChunkPosition) -> Option<&Chunk> {
        self.position_to_index.get(&position).map(|it| &self.chunks[it.0 as usize])
    }

    pub fn get_chunk_mut(&mut self, position: ChunkPosition) -> Option<&mut Chunk> {
        self.position_to_index.get(&position).map(|it| &mut self.chunks[it.0 as usize])
    }

    pub fn neighbours(&self, position: ChunkPosition) -> Option<ChunkNeighbours> {
        Some(ChunkNeighbours {
            pos_x: self.get_chunk(position.plus(IVec3::X))?,
            neg_x: self.get_chunk(position.plus(IVec3::NEG_X))?,
            pos_y: self.get_chunk(position.plus(IVec3::Y))?,
            neg_y: self.get_chunk(position.plus(IVec3::NEG_Y))?,
            pos_z: self.get_chunk(position.plus(IVec3::Z))?,
            neg_z: self.get_chunk(position.plus(IVec3::NEG_Z))?,
        })
    }

    fn update_neighbour_meshes(&mut self, position: ChunkPosition) {
        self.update_mesh(position.plus(IVec3::X));
        self.update_mesh(position.plus(IVec3::NEG_X));
        self.update_mesh(position.plus(IVec3::Y));
        self.update_mesh(position.plus(IVec3::NEG_Y));
        self.update_mesh(position.plus(IVec3::Z));
        self.update_mesh(position.plus(IVec3::NEG_Z));
    }

    fn update_mesh(&mut self, position: ChunkPosition) {
        if self.get_chunk(position).map(|it| it.has_valid_mesh).unwrap_or(false) {
            return;
        }
        let neighbours = self.neighbours(position);
        if neighbours.is_none() {
            return;
        }
        let chunk = self.get_chunk_mut(position).unwrap();
        chunk.has_valid_mesh = true;
        self.mesh_queue.push_back(position);
    }
}

pub struct ChunkNeighbours<'a> {
    pub pos_x: &'a Chunk,
    pub neg_x: &'a Chunk,
    pub pos_y: &'a Chunk,
    pub neg_y: &'a Chunk,
    pub pos_z: &'a Chunk,
    pub neg_z: &'a Chunk,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
struct ChunkIndex(u32);
