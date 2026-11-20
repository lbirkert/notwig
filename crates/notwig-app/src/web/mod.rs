pub(crate) mod storage;

use crate::gui::NotwigApp;

pub fn create_app(cc: &eframe::CreationContext<'_>) -> NotwigApp {
    crate::logging::init();
    let mut app = NotwigApp::new_web();
    app.configure(&cc.egui_ctx);
    app
}
