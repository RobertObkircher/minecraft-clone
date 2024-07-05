use minecraft_clone::worker::thread_worker::ThreadWorker;
use minecraft_clone::RendererState;
use std::process::exit;
use winit::dpi::LogicalSize;
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;

fn main() {
    env_logger::init();

    let mut worker = ThreadWorker::new(None);

    let event_loop = EventLoop::new().unwrap();

    let window = WindowBuilder::new()
        .with_title("Hello, world!")
        .with_inner_size(LogicalSize::new(800.0, 600.0))
        .build(&event_loop)
        .unwrap();

    let mut state = pollster::block_on(RendererState::new(&window, &mut worker, false));

    event_loop
        .run(|event, target| {
            while let Ok(message) = worker.incoming.try_recv() {
                state.update(&mut worker, Some(message));
            }
            state.process_event(event, target, &worker);
        })
        .unwrap();

    // There are no RecvErrors because both parents and children have references to Senders
    exit(0);
}
