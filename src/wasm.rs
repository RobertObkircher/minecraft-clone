use crate::statistics::Statistics;
use crate::worker::web_worker::WebWorker;
use crate::worker::{State, WorkerId, WorkerMessage};
use crate::RendererState;
use log::{Level, Log, Metadata, Record};
use std::cell::RefCell;
use std::num::NonZeroU32;
use std::panic::PanicInfo;
use wasm_bindgen::prelude::*;
use web_sys::console;
use web_sys::Element;
use winit::event_loop::EventLoop;
use winit::platform::web::{EventLoopExtWebSys, WindowExtWebSys};
use winit::window::Window;

#[wasm_bindgen(start)]
pub fn wasm_start() {
    std::panic::set_hook(Box::new(panic_hook));
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(log::Level::Info.to_level_filter());
}

// This could also be a `static mut`
thread_local! {static STATE: RefCell<Option<State<'static>>> = const { RefCell::new(None) } }

#[wasm_bindgen]
pub async fn wasm_renderer() {
    let event_loop = EventLoop::new().unwrap();

    let winit_window = Window::new(&event_loop).unwrap();

    {
        let document = web_sys::window().unwrap().document().unwrap();
        let body = document.body().unwrap();

        let canvas = winit_window.canvas().unwrap();
        let canvas = Element::from(canvas);
        body.append_child(&canvas).unwrap();
    }

    let winit_window = Box::leak(Box::new(winit_window));

    let state = RendererState::new(winit_window, &mut WebWorker).await;
    STATE.with_borrow_mut(|s| *s = Some(State::Renderer(state)));

    // This only registers callbacks and returns immediately,
    // because we must not block the javascript event loop.
    // If we called run instead then winit would force the immediate
    // return with a javascript exception.
    event_loop.spawn(|event, target| {
        STATE.with_borrow_mut(|state| match state {
            Some(State::Renderer(state)) => state.process_event(event, target, &WebWorker),
            _ => unreachable!(),
        });
    });
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

fn panic_hook(info: &PanicInfo) {
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
