use crate::RendererState;
use crate::statistics::Statistics;
use crate::worker::web_worker::WebWorker;
use crate::worker::{State, WorkerId, WorkerMessage};
use log::{Level, Log, Metadata, Record};
use std::cell::RefCell;
use std::num::NonZeroU32;
use std::panic::PanicHookInfo;
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use web_sys::Element;
use web_sys::console;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::platform::web::{EventLoopExtWebSys, WindowExtWebSys};
use winit::window::Window;

#[wasm_bindgen(start)]
pub fn wasm_start() {
    std::panic::set_hook(Box::new(panic_hook));
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(log::Level::Info.to_level_filter());
}

// This could also be a `static mut`
thread_local! {static STATE: RefCell<Option<State>> = const { RefCell::new(None) } }

fn if_renderer_state_mut(f: impl FnOnce(&mut RendererState)) {
    STATE.with_borrow_mut(|s| match s {
        Some(State::Renderer(s)) => f(s),
        None => {}
        _ => unreachable!("unexpected renderer state"),
    })
}

#[wasm_bindgen]
pub fn wasm_renderer(disable_webgpu: bool) {
    let event_loop = EventLoop::with_user_event().build().unwrap();
    let proxy = Some(event_loop.create_proxy());
    let app = WasmApp {
        proxy,
        disable_webgpu,
    };
    event_loop.spawn_app(app);
}

pub struct WasmApp {
    proxy: Option<EventLoopProxy<RendererState>>,
    disable_webgpu: bool,
}

impl ApplicationHandler<RendererState> for WasmApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(proxy) = self.proxy.take() {
            let winit_window = Arc::new(
                event_loop
                    .create_window(Window::default_attributes())
                    .unwrap(),
            );
            {
                let document = web_sys::window().unwrap().document().unwrap();
                let body = document.body().unwrap();

                let canvas = winit_window.canvas().unwrap();
                let canvas = Element::from(canvas);
                body.append_child(&canvas).unwrap();
            }
            let disable_webgpu = self.disable_webgpu;
            wasm_bindgen_futures::spawn_local(async move {
                assert!(
                    proxy
                        .send_event(
                            RendererState::new(winit_window, &mut WebWorker, disable_webgpu).await
                        )
                        .is_ok()
                )
            });
        }
    }

    #[allow(unused_mut)]
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, mut state: RendererState) {
        // This is where proxy.send_event() ends up
        state.window.request_redraw();
        // TODO resize?
        // state.resize(
        //     event.window.inner_size().width,
        //     event.window.inner_size().height,
        // );
        STATE.with_borrow_mut(|s| *s = Some(State::Renderer(state)));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        if_renderer_state_mut(|state| {
            state.window_event(event_loop, window_id, event, &WebWorker);
        })
    }

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if_renderer_state_mut(|state| {
            state.device_event(event_loop, device_id, event);
        })
    }
}

#[wasm_bindgen]
pub fn wasm_update() -> i32 {
    update(None)
}

#[wasm_bindgen]
pub fn wasm_update_with_message(id: u32, message: Box<[u8]>) -> i32 {
    update(Some(WorkerMessage {
        sender: NonZeroU32::try_from(id)
            .map(WorkerId::Child)
            .unwrap_or(WorkerId::Parent),
        bytes: message,
    }))
}

fn update(message: Option<WorkerMessage>) -> i32 {
    let duration =
        STATE.with_borrow_mut(|state| crate::worker::update(&mut WebWorker, state, message));
    duration
        .map(|it| i32::try_from(it.as_millis()).unwrap())
        .unwrap_or(-1)
}

fn set_statistics(text_content: Option<&str>) {
    let element = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .get_element_by_id("statistics")
        .unwrap();
    element.set_text_content(text_content);
}

pub fn display_statistics(statistics: &Statistics) {
    let mut string = Vec::<u8>::new();
    statistics.print_last_frame(&mut string).unwrap();
    set_statistics(Some(std::str::from_utf8(&string).unwrap()));
}

pub fn hide_statistics() {
    set_statistics(None);
}

fn panic_hook(info: &PanicHookInfo) {
    console::error_1(&info.to_string().into());
}

struct ConsoleLogger;

static LOGGER: ConsoleLogger = ConsoleLogger {};

impl Log for ConsoleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let log = match record.level() {
                Level::Error => console::error_1,
                Level::Warn => console::warn_1,
                Level::Info => console::info_1,
                Level::Debug => console::log_1,
                Level::Trace => console::debug_1,
            };
            log(&format!("{}", record.args()).into());
        }
    }

    fn flush(&self) {}
}
