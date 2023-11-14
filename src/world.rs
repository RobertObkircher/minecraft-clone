use std::collections::{HashMap, VecDeque};
use std::mem;

use glam::{IVec3, Vec3};
use log::info;
use wgpu::{BindGroupLayout, Device, Queue};

use crate::chunk::{Block, Chunk, Transparency};
use crate::mesh::ChunkMesh;
use crate::position::{BlockPosition, ChunkPosition};
use crate::statistics::Statistics;
use crate::terrain::TerrainGenerator;

#[allow(unused)]
pub struct World {
    view_distance: u16,
    highest_generated_chunk: i32,
    lowest_generated_chunk: i32,
    chunks: Vec<Chunk>,
    position_to_index: HashMap<ChunkPosition, ChunkIndex>,
    position_to_mesh: HashMap<ChunkPosition, ChunkMesh>,
    generation_queue: VecDeque<(i32, i32, i32)>,
    mesh_queue: VecDeque<ChunkPosition>,
    //simulation_regions: Vec<SimulationRegion>,
}

impl World {
    pub fn new(view_distance: u16, height: u16) -> Self {
        let mut chunk = Chunk::default();
        chunk.transparency = !0u8;

        // 2 -> -1..=0
        // 3 -> -1..=1
        // 4 -> -2..=1
        // 5 -> -2..=2
        let lowest_generated_chunk = -(height as i32) / 2;
        let highest_generated_chunk = lowest_generated_chunk + height as i32 - 1;

        let mut generation_queue = Vec::<(i32, i32, i32)>::new();
        let v = view_distance as i32;
        for x in -v..=v {
            for z in -v..=v {
                generation_queue.push((x, highest_generated_chunk, z));
            }
        }
        generation_queue.sort_by_key(|(x, _, z)| x * x + z * z);

        Self {
            view_distance,
            highest_generated_chunk,
            lowest_generated_chunk,
            chunks: vec![chunk],
            position_to_index: Default::default(),
            position_to_mesh: Default::default(),
            generation_queue: VecDeque::from(generation_queue),
            mesh_queue: VecDeque::new(),
        }
    }

    pub fn generate_chunks(
        &mut self,
        terrain: &mut TerrainGenerator,
        statistics: &mut Statistics,
        player_chunk: ChunkPosition,
    ) {
        if let Some((x, mut y, z)) = self.generation_queue.pop_front() {
            while y >= self.lowest_generated_chunk {
                let position = ChunkPosition::from_chunk_index(IVec3::new(x, y, z));
                y -= 1;

                if self.get_chunk(position).is_some() {
                    continue; // guarantee progress even if we would defer it below
                }

                let (chunk, chunk_info) = terrain.fill_chunk(position);
                statistics.chunk_generated(chunk_info);
                if let Some(chunk) = chunk {
                    self.add_chunk(position, chunk);
                } else {
                    statistics.air_chunks += 1;
                    self.add_air_chunk(position);
                }

                // defer distant underground chunks
                if y >= self.lowest_generated_chunk
                    && position.index().distance_squared(player_chunk.index()) > 5
                {
                    if let Some(above) = self.get_chunk(position.plus(IVec3::Y)) {
                        if above.get_transparency(Transparency::Computed)
                            && !above.get_transparency(Transparency::NegY)
                        {
                            self.generation_queue.push_back((x, y, z));
                            break;
                        }
                    }
                }
            }
        }
    }

    pub fn update_meshes(
        &mut self,
        device: &Device,
        queue: &Queue,
        chunk_bind_group_layout: &BindGroupLayout,
        statistics: &mut Statistics,
    ) {
        while let Some(position) = self.mesh_queue.pop_front() {
            self.get_chunk_mut(position).unwrap().in_mesh_queue = false;
            let previous_mesh = self.position_to_mesh.remove(&position);
            statistics.replaced_meshes += previous_mesh.is_some() as usize;

            let chunk = self.get_chunk(position).unwrap();
            if chunk.non_air_block_count == 0 {
                // TODO is it worth it to cache the previous_mesh so that it can be recycled later?
                continue; // invisible
            }

            let neighbours = if let Some(neighbours) = self.neighbours(position) {
                neighbours
            } else {
                continue; // we can't generate a mesh if we don't have all neighbours
            };

            if chunk.non_air_block_count == Chunk::MAX_BLOCK_COUNT
                && !neighbours.pos_x.get_transparency(Transparency::NegX)
                && !neighbours.neg_x.get_transparency(Transparency::PosX)
                && !neighbours.pos_y.get_transparency(Transparency::NegY)
                && !neighbours.neg_y.get_transparency(Transparency::PosY)
                && !neighbours.pos_z.get_transparency(Transparency::NegZ)
                && !neighbours.neg_z.get_transparency(Transparency::PosZ)
            {
                statistics.full_invisible_chunks += 1;
                continue;
            }
            let (mesh, info) = ChunkMesh::generate(
                &device,
                position,
                &chunk,
                neighbours,
                &chunk_bind_group_layout,
                previous_mesh.map(|it| (it, queue)),
            );
            statistics.chunk_mesh_generated(info);

            self.position_to_mesh.insert(position, mesh);
        }
    }

    pub fn add_chunk(&mut self, position: ChunkPosition, chunk: Chunk) {
        let index = ChunkIndex(self.chunks.len().try_into().unwrap());
        self.chunks.push(chunk);
        self.position_to_index.insert(position, index);
        self.request_mesh_update(position);
        self.request_neighbour_mesh_updates(position);
    }

    pub fn add_air_chunk(&mut self, position: ChunkPosition) {
        let index = ChunkIndex(0);
        self.position_to_index.insert(position, index);
        self.request_neighbour_mesh_updates(position);
    }

    pub fn iter_chunk_meshes(&self) -> impl Iterator<Item = (&ChunkPosition, &ChunkMesh)> {
        self.position_to_mesh.iter()
    }

    pub fn get_chunk(&self, position: ChunkPosition) -> Option<&Chunk> {
        self.position_to_index
            .get(&position)
            .map(|it| &self.chunks[it.0 as usize])
    }

    pub fn get_chunk_mut(&mut self, position: ChunkPosition) -> Option<&mut Chunk> {
        self.position_to_index
            .get(&position)
            .map(|it| &mut self.chunks[it.0 as usize])
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

    fn request_neighbour_mesh_updates(&mut self, position: ChunkPosition) {
        self.request_mesh_update(position.plus(IVec3::X));
        self.request_mesh_update(position.plus(IVec3::NEG_X));
        self.request_mesh_update(position.plus(IVec3::Y));
        self.request_mesh_update(position.plus(IVec3::NEG_Y));
        self.request_mesh_update(position.plus(IVec3::Z));
        self.request_mesh_update(position.plus(IVec3::NEG_Z));
    }

    fn request_mesh_update(&mut self, position: ChunkPosition) {
        if let Some(chunk) = self.get_chunk_mut(position) {
            if !chunk.in_mesh_queue {
                chunk.in_mesh_queue = true;
                self.mesh_queue.push_back(position);
            }
        }
    }

    pub fn find_nearest_block_on_ray(
        &self,
        start_chunk: ChunkPosition,
        offset: Vec3,
        direction: Vec3,
        max_distance: usize,
    ) -> (Option<BlockPosition>, Option<BlockPosition>) {
        let direction = direction.normalize();

        // TODO properly intersect with the faces of neighbouring cubes and handle edges/corners
        let divisor = 100;
        let mut previous = None;

        for i in 0..max_distance * divisor {
            let distance = offset + i as f32 * direction * (1.0 / divisor as f32);

            let position = start_chunk.block().plus(distance.floor().as_ivec3());
            if Some(position) == previous {
                continue;
            }

            if let Some(chunk) = self.get_chunk(position.chunk()) {
                let relative = position.index() - position.chunk().block().index();
                if !chunk.blocks[relative.x as usize][relative.y as usize][relative.z as usize]
                    .transparent()
                {
                    return (previous, Some(position));
                }
            }

            previous = Some(position);
        }
        (None, None)
    }

    pub fn set_block(&mut self, position: BlockPosition, block: Block) -> Option<Block> {
        if let Some(chunk) = self.get_chunk_mut(position.chunk()) {
            let relative = position.index() - position.chunk().block().index();

            let previous = mem::replace(
                &mut chunk.blocks[relative.x as usize][relative.y as usize][relative.z as usize],
                block,
            );

            if previous != block {
                if let Block::Air = previous {
                    chunk.non_air_block_count += 1;
                }
                if let Block::Air = block {
                    chunk.non_air_block_count -= 1;
                }
                let cs = Chunk::SIZE as i32;
                if relative.x == 0
                    || relative.y == 0
                    || relative.z == 0
                    || relative.x == cs
                    || relative.y == cs
                    || relative.z == cs
                {
                    chunk.compute_transparency();
                }

                self.request_mesh_update(position.plus(IVec3::X).chunk());
                self.request_mesh_update(position.plus(IVec3::NEG_X).chunk());
                self.request_mesh_update(position.plus(IVec3::Y).chunk());
                self.request_mesh_update(position.plus(IVec3::NEG_Y).chunk());
                self.request_mesh_update(position.plus(IVec3::Z).chunk());
                self.request_mesh_update(position.plus(IVec3::NEG_Z).chunk());
            }
        }
        None
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
