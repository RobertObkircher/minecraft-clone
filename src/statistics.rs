use std::{io, mem};
use std::time::{Duration, Instant};

use glam::Vec3;

/// TODO bound memory usage? if we collect 100 bytes per frame at 60 fps that is only 21.6MB per hour. An easy optimization would be to replace 12 byte Durations with 4 byte f32
pub struct Statistics {
    pub frame_infos: Vec<FrameInfo>,
    pub chunk_infos: Vec<ChunkInfo>,
    pub chunk_mesh_infos: Vec<ChunkMeshInfo>,
    pub total_chunk_time: Duration,
    pub total_chunk_mesh_time: Duration,
    pub full_invisible_chunks: usize,
}

pub struct FrameInfo {
    pub player_position: Vec3,
    pub player_orientation: Vec3,
    pub frame_time: Duration,
    pub chunk_info_count: usize,
    pub chunk_mesh_info_count: usize,
}

pub struct ChunkInfo {
    pub non_air_block_count: u16,
    pub time: Duration,
}

pub struct ChunkMeshInfo {
    pub time: Duration,
    pub face_count: usize,
}

impl Statistics {
    pub const fn new() -> Self {
        Self {
            frame_infos: vec![],
            chunk_infos: vec![],
            chunk_mesh_infos: vec![],
            total_chunk_time: Duration::ZERO,
            total_chunk_mesh_time: Duration::ZERO,
            full_invisible_chunks: 0,
        }
    }

    pub fn chunk_generated(&mut self, info: ChunkInfo) {
        self.total_chunk_time += info.time;
        self.chunk_infos.push(info);
    }

    pub fn chunk_mesh_generated(&mut self, info: ChunkMeshInfo) {
        self.total_chunk_mesh_time += info.time;
        self.chunk_mesh_infos.push(info);
    }

    pub fn end_frame(&mut self, info: FrameInfo) {
        self.frame_infos.push(info);
    }

    pub fn print_last_frame(&self, w: &mut dyn io::Write) -> io::Result<()> {
        let start = Instant::now();
        let frame = self.frame_infos.last().unwrap();

        writeln!(w)?;
        writeln!(w, "Frame: {}", self.frame_infos.len())?;

        let last_10: Duration = self.frame_infos.iter().rev().take(10).map(|it| it.frame_time).sum();
        writeln!(w, "    current: {:4}ms = {:6.2}f/s",
                 frame.frame_time.as_millis(),
                 1.0 / frame.frame_time.as_secs_f64(),
        )?;
        writeln!(w, "    last 10: {:4}ms = {:6.2}f/s",
                 last_10.as_millis(),
                 10.0 / last_10.as_secs_f64(),
        )?;

        let p = frame.player_position;
        let o = frame.player_orientation;
        writeln!(w, "Player: at ({:6.1}, {:6.1}, {:6.1}) facing ({:6.3}, {:6.3}, {:6.3})", p.x, p.y, p.z, o.x, o.y, o.z)?;

        let (prev_cic, prev_cmic) = self.frame_infos.iter().nth_back(1).map(|it| (it.chunk_info_count, it.chunk_mesh_info_count)).unwrap_or((0, 0));

        writeln!(w, "Chunks:")?;
        {
            writeln!(w, "    total: {:4} generated, {:6.2}ms total, {:6.2}ms average",
                     self.chunk_infos.len(),
                     1000.0 * self.total_chunk_time.as_secs_f64(),
                     1000.0 * self.total_chunk_time.as_secs_f64() / self.chunk_infos.len() as f64,
            )?;
            let chunk_infos = &self.chunk_infos[prev_cic..frame.chunk_info_count];
            let chunk_infos_duration: f64 = chunk_infos.iter().map(|it| it.time.as_secs_f64()).sum();
            if chunk_infos.is_empty() {
                writeln!(w, "    frame: 0 generated")?;
            } else {
                writeln!(w, "    frame: {:4} generated, {:6.2}ms total, {:6.2}ms average",
                         chunk_infos.len(),
                         1000.0 * chunk_infos_duration,
                         1000.0 * chunk_infos_duration / chunk_infos.len() as f64,
                )?;
            }
        }

        writeln!(w, "Chunk meshes:")?;
        {
            writeln!(w, "    total: {:4} generated, {:6.2}ms total, {:6.2}ms average",
                     self.chunk_mesh_infos.len(),
                     1000.0 * self.total_chunk_mesh_time.as_secs_f64(),
                     1000.0 * self.total_chunk_mesh_time.as_secs_f64() / self.chunk_mesh_infos.len() as f64,
            )?;
            let chunk_mesh_infos = &self.chunk_mesh_infos[prev_cmic..frame.chunk_mesh_info_count];
            let chunk_mesh_infos_duration: f64 = chunk_mesh_infos.iter().map(|it| it.time.as_secs_f64()).sum();
            if chunk_mesh_infos.is_empty() {
                writeln!(w, "    frame: 0 generated")?;
            } else {
                writeln!(w, "    frame: {:4} generated, {:6.2}ms total, {:6.2}ms average",
                         chunk_mesh_infos.len(),
                         1000.0 * chunk_mesh_infos_duration,
                         1000.0 * chunk_mesh_infos_duration / chunk_mesh_infos.len() as f64,
                )?;
            }
            writeln!(w, "    full but invisible: {}", self.full_invisible_chunks)?;
        }

        let size = mem::size_of::<Statistics>() +
            mem::size_of_val(self.frame_infos.as_slice()) +
            mem::size_of_val(self.chunk_infos.as_slice()) +
            mem::size_of_val(self.chunk_mesh_infos.as_slice());
        writeln!(w, "Statistics: {:.2}ms printing time, {:.3}kB total size",
                 start.elapsed().as_secs_f64() * 1000.0,
                 size as f64 / 1000.0
        )
    }
}