//! VST 2 editor window lifecycle and size handling.

use crate::abi::{self, Rect};
use crate::plugin::Vst2Plugin;
use notwig_rack_core::editor::{PluginEditor, WindowHandle};
use notwig_rack_core::error::{Error, Result};
use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::Ordering;

impl Vst2Plugin {
    pub(crate) fn query_editor_size(&self) -> Option<(u32, u32)> {
        let mut rect: *mut Rect = ptr::null_mut();
        self.dispatch(abi::EFF_EDIT_GET_RECT, 0, 0, (&raw mut rect).cast(), 0.0);
        if rect.is_null() {
            return None;
        }
        rect_size(unsafe { rect.read() })
    }
}

pub(crate) fn rect_size(rect: Rect) -> Option<(u32, u32)> {
    let width = i32::from(rect.right).saturating_sub(i32::from(rect.left));
    let height = i32::from(rect.bottom).saturating_sub(i32::from(rect.top));
    if width <= 0 || height <= 0 {
        return None;
    }
    Some((u32::try_from(width).ok()?, u32::try_from(height).ok()?))
}

impl PluginEditor for Vst2Plugin {
    fn open(&mut self, parent: WindowHandle, _scale: f64) -> Result<()> {
        if self.editor_open {
            return Ok(());
        }
        let handle = match parent {
            WindowHandle::NSView(pointer) | WindowHandle::HWND(pointer) => pointer,
            WindowHandle::X11(id) => id as usize as *mut c_void,
        };
        let opened = self.dispatch(abi::EFF_EDIT_OPEN, 0, 0, handle, 0.0);
        let size = self.query_editor_size();
        if opened == 0 && size.is_none() {
            self.dispatch(abi::EFF_EDIT_CLOSE, 0, 0, ptr::null_mut(), 0.0);
            return Err(Error::Other("VST2 effEditOpen failed".into()));
        }
        self.editor_open = true;
        self.host.editor_open.store(true, Ordering::Release);
        self.editor_size.set(size);
        Ok(())
    }

    fn close(&mut self) {
        if !self.editor_open {
            return;
        }
        self.dispatch(abi::EFF_EDIT_CLOSE, 0, 0, ptr::null_mut(), 0.0);
        self.editor_open = false;
        self.host.editor_open.store(false, Ordering::Release);
        self.host.editor_size.store(0, Ordering::Release);
        self.editor_size.set(None);
    }

    fn is_open(&self) -> bool {
        self.editor_open
    }

    fn size(&self) -> Option<(u32, u32)> {
        if let Some(size) = self.host.take_editor_size() {
            self.editor_size.set(Some(size));
        }
        if let Some(size) = self.editor_size.get() {
            return Some(size);
        }
        let size = self.query_editor_size();
        self.editor_size.set(size);
        size
    }

    fn is_resizable(&self) -> bool {
        false
    }

    fn set_size(&mut self, _width: u32, _height: u32) -> Option<(u32, u32)> {
        // VST 2 has no standard host-driven resize request.
        None
    }

    fn show(&mut self) {}

    fn hide(&mut self) {}

    fn on_idle(&mut self) {
        if !self.editor_open {
            return;
        }
        self.dispatch(abi::EFF_EDIT_IDLE, 0, 0, ptr::null_mut(), 0.0);
        if let Some(size) = self.host.take_editor_size() {
            self.editor_size.set(Some(size));
        }
    }
}
