#[cfg(not(target_arch = "wasm32"))]
pub mod thread_worker;
#[cfg(target_arch = "wasm32")]
pub mod web_worker;

use std::cell::RefCell;
use std::mem;
use std::num::{NonZeroU32, NonZeroUsize};
use std::time::Duration;

use bytemuck::{AnyBitPattern, Contiguous};

use crate::{GeneratorState, RendererState, SimulationState};

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

thread_local! {static WORKER_STATE: RefCell<Option<State>> = RefCell::new(None); }

pub fn set_renderer_state(s: RendererState) {
    WORKER_STATE.with_borrow_mut(|state| {
        assert!(state.is_none());
        *state = Some(State::Renderer(s));
    })
}

pub fn with_renderer_state<F: FnOnce(&mut RendererState)>(f: F) {
    WORKER_STATE.with_borrow_mut(|s| match s {
        Some(State::Renderer(s)) => f(s),
        _ => unreachable!(),
    });
}

enum State {
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
}

pub fn update(worker: &mut impl Worker, message: Option<WorkerMessage>) -> Option<Duration> {
    WORKER_STATE.with_borrow_mut(|state| {
        // state changing messages:
        let tag = message.as_ref().map(WorkerMessage::tag);
        if tag == Some(MessageTag::InitSimulation) {
            let message = message.unwrap();
            let (s, result) = SimulationState::initialize(worker, message);
            *state = Some(State::Simulation(s));
            return result;
        }
        if tag == Some(MessageTag::InitGenerator) {
            let message = message.unwrap();
            let s = GeneratorState::initialize(worker, message);
            *state = Some(State::Generator(s));
            return None;
        }

        // non-state changing messages:
        match state.as_mut().unwrap() {
            State::Renderer(s) => {
                s.update(worker, message);
                None
            }
            State::Simulation(s) => s.update(worker, message),
            State::Generator(s) => s.update(worker, message),
        }
    })
}
