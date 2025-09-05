use std::mem;
use std::mem::size_of_val;
use std::time::Duration;

use bytemuck::{Contiguous, Pod, Zeroable};
use glam::{IVec3, Vec3};

use chunk::{Block, Chunk};
use position::ChunkPosition;
use world::World;

use crate::generator::ChunkColumnElement;
use crate::generator::terrain::WorldSeed;
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
    player_chunk: ChunkPosition,
    player_position: Vec3,
    last_world_cropping_player_chunk: ChunkPosition,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Zeroable, Pod)]
pub struct PlayerCommand {
    pub player_chunk: [i32; 3],
    pub position: [f32; 3],
    pub direction: [f32; 3],
    pub diameter: i32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Zeroable, Pod)]
pub struct MovementCommand {
    pub direction: [f32; 3],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Zeroable, Pod)]
pub struct MovementCommandReply {
    pub player_chunk: [i32; 3],
    pub position: [f32; 3],
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

        let player_chunk = ChunkPosition::from_chunk_index(IVec3::new(0, 3, 0));
        let mut state = SimulationState {
            seed,
            world,
            workers,
            worker_task_count: 0,
            next_worker_index: 0,
            player_chunk,
            player_position: Vec3::new(6.0, 6.0, 6.0),
            last_world_cropping_player_chunk: player_chunk,
        };

        state.send_commands_to_workers(worker);

        (state, None)
    }

    fn send_commands_to_workers(&mut self, worker: &impl Worker) {
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
            Some(MessageTag::MovementCommand) => {
                let message = message.unwrap();
                let mut remainder = &message.bytes[..];
                let c = WorkerMessage::take::<MovementCommand>(&mut remainder).unwrap();
                assert_eq!(remainder.len(), 1);

                let mut final_movement = Vec3::from(c.direction);
                for _ in 0..10 {
                    let (new_chunk, new_position) = self
                        .player_chunk
                        .normalize(self.player_position + final_movement);

                    if self.world.collide(new_chunk, new_position) {
                        break;
                    }
                    let mut adjust = None;
                    for dx in (-1..=1).step_by(2) {
                        for dy in (-2..=2).step_by(4) {
                            for dz in (-1..=1).step_by(2) {
                                let offset = IVec3::new(dx, dy, dz).as_vec3() * 0.3;

                                let (new_chunk, new_position) =
                                    new_chunk.normalize(new_position + offset);

                                if self.world.collide(new_chunk, new_position) {
                                    let amount = -offset * 0.1;
                                    adjust = adjust.map(|a| a + amount).or(Some(amount));
                                }
                            }
                        }
                    }
                    if let Some(adjust) = adjust {
                        final_movement += adjust;
                    } else {
                        self.player_chunk = new_chunk;
                        self.player_position = new_position;
                        break;
                    }
                }

                let reply = MovementCommandReply {
                    player_chunk: self.player_chunk.index().to_array(),
                    position: self.player_position.to_array(),
                };
                worker.send_message(WorkerId::Parent, {
                    let command_bytes = bytemuck::bytes_of(&reply);
                    let mut message_bytes = [0u8; mem::size_of::<MovementCommandReply>() + 1];
                    message_bytes[0..mem::size_of::<MovementCommandReply>()]
                        .copy_from_slice(command_bytes);
                    *message_bytes.last_mut().unwrap() = MessageTag::MovementCommandReply as u8;
                    Box::<[u8]>::from(message_bytes)
                });

                return Some(Duration::ZERO);
            }
            Some(t) => unreachable!("Unknown message {t:?}"),
            None => {}
        }

        self.world.generate_around(self.player_chunk);
        self.send_commands_to_workers(worker);

        if self
            .player_chunk
            .index()
            .distance_squared(self.last_world_cropping_player_chunk.index())
            >= 4
        {
            self.last_world_cropping_player_chunk = self.player_chunk;
            let deleted = self.world.crop(self.player_chunk);
            if deleted.len() > 0 {
                let total_size = deleted.len() * mem::size_of::<[i32; 3]>();

                let mut message = Vec::with_capacity(total_size + 1);
                for position in deleted {
                    message.extend_from_slice(bytemuck::bytes_of(position.index().as_ref()));
                }
                debug_assert_eq!(message.len(), total_size);
                message.push(MessageTag::ChunkRemoval as u8);

                let message = message.into_boxed_slice();
                worker.send_message(WorkerId::Parent, message);
            }
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
