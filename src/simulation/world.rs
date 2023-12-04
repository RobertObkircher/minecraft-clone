use std::collections::{HashMap, HashSet, VecDeque};
use std::mem;

use glam::{IVec3, Vec3};

use crate::renderer::mesh::{ChunkMesh, Vertex};
use crate::renderer::MeshData;
use crate::simulation::chunk::{Block, Chunk, Transparency};
use crate::simulation::position::{BlockPosition, ChunkPosition};

#[allow(unused)]
pub struct World {
    view_distance: u16,
    pub highest_generated_chunk: i32,
    pub lowest_generated_chunk: i32,
    chunks: Vec<Chunk>,
    position_to_index: HashMap<ChunkPosition, ChunkIndex>,
    position_has_mesh: HashSet<ChunkPosition>,
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
            position_has_mesh: HashSet::default(),
            generation_queue: VecDeque::from(generation_queue),
            mesh_queue: VecDeque::new(),
        }
    }

    pub fn next_column_to_generate(&mut self) -> Option<(i32, i32)> {
        self.generation_queue.pop_front().map(|(x, _, z)| (x, z))
    }

    pub fn get_updated_meshes(&mut self) -> Vec<(MeshData, Vec<Vertex>, Vec<u16>)> {
        let mut result = Vec::with_capacity(self.mesh_queue.len());
        while let Some(position) = self.mesh_queue.pop_front() {
            self.get_chunk_mut(position, false).unwrap().in_mesh_queue = false;

            let chunk = self.get_chunk(position).unwrap();

            if chunk.non_air_block_count == 0 {
                result.push((
                    MeshData {
                        chunk: position.index().to_array(),
                        vertex_count: 0,
                        index_count: 0,
                        is_full_and_invisible: 0,
                    },
                    vec![],
                    vec![],
                ));
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
                result.push((
                    MeshData {
                        chunk: position.index().to_array(),
                        vertex_count: 0,
                        index_count: 0,
                        is_full_and_invisible: 1,
                    },
                    vec![],
                    vec![],
                ));
                continue;
            }

            let (vertices, indices) = ChunkMesh::generate(&chunk, neighbours);
            result.push((
                MeshData {
                    chunk: position.index().to_array(),
                    vertex_count: vertices.len() as u32,
                    index_count: indices.len() as u32,
                    is_full_and_invisible: 0,
                },
                vertices,
                indices,
            ));
        }

        result
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

    pub fn get_chunk(&self, position: ChunkPosition) -> Option<&Chunk> {
        self.position_to_index
            .get(&position)
            .map(|it| &self.chunks[it.0 as usize])
    }

    pub fn get_chunk_mut(
        &mut self,
        position: ChunkPosition,
        clone_air: bool,
    ) -> Option<&mut Chunk> {
        self.position_to_index
            .get(&position)
            .cloned()
            .filter(|it| clone_air || it.0 != 0)
            .map(|mut it| {
                if it.0 == 0 {
                    let air = self.chunks[0].clone();
                    it = ChunkIndex(self.chunks.len().try_into().unwrap());
                    self.chunks.push(air);
                    self.position_to_index.insert(position, it);
                }
                &mut self.chunks[it.0 as usize]
            })
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
        if let Some(chunk) = self.get_chunk_mut(position, false) {
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
        if let Some(chunk) = self.get_chunk_mut(position.chunk(), !matches!(block, Block::Air)) {
            let relative = position.index() - position.chunk().block().index();

            let previous = mem::replace(
                &mut chunk.blocks[relative.x as usize][relative.y as usize][relative.z as usize],
                block,
            );

            if previous != block {
                if let Block::Air = previous {
                    assert!(chunk.non_air_block_count < Chunk::MAX_BLOCK_COUNT);
                    chunk.non_air_block_count += 1;
                }
                if let Block::Air = block {
                    assert!(chunk.non_air_block_count > 0);
                    chunk.non_air_block_count -= 1;
                }
                let last = Chunk::SIZE as i32 - 1;
                if relative.x == 0
                    || relative.y == 0
                    || relative.z == 0
                    || relative.x == last
                    || relative.y == last
                    || relative.z == last
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
            return Some(previous);
        }
        if !matches!(block, Block::Air) && self.get_chunk(position.chunk()).is_some() {
            Some(Block::Air)
        } else {
            None
        }
    }

    pub fn collide(&self, chunk: ChunkPosition, offset: Vec3) -> bool {
        if let Some(chunk) = self.get_chunk(chunk) {
            let p = offset.as_uvec3();
            let block = chunk.blocks[p.x as usize][p.y as usize][p.z as usize];
            !block.transparent()
        } else {
            true
        }
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
