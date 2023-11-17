use crate::statistics::Statistics;
use log::{Level, Log, Metadata, Record};
use std::collections::VecDeque;
use std::panic::PanicInfo;
use std::sync::Mutex;
use wasm_bindgen::prelude::*;
use web_sys::console;
use web_sys::Element;
use winit::platform::web::WindowExtWebSys;
use winit::window::Window;

#[wasm_bindgen]
extern "C" {
    fn hardware_concurrency() -> u32;
    fn spawn_worker() -> u32;
    fn post_message(receiver: u32, message: Box<[u32]>);
}

#[wasm_bindgen(start)]
pub fn wasm_start() {
    std::panic::set_hook(Box::new(panic_hook));
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(log::Level::Info.to_level_filter());
}

#[wasm_bindgen]
pub async fn wasm_main() {
    for _ in 0..hardware_concurrency() {
        let id = spawn_worker();
        log::info!("Spawned {id}");
    }
    post_message(1, Box::new([1]));
    post_message(2, Box::new([1, 2]));
    post_message(3, Box::new([1, 2, 3]));

    crate::run().await;
}

#[wasm_bindgen]
pub fn wasm_worker() -> bool {
    post_message(0, Box::new([42]));
    false
}

static INCOMING: Mutex<VecDeque<(u32, Box<[u8]>)>> = Mutex::new(VecDeque::new());

#[wasm_bindgen]
pub fn wasm_onmessage(id: u32, message: Box<[u8]>) {
    log::info!("onmessage got: {id} {message:?}");
    INCOMING.lock().unwrap().push_back((id, message));
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
