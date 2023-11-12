use std::collections::{HashMap, VecDeque};

use glam::IVec3;

use crate::chunk::Chunk;
use crate::mesh::ChunkMesh;
use crate::position::ChunkPosition;

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
