use minecraft_clone::RendererState;
use minecraft_clone::worker::thread_worker::ThreadWorker;
use std::process::exit;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, DeviceId, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::Window;

fn main() {
    env_logger::init();

    let event_loop = EventLoop::with_user_event().build().unwrap();
    let mut app = MainApp {
        thread_worker: ThreadWorker::new(None),
        state: None,
    };
    event_loop.run_app(&mut app).unwrap();

    // There are no RecvErrors because both parents and children have references to Senders
    exit(0);
}

pub struct MainApp {
    thread_worker: ThreadWorker,
    state: Option<RendererState>,
}

impl ApplicationHandler<RendererState> for MainApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let mut window_attributes = Window::default_attributes();
        window_attributes.title = "Hello, world!".to_string();
        window_attributes.inner_size = Some(LogicalSize::new(800.0, 600.0).into());
        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());
        self.state = Some(pollster::block_on(RendererState::new(
            window,
            &mut self.thread_worker,
            false,
        )));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = &mut self.state else { return };
        // TODO move this somewhere else?
        while let Ok(message) = self.thread_worker.incoming.try_recv() {
            state.update(&mut self.thread_worker, Some(message));
        }
        state.window_event(event_loop, window_id, event, &self.thread_worker);
    }

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        device_id: DeviceId,
        event: DeviceEvent,
    ) {
        let Some(state) = &mut self.state else { return };
        state.device_event(event_loop, device_id, event);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        // TODO shutdown workers
        self.state = None;
    }
}
