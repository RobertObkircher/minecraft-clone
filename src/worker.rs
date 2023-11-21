#[cfg(not(target_arch = "wasm32"))]
pub mod thread_worker;
#[cfg(target_arch = "wasm32")]
pub mod web_worker;

use std::mem;
use std::num::{NonZeroU32, NonZeroUsize};
use std::time::Duration;

use bytemuck::{AnyBitPattern, Contiguous};

use crate::generator::GeneratorState;
use crate::renderer::RendererState;
use crate::simulation::SimulationState;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum WorkerId {
    Parent,
    Child(NonZeroU32),
}

pub trait Worker {
    fn spawn_child(&mut self) -> WorkerId;
    fn send_message(&self, receiver: WorkerId, message: Box<[u8]>);
    fn available_parallelism() -> NonZeroUsize;
}

pub enum State {
    Renderer(RendererState),
    Simulation(SimulationState),
    Generator(GeneratorState),
}

pub struct WorkerMessage {
    pub sender: WorkerId,
    pub bytes: Box<[u8]>,
}

impl WorkerMessage {
    pub fn tag(&self) -> MessageTag {
        Contiguous::from_integer(*self.bytes.last().unwrap()).unwrap()
    }

    pub fn take<'a, T: AnyBitPattern>(bytes: &mut &'a [u8]) -> Option<&'a T> {
        let n = mem::size_of::<T>();
        if bytes.len() < n {
            None
        } else {
            let result = bytes.split_at(n);
            *bytes = result.1;
            Some(bytemuck::from_bytes::<T>(result.0))
        }
    }
    pub fn take_slice<'a, T: AnyBitPattern>(bytes: &mut &'a [u8], count: usize) -> Option<&'a [T]> {
        let n = mem::size_of::<T>() * count;
        if bytes.len() < n {
            None
        } else {
            let result = bytes.split_at(n);
            *bytes = result.1;
            Some(bytemuck::cast_slice::<u8, T>(result.0))
        }
    }
}

#[repr(u8)]
#[derive(Contiguous, Copy, Clone, Debug, Eq, PartialEq)]
pub enum MessageTag {
    InitSimulation,
    InitGenerator,
    GenerateColumn,
    GenerateColumnReply,
    MeshData,
    ChunkInfo,
    PlayerCommand,
}

pub fn update(
    worker: &mut impl Worker,
    state: &mut Option<State>,
    message: Option<WorkerMessage>,
) -> Option<Duration> {
    match message.as_ref().map(WorkerMessage::tag) {
        Some(MessageTag::InitSimulation) => {
            assert!(state.is_none());
            let (s, result) = SimulationState::initialize(worker, message.unwrap());
            *state = Some(State::Simulation(s));
            result
        }
        Some(MessageTag::InitGenerator) => {
            assert!(state.is_none());
            let s = GeneratorState::initialize(worker, message.unwrap());
            *state = Some(State::Generator(s));
            None
        }
        // non-state changing messages:
        _ => match state.as_mut().expect("state must already be set") {
            State::Renderer(s) => {
                s.update(worker, message);
                None
            }
            State::Simulation(s) => s.update(worker, message),
            State::Generator(s) => s.update(worker, message),
        },
    }
}
