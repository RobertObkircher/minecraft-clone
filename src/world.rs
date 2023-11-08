use std::collections::HashMap;

use glam::IVec3;

use crate::chunk::Chunk;
use crate::mesh::ChunkMesh;

pub struct World {
    chunks: Vec<Chunk>,
    position_to_index: HashMap<ChunkPosition, ChunkIndex>,
    position_to_mesh: HashMap<ChunkPosition, ChunkMesh>,
    //simulation_regions: Vec<SimulationRegion>,
}

impl World {
    pub fn add_chunk(&mut self, position: ChunkPosition, chunk: Chunk) {
        let index = ChunkIndex(self.chunks.len().try_into().unwrap());
        self.chunks.push(chunk);
        self.position_to_index.insert(position, index);
    }

    pub fn add_mesh(&mut self, position: ChunkPosition, mesh: ChunkMesh) {
        self.position_to_mesh.insert(position, mesh);
    }

    pub fn iter_chunk_meshes(&self) -> impl Iterator<Item=(&ChunkPosition, &ChunkMesh)> {
        self.position_to_mesh.iter()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
struct ChunkIndex(u32);

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct ChunkPosition(pub IVec3);

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct BlockPosition(pub IVec3);

impl Default for World {
    fn default() -> Self {
        Self {
            chunks: vec![],
            position_to_index: Default::default(),
            position_to_mesh: Default::default(),
        }
    }
}