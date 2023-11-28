use std::mem::size_of_val;
use std::time::Duration;

use bytemuck::{Contiguous, Pod, Zeroable};
use glam::{IVec3, Vec3};

use chunk::{Block, Chunk};
use position::ChunkPosition;
use world::World;

use crate::generator::terrain::WorldSeed;
use crate::generator::ChunkColumnElement;
use crate::worker::{MessageTag, Worker, WorkerId, WorkerMessage};

pub mod chunk;
pub mod position;
pub mod world;

pub struct SimulationState {
    seed: WorldSeed,
    world: World,
    workers: Vec<WorkerId>,
    worker_task_count: usize,
    next_worker_index: usize,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Zeroable, Pod)]
pub struct PlayerCommand {
    pub player_chunk: [i32; 3],
    pub position: [f32; 3],
    pub direction: [f32; 3],
    pub diameter: i32,
}

impl SimulationState {
    pub fn initialize<W: Worker>(
        worker: &mut W,
        message: WorkerMessage,
    ) -> (Self, Option<Duration>) {
        let seed = *bytemuck::from_bytes(&message.bytes[0..8]);

        let world = World::new(12, 16);

        let workers = (0..W::available_parallelism().get())
            .map(|_| worker.spawn_child())
            .collect::<Vec<_>>();

        workers.iter().for_each(|&w| {
            worker.send_message(w, {
                let mut message = [0u8; 17];
                message[0..8].copy_from_slice(bytemuck::bytes_of(&seed));
                message[8..12].copy_from_slice(bytemuck::bytes_of(&world.highest_generated_chunk));
                message[12..16].copy_from_slice(bytemuck::bytes_of(&world.lowest_generated_chunk));
                *message.last_mut().unwrap() = MessageTag::InitGenerator as u8;
                Box::new(message)
            })
        });

        let mut state = SimulationState {
            seed,
            world,
            workers,
            worker_task_count: 0,
            next_worker_index: 0,
        };

        state.send_commands_to_workers(worker);

        (state, None)
    }

    fn send_commands_to_workers(&mut self, worker: &mut impl Worker) {
        while let Some((x, z)) = self.world.next_column_to_generate() {
            let mut message = [0u8; 9];
            message[0..4].copy_from_slice(&x.to_ne_bytes());
            message[4..8].copy_from_slice(&z.to_ne_bytes());
            *message.last_mut().unwrap() = MessageTag::GenerateColumn as u8;

            worker.send_message(self.workers[self.next_worker_index], Box::new(message));

            self.worker_task_count += 1;

            self.next_worker_index += 1;
            if self.next_worker_index >= self.workers.len() {
                self.next_worker_index = 0;
            }
        }
    }

    pub fn update(
        &mut self,
        worker: &impl Worker,
        message: Option<WorkerMessage>,
    ) -> Option<Duration> {
        let tag = message.as_ref().map(WorkerMessage::tag);

        match tag {
            Some(MessageTag::ChunkInfo) => {
                worker.send_message(WorkerId::Parent, message.unwrap().bytes);
                return None;
            }
            Some(MessageTag::GenerateColumnReply) => {
                let message = message.unwrap();
                let mut remainder = &*message.bytes;

                let x = *WorkerMessage::take::<i32>(&mut remainder).unwrap();
                let z = *WorkerMessage::take::<i32>(&mut remainder).unwrap();

                for y in self.world.lowest_generated_chunk..=self.world.highest_generated_chunk {
                    let is_some = remainder[0];
                    let position = ChunkPosition::from_chunk_index(IVec3::new(x, y, z));

                    if is_some == 1 {
                        let element =
                            WorkerMessage::take::<ChunkColumnElement>(&mut remainder).unwrap();
                        let chunk = Chunk {
                            blocks: element.blocks.map(|it| {
                                it.map(|it| it.map(|it| Block::from_integer(it).unwrap()))
                            }),
                            transparency: element.transparency,
                            in_mesh_queue: false,
                            non_air_block_count: element.non_air_block_count,
                        };
                        self.world.add_chunk(position, chunk);
                    } else {
                        self.world.add_air_chunk(position);
                        remainder = &remainder[2..];
                    }
                }
            }
            Some(MessageTag::PlayerCommand) => {
                let message = message.unwrap();
                let mut remainder = &message.bytes[..];

                let c = WorkerMessage::take::<PlayerCommand>(&mut remainder).unwrap();
                assert_eq!(remainder.len(), 1);

                let hit = self.world.find_nearest_block_on_ray(
                    ChunkPosition::from_chunk_index(IVec3::from(c.player_chunk)),
                    Vec3::from(c.position),
                    Vec3::from(c.direction),
                    200,
                );

                let (hit, block) = if c.diameter > 0 {
                    (hit.0, Block::Dirt)
                } else {
                    (hit.1, Block::Air)
                };
                if let Some(hit) = hit {
                    let d = c.diameter.abs();
                    let r = d / 2;
                    for x in 0..d {
                        for y in 0..d {
                            for z in 0..d {
                                let delta = IVec3::new(x, y, z) - r;
                                if delta.length_squared() <= r * r {
                                    self.world.set_block(hit.plus(delta), block);
                                }
                            }
                        }
                    }
                }
            }
            _ => unreachable!("Unknown message"),
        }

        let meshes = self.world.get_updated_meshes();
        if meshes.len() > 0 {
            let total_size = meshes
                .iter()
                .map(|it| {
                    size_of_val(&it.0)
                        + size_of_val(it.1.as_slice())
                        + size_of_val(it.2.as_slice())
                        + if it.2.len() % 2 == 0 { 0 } else { 2 }
                })
                .sum::<usize>();

            let mut message = Vec::with_capacity(total_size + 1);
            for (mesh_data, vertices, indices) in meshes {
                message.extend_from_slice(bytemuck::bytes_of(&mesh_data));
                message.extend_from_slice(bytemuck::cast_slice(&vertices));
                message.extend_from_slice(bytemuck::cast_slice(&indices));
                if indices.len() % 2 != 0 {
                    message.extend_from_slice(&[0, 0]); // align
                }
            }
            debug_assert_eq!(message.len(), total_size);
            message.push(MessageTag::MeshData as u8);

            let message = message.into_boxed_slice();
            worker.send_message(WorkerId::Parent, message);
        }

        None
    }
}
