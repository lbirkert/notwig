#![cfg(target_arch = "wasm32")]
#![forbid(unsafe_code)]

use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

#[wasm_bindgen]
pub struct WebApp {
    runner: eframe::WebRunner,
}

impl Default for WebApp {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl WebApp {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        eframe::WebLogger::init(log::LevelFilter::Info).ok();
        Self {
            runner: eframe::WebRunner::new(),
        }
    }

    #[wasm_bindgen]
    pub async fn start(&self, canvas: HtmlCanvasElement) -> Result<(), JsValue> {
        self.runner
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(notwig_app::web::create_app(cc)))),
            )
            .await
    }

    #[wasm_bindgen]
    pub fn enable_audio(&self) {
        if let Some(mut app) = self.runner.app_mut::<notwig_app::gui::NotwigApp>() {
            app.enable_audio();
        }
    }

    #[wasm_bindgen]
    pub fn has_panicked(&self) -> bool {
        self.runner.has_panicked()
    }

    #[wasm_bindgen]
    pub fn panic_message(&self) -> Option<String> {
        self.runner.panic_summary().map(|summary| summary.message())
    }
}
