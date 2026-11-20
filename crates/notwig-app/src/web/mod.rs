pub(crate) mod storage;

use js_sys::{Array, Function, Reflect};
use wasm_bindgen::{JsCast, JsValue};

use crate::gui::NotwigApp;

pub fn create_app(cc: &eframe::CreationContext<'_>) -> NotwigApp {
    crate::logging::init();
    let mut app = NotwigApp::new_web();
    app.configure(&cc.egui_ctx);
    app
}

pub(crate) fn take_drag_pointer() -> Option<egui::Pos2> {
    let window = web_sys::window()?;
    let function = Reflect::get(&window, &JsValue::from_str("__notwigTakeDragPos")).ok()?;
    let function: Function = function.dyn_into().ok()?;
    let value = function.call0(&window).ok()?;
    let array: Array = value.dyn_into().ok()?;
    let x = array.get(0).as_f64()? as f32;
    let y = array.get(1).as_f64()? as f32;
    Some(egui::pos2(x, y))
}
