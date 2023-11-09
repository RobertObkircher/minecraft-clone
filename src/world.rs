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

    pub fn get_chunk(&self, position: ChunkPosition) -> Option<&Chunk> {
        self.position_to_index.get(&position).map(|it| &self.chunks[it.0 as usize])
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
struct ChunkIndex(u32);

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct ChunkPosition(IVec3);

impl ChunkPosition {
    pub fn from_chunk_index(index: IVec3) -> Self {
        Self(index)
    }

    pub fn block(self) -> BlockPosition {
        return BlockPosition(self.0 * Chunk::SIZE as i32);
    }

    #[must_use]
    pub fn plus(self, direction: IVec3) -> Self {
        self.block().plus(direction.wrapping_mul(IVec3::splat(Chunk::SIZE as i32))).chunk()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct BlockPosition(IVec3);

impl BlockPosition {
    pub fn index(self) -> IVec3 {
        self.0
    }

    pub fn chunk(self) -> ChunkPosition {
        // copied from standard library to avoid overflow checks in debug builds
        pub const fn div_floor(lhs: i32, rhs: i32) -> i32 {
            let d = lhs.wrapping_div(rhs);
            let r = lhs.wrapping_rem(rhs);
            if (r > 0 && rhs < 0) || (r < 0 && rhs > 0) {
                d.wrapping_sub(1)
            } else {
                d
            }
        }
        ChunkPosition::from_chunk_index(IVec3 {
            x: div_floor(self.0.x, Chunk::SIZE as i32),
            y: div_floor(self.0.y, Chunk::SIZE as i32),
            z: div_floor(self.0.z, Chunk::SIZE as i32),
        })
    }

    #[must_use]
    pub fn plus(self, direction: IVec3) -> BlockPosition {
        BlockPosition(self.0.wrapping_add(direction))
    }
}

#[cfg(test)]
#[test]
fn test_position() {
    let cs = Chunk::SIZE as i32;
    for i in 0..cs {
        let position = BlockPosition(IVec3::splat(i32::MIN + i));
        assert_eq!(position.chunk().0 * cs, IVec3::MIN);
    }
    for i in 0..cs {
        let position = BlockPosition(IVec3::splat(i32::MIN + cs + i));
        assert_eq!(position.chunk().0 * cs, IVec3::MIN + cs);
    }
    for i in 0..cs {
        let position = BlockPosition(IVec3::splat(-cs + i));
        assert_eq!(position.chunk().0 * cs, IVec3::splat(-cs));
    }
    for i in 0..cs {
        let position = BlockPosition(IVec3::splat(i));
        assert_eq!(position.chunk().0 * 8, IVec3::ZERO);
    }
    for i in 0..cs {
        let position = BlockPosition(IVec3::splat(cs + i));
        assert_eq!(position.chunk().0 * cs, IVec3::splat(cs));
    }
    for i in 0..cs {
        let position = BlockPosition(IVec3::splat(i32::MAX - i));
        assert_eq!(position.chunk().0 * cs, IVec3::MAX - cs + 1);
    }
}

impl Default for World {
    fn default() -> Self {
        Self {
            chunks: vec![],
            position_to_index: Default::default(),
            position_to_mesh: Default::default(),
        }
    }
}