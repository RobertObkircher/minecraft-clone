use rand::SeedableRng;
use rand::rngs::StdRng;

use crate::chunk::{Block, Chunk};
use crate::noise::ImprovedNoise;
use crate::world::ChunkPosition;

#[derive(Copy, Clone, Debug)]
pub struct WorldSeed(pub usize);

#[derive(Copy, Clone, Debug)]
enum Usage {
    FillChunk
}

fn random(position: ChunkPosition, world_seed: WorldSeed, usage: Usage) -> StdRng {
    let position = position.block().index();

    let mut seed = [0u8; 32];
    seed[0..8].copy_from_slice(&world_seed.0.to_le_bytes());
    seed[8..12].copy_from_slice(&position.x.to_le_bytes());
    seed[12..16].copy_from_slice(&position.y.to_le_bytes());
    seed[16..20].copy_from_slice(&position.z.to_le_bytes());

    match usage {
        Usage::FillChunk => { seed[20] = 42; }
    }

    // balance out bits around (0, 0, 0) coordinates
    seed.iter_mut().for_each(|it| *it ^= 0xA5);

    StdRng::from_seed(seed)
}


pub struct TerrainGenerator {
    world_seed: WorldSeed,
}

impl TerrainGenerator {
    pub fn new(world_seed: WorldSeed) -> Self {
        Self {
            world_seed,
        }
    }
    pub fn fill_chunk(&mut self, position: ChunkPosition) -> Chunk {
        let mut result = Chunk::default();

        let mut random = random(position, self.world_seed, Usage::FillChunk);

        let position = position.block().index();
        let average_height = 0; // random.gen_range(0..7);

        let noise = ImprovedNoise::new(&mut random);
        for x in 0..Chunk::SIZE {
            for y in 0..Chunk::SIZE {
                for z in 0..Chunk::SIZE {
                    let block_x = position.x + x as i32;
                    let block_y = position.y + y as i32;
                    let block_z = position.z + z as i32;

                    let delta_h = (average_height - block_y) as f64;
                    let base_density = delta_h / 127.0;

                    let noise  = noise.noise(block_x as f64 * 0.1, block_y as f64 * 0.1, block_z as f64 * 0.1);

                    let density = base_density + noise * 0.1;

                    result.blocks[x][y][z] = if density > 0.0 {
                        Block::Dirt
                    } else {
                        Block::Air
                    };
                }
            }
        }

        result
    }
}