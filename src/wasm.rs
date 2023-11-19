use crate::statistics::Statistics;
use crate::worker::web_worker::WebWorker;
use crate::worker::{WorkerId, WorkerMessage};
use log::{Level, Log, Metadata, Record};
use std::num::NonZeroU32;
use std::panic::PanicInfo;
use std::time::Duration;
use wasm_bindgen::prelude::*;
use web_sys::console;
use web_sys::Element;
use winit::platform::web::WindowExtWebSys;
use winit::window::Window;

#[wasm_bindgen(start)]
pub fn wasm_start() {
    std::panic::set_hook(Box::new(panic_hook));
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(log::Level::Info.to_level_filter());
}

#[wasm_bindgen]
pub async fn wasm_renderer() {
    let mut worker = WebWorker;
    crate::renderer(&mut worker).await;
}

#[wasm_bindgen]
pub fn wasm_update() -> i32 {
    let mut worker = WebWorker;
    to_millis(crate::worker::update(&mut worker, None))
}

#[wasm_bindgen]
pub fn wasm_update_with_message(id: u32, message: Box<[u8]>) -> i32 {
    let mut worker = WebWorker;
    to_millis(crate::worker::update(
        &mut worker,
        Some(WorkerMessage {
            sender: NonZeroU32::try_from(id)
                .map(WorkerId::Child)
                .unwrap_or(WorkerId::Parent),
            bytes: message,
        }),
    ))
}

fn to_millis(duration: Option<Duration>) -> i32 {
    duration
        .map(|it| i32::try_from(it.as_millis()).unwrap())
        .unwrap_or(-1)
}

pub fn setup_window(winit_window: &Window) {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let body = document.body().unwrap();

    let canvas = winit_window.canvas().unwrap();
    let canvas = Element::from(canvas);
    body.append_child(&canvas).unwrap();
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
