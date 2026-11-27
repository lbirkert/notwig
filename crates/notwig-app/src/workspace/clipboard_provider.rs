use std::sync::Mutex;

use notwig_core::Clipboard;
use notwig_ext_api::ClipboardProvider;

/// Default clipboard provider owned by `notwig.core`.
#[derive(Debug, Default)]
pub(crate) struct CoreClipboardProvider {
    data: Mutex<Clipboard>,
    text: Mutex<Option<String>>,
}

impl ClipboardProvider for CoreClipboardProvider {
    fn get(&self) -> Clipboard {
        self.data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn set(&self, clipboard: Clipboard) {
        *self
            .data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = clipboard;
    }

    fn get_text(&self) -> Option<String> {
        self.text
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn set_text(&self, text: Option<String>) {
        *self
            .text
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = text;
    }
}
