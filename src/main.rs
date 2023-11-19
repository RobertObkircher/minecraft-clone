use minecraft_clone::worker::thread_worker::ThreadWorker;
use std::process::exit;

fn main() {
    env_logger::init();

    let mut worker = ThreadWorker::new(None);
    pollster::block_on(minecraft_clone::renderer(&mut worker));

    // There are no RecvErrors because both parents and children have references to Senders
    exit(0);
}
