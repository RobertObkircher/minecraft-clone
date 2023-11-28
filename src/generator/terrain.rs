use crate::generator::noise::ImprovedNoise;
use crate::simulation::chunk::{Block, Chunk};
use crate::simulation::position::ChunkPosition;
use crate::statistics::ChunkInfo;
use crate::timer::Timer;
use bytemuck::{Pod, Zeroable};
use glam::IVec3;
use rand::rngs::StdRng;
use rand::SeedableRng;

#[repr(C)]
#[derive(Copy, Clone, Debug, Zeroable, Pod)]
pub struct WorldSeed(pub u64);

#[derive(Copy, Clone, Debug)]
enum Usage {
    FillChunk,
    FillWorld,
}

fn random(position: ChunkPosition, world_seed: WorldSeed, usage: Usage) -> StdRng {
    let position = position.block().index();

    let mut seed = [0u8; 32];
    seed[0..8].copy_from_slice(&world_seed.0.to_le_bytes());
    seed[8..12].copy_from_slice(&position.x.to_le_bytes());
    seed[12..16].copy_from_slice(&position.y.to_le_bytes());
    seed[16..20].copy_from_slice(&position.z.to_le_bytes());

    seed[20] = usage as u8;

    // balance out bits around (0, 0, 0) coordinates
    seed.iter_mut().for_each(|it| *it ^= 0xA5);

    StdRng::from_seed(seed)
}

pub struct TerrainGenerator {
    world_seed: WorldSeed,
    global_noise: ImprovedNoise,
}

impl TerrainGenerator {
    pub fn new(world_seed: WorldSeed) -> Self {
        let mut random = random(
            ChunkPosition::from_chunk_index(IVec3::ZERO),
            world_seed,
            Usage::FillWorld,
        );
        let global_noise = ImprovedNoise::new(&mut random);
        Self {
            world_seed,
            global_noise,
        }
    }

    fn height(&self, x: f64, z: f64, num_octaves: usize) -> f64 {
        let mut result = 0.0;
        let mut amplitude = 1.0;
        let mut frequency = 0.005;

        for _ in 0..num_octaves {
            let n = amplitude * self.global_noise.noise_2d(x * frequency, z * frequency);
            result += n;

            amplitude *= 0.5;
            frequency *= 2.0;
        }

        result
    }

    pub fn fill_chunk(&mut self, position: ChunkPosition) -> (Option<Chunk>, ChunkInfo) {
        let start = Timer::now();
        let mut result = Chunk::default();

        let mut random = random(position, self.world_seed, Usage::FillChunk);

        let position = position.block().index();

        let noise = ImprovedNoise::new(&mut random);
        let mut non_air_block_count = 0;

        for x in 0..Chunk::SIZE {
            for y in 0..Chunk::SIZE {
                for z in 0..Chunk::SIZE {
                    let block_x = position.x + x as i32;
                    let block_y = position.y + y as i32;
                    let block_z = position.z + z as i32;

                    let global_height = self.height(block_x as f64, block_z as f64, 4) * 40.0;

                    let delta_h = global_height - block_y as f64;
                    let base_density = delta_h / 127.0;

                    let noise = noise.noise(
                        block_x as f64 * 0.1,
                        block_y as f64 * 0.1,
                        block_z as f64 * 0.1,
                    );

                    let density = base_density * (1.0 + noise.abs());

                    result.blocks[x][y][z] = if density > 0.0 || block_y < 0 {
                        non_air_block_count += 1;
                        if density > 0.1 {
                            Block::Stone
                        } else if density > 0.0 {
                            if block_y < 1 {
                                Block::Sand
                            } else {
                                Block::Dirt
                            }
                        } else {
                            Block::Water
                        }
                    } else {
                        Block::Air
                    };
                }
            }
        }

        if non_air_block_count == 0 {
            return (
                None,
                ChunkInfo {
                    non_air_block_count,
                    time: start.elapsed(),
                },
            );
        }

        result.non_air_block_count = non_air_block_count;
        result.compute_transparency();

        (
            Some(result),
            ChunkInfo {
                non_air_block_count,
                time: start.elapsed(),
            },
        )
    }
}
