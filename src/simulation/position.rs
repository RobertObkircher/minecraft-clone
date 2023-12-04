use crate::simulation::chunk::Chunk;
use glam::{IVec3, Vec3};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct ChunkPosition(IVec3);

impl ChunkPosition {
    pub fn from_chunk_index(index: IVec3) -> Self {
        Self(index)
    }

    pub fn block(self) -> BlockPosition {
        return BlockPosition(self.0 * Chunk::SIZE as i32);
    }

    pub fn index(self) -> IVec3 {
        return self.0;
    }
    #[must_use]
    pub fn plus(self, direction: IVec3) -> Self {
        self.block()
            .plus(direction.wrapping_mul(IVec3::splat(Chunk::SIZE as i32)))
            .chunk()
    }

    pub fn normalize(self, relative: Vec3) -> (ChunkPosition, Vec3) {
        let chunk_offset = BlockPosition::new(relative.floor().as_ivec3())
            .chunk()
            .index();
        if chunk_offset != IVec3::ZERO {
            (
                self.plus(chunk_offset),
                relative - (chunk_offset * Chunk::SIZE as i32).as_vec3(),
            )
        } else {
            (self, relative)
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct BlockPosition(IVec3);

impl BlockPosition {
    pub fn new(index: IVec3) -> Self {
        Self(index)
    }
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
