use crate::worker::{Worker, WorkerId};
use std::num::{NonZeroU32, NonZeroUsize};
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(thread_local_v2, js_namespace = ["self", "navigator"], js_name = "hardwareConcurrency")]
    static HARDWARE_CONCURRENCY: u32;
    fn spawn_worker() -> u32;
    fn post_message(receiver: u32, message: Box<[u8]>);
}

pub struct WebWorker;

impl Worker for WebWorker {
    fn spawn_child(&mut self) -> WorkerId {
        let id = spawn_worker();
        WorkerId::Child(NonZeroU32::try_from(id).unwrap())
    }
    fn send_message(&self, receiver: WorkerId, message: Box<[u8]>) {
        match receiver {
            WorkerId::Parent => {
                post_message(0, message);
            }
            WorkerId::Child(c) => {
                post_message(c.get(), message);
            }
        }
    }

    fn available_parallelism() -> NonZeroUsize {
        NonZeroUsize::try_from(HARDWARE_CONCURRENCY.with(|x| *x as usize)).unwrap()
    }
}
