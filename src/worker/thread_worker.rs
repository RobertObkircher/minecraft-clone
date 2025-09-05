use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, RecvError, RecvTimeoutError, Sender};
use std::thread;

use crate::worker;
use crate::worker::{Worker, WorkerId, WorkerMessage};

fn run_thread(mut w: ThreadWorker) {
    let mut state = None;
    let mut timeout = None;
    loop {
        let message = if let Some(timeout) = timeout {
            match w.incoming.recv_timeout(timeout) {
                Ok(message) => Some(message),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        } else {
            match w.incoming.recv() {
                Ok(message) => Some(message),
                Err(RecvError {}) => return,
            }
        };
        timeout = worker::update(&mut w, &mut state, message);
    }
}

pub struct ThreadWorker {
    for_others: Sender<WorkerMessage>,
    pub incoming: Receiver<WorkerMessage>,
    parent: Option<(NonZeroU32, Sender<WorkerMessage>)>,
    children: Vec<Sender<WorkerMessage>>,
}

impl ThreadWorker {
    pub fn new(parent: Option<(NonZeroU32, Sender<WorkerMessage>)>) -> Self {
        let (my_sender, my_receiver) = mpsc::channel();

        Self {
            for_others: my_sender,
            incoming: my_receiver,
            parent,
            children: vec![],
        }
    }

    fn spawn_child_worker<F>(&mut self, f: F) -> WorkerId
    where
        F: Send + 'static + FnOnce(ThreadWorker),
    {
        let id = NonZeroU32::try_from(self.children.len() as u32 + 1).unwrap();
        let to_parent = self.for_others.clone();
        let worker = ThreadWorker::new(Some((id, to_parent)));

        self.children.push(worker.for_others.clone());

        thread::spawn(|| f(worker));
        WorkerId::Child(id)
    }
}

impl Worker for ThreadWorker {
    fn spawn_child(&mut self) -> WorkerId {
        self.spawn_child_worker(run_thread)
    }
    fn send_message(&self, receiver: WorkerId, message: Box<[u8]>) {
        match receiver {
            WorkerId::Parent => {
                let p = self.parent.as_ref().unwrap();
                p.1.send(WorkerMessage {
                    sender: WorkerId::Child(p.0),
                    bytes: message,
                })
                .unwrap();
            }
            WorkerId::Child(c) => {
                self.children[c.get() as usize - 1]
                    .send(WorkerMessage {
                        sender: WorkerId::Parent,
                        bytes: message,
                    })
                    .unwrap();
            }
        }
    }

    fn available_parallelism() -> NonZeroUsize {
        thread::available_parallelism().unwrap_or_else(|e| {
            log::warn!("Could not determine available parallelism. Defaulting to 4. {e}");
            NonZeroUsize::try_from(4usize).unwrap()
        })
    }
}
