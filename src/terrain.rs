use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

use crate::chunk::{Block, Chunk};
use crate::world::ChunkPosition;

pub fn fill_chunk(position: ChunkPosition) -> Chunk {
    let mut result = Chunk::default();

    let position = position.0 * Chunk::SIZE as i32;

    let mut seed = [0xA5u8; 32];
    seed[0..4].copy_from_slice(&position.x.to_le_bytes());
    seed[4..8].copy_from_slice(&position.y.to_le_bytes());
    seed[8..12].copy_from_slice(&position.z.to_le_bytes());

    let mut random = StdRng::from_seed(seed);

    let average_height = random.gen_range(0..7);

    for x in 0..Chunk::SIZE {
        for z in 0..Chunk::SIZE {
            let height = average_height + random.gen_range(-0..=1);

            for y in 0..Chunk::SIZE {
                let _block_x = position.x + x as i32;
                let block_y = position.y + y as i32;
                let _block_z = position.z + z as i32;

                result.blocks[x][y][z] = if block_y <= height {
                    Block::Dirt
                } else {
                    Block::Air
                };
            }
        }
    }

    result
}