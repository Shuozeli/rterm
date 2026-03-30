#[allow(unused_imports, dead_code, clippy::all, non_snake_case)]
mod generated;

mod app;
mod connection;
mod input;
mod messages;
mod protocol;
mod render;
mod scroll;
mod selection;
mod session;
mod transport;

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("no window")
            .document()
            .expect("no document");
        let canvas: web_sys::HtmlCanvasElement = document
            .get_element_by_id("rterm-canvas")
            .expect("no canvas")
            .unchecked_into();

        eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(app::TerminalApp::new(cc)))),
            )
            .await
            .expect("failed to start eframe");
    });
}
