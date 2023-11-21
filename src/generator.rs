use std::mem::size_of;
use std::time::Duration;

use bytemuck::{Pod, Zeroable};
use glam::IVec3;

use terrain::TerrainGenerator;

use crate::simulation::chunk::Chunk;
use crate::simulation::position::ChunkPosition;
use crate::worker::{MessageTag, Worker, WorkerId, WorkerMessage};

mod noise;
pub mod terrain;

pub struct GeneratorState {
    generator: TerrainGenerator,
    highest_generated_chunk: i32,
    lowest_generated_chunk: i32,
}

#[repr(C)]
#[derive(Zeroable, Pod, Copy, Clone)]
pub struct ChunkColumnElement {
    pub is_some: u8,
    pub transparency: u8,
    pub non_air_block_count: u16,
    pub blocks: [[[u8; Chunk::SIZE]; Chunk::SIZE]; Chunk::SIZE],
}

#[repr(C)]
#[derive(Zeroable, Pod, Copy, Clone)]
pub struct ChunkInfoBytes {
    pub time_secs: u64,
    pub time_subsec_nanos: u32,
    pub non_air_block_count: u16,
    padding: u16,
}

impl GeneratorState {
    pub fn initialize<W: Worker>(_worker: &mut W, message: WorkerMessage) -> Self {
        let seed = *bytemuck::from_bytes(&message.bytes[0..8]);
        let highest_generated_chunk = *bytemuck::from_bytes(&message.bytes[8..12]);
        let lowest_generated_chunk = *bytemuck::from_bytes(&message.bytes[12..16]);

        GeneratorState {
            generator: TerrainGenerator::new(seed),
            highest_generated_chunk,
            lowest_generated_chunk,
        }
    }
    pub fn update(
        &mut self,
        worker: &impl Worker,
        message: Option<WorkerMessage>,
    ) -> Option<Duration> {
        let tag = message.as_ref().map(WorkerMessage::tag);

        if tag == Some(MessageTag::GenerateColumn) {
            let message = message.unwrap();
            let x = *bytemuck::from_bytes::<i32>(&message.bytes[0..4]);
            let z = *bytemuck::from_bytes::<i32>(&message.bytes[4..8]);

            let count = (self.highest_generated_chunk - self.lowest_generated_chunk) as usize + 1;

            let mut message =
                Vec::<u8>::with_capacity(8 + count * size_of::<ChunkColumnElement>() + 1);

            message.extend_from_slice(bytemuck::bytes_of(&x));
            message.extend_from_slice(bytemuck::bytes_of(&z));

            let mut info_message =
                Vec::<u8>::with_capacity(count * size_of::<ChunkInfoBytes>() + 1);

            for y in self.lowest_generated_chunk..=self.highest_generated_chunk {
                let (chunk, info) = self
                    .generator
                    .fill_chunk(ChunkPosition::from_chunk_index(IVec3::new(x, y, z)));

                info_message.extend_from_slice(bytemuck::bytes_of(&ChunkInfoBytes {
                    time_secs: info.time.as_secs(),
                    time_subsec_nanos: info.time.subsec_nanos(),
                    non_air_block_count: info.non_air_block_count,
                    padding: 0,
                }));

                if let Some(chunk) = chunk {
                    message.extend_from_slice(bytemuck::bytes_of(&ChunkColumnElement {
                        is_some: 1,
                        transparency: chunk.transparency,
                        non_air_block_count: chunk.non_air_block_count,
                        blocks: chunk.blocks.map(|it| it.map(|it| it.map(|it| it as u8))),
                    }));
                } else {
                    message.push(0); // false
                    message.push(0); // alignment
                }
            }

            message.push(MessageTag::GenerateColumnReply as u8);
            let message = message.into_boxed_slice();
            worker.send_message(WorkerId::Parent, message);

            info_message.push(MessageTag::ChunkInfo as u8);
            worker.send_message(WorkerId::Parent, info_message.into_boxed_slice());
        }

        None
    }
}
