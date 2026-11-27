use notwig_core::Clipboard;
use notwig_ext_api::ClipboardProvider;

/// Default clipboard provider owned by `notwig.core`.
#[derive(Debug, Default)]
pub(crate) struct CoreClipboardProvider {
    data: Clipboard,
    text: Option<String>,
}

impl ClipboardProvider for CoreClipboardProvider {
    fn get(&self) -> Clipboard {
        self.data.clone()
    }

    fn set(&mut self, clipboard: Clipboard) {
        self.data = clipboard;
    }

    fn get_text(&self) -> Option<String> {
        self.text.clone()
    }

    fn set_text(&mut self, text: Option<String>) {
        self.text = text;
    }
}
