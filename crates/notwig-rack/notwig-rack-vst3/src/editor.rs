//! VST3 `IPlugView` editor hosting.

use notwig_rack_core::error::Error;

use vst3::ComPtr;
use vst3::Steinberg::Vst::{IEditController, IEditControllerTrait, ViewType};
use vst3::Steinberg::{
    IPlugView, IPlugViewTrait, ViewRect, kPlatformTypeHWND, kPlatformTypeNSView,
    kPlatformTypeX11EmbedWindowID, kResultOk,
};

use crate::plugin::Vst3Plugin;

/// Call `IEditController::createView("editor")` and wrap the raw
/// pointer. Returns `None` when the plugin has no editor.
pub(crate) unsafe fn create_editor_view(
    controller: &ComPtr<IEditController>,
) -> Option<ComPtr<IPlugView>> {
    let raw = unsafe { controller.createView(ViewType::kEditor) };
    if raw.is_null() {
        return None;
    }
    unsafe { ComPtr::<IPlugView>::from_raw(raw) }
}

fn platform_type_for_handle(
    handle: notwig_rack_core::editor::WindowHandle,
) -> (*const i8, *mut std::ffi::c_void) {
    use notwig_rack_core::editor::WindowHandle;
    match handle {
        WindowHandle::NSView(p) => (kPlatformTypeNSView, p),
        WindowHandle::HWND(p) => (kPlatformTypeHWND, p),
        WindowHandle::X11(id) => (kPlatformTypeX11EmbedWindowID, id as *mut std::ffi::c_void),
    }
}

impl notwig_rack_core::editor::PluginEditor for Vst3Plugin {
    fn open(
        &mut self,
        parent: notwig_rack_core::editor::WindowHandle,
        _scale: f64,
    ) -> notwig_rack_core::error::Result<()> {
        if self.editor_open {
            return Ok(());
        }
        let view = unsafe { create_editor_view(&self.controller) }
            .ok_or_else(|| Error::Other("IEditController::createView returned NULL".into()))?;
        let (type_str, parent_ptr) = platform_type_for_handle(parent);
        if unsafe { view.isPlatformTypeSupported(type_str) } != kResultOk {
            return Err(Error::Other(
                "IPlugView::isPlatformTypeSupported returned false".into(),
            ));
        }
        if unsafe { view.attached(parent_ptr, type_str) } != kResultOk {
            return Err(Error::Other("IPlugView::attached returned non-OK".into()));
        }
        self.view = Some(view);
        self.editor_open = true;
        Ok(())
    }

    fn close(&mut self) {
        if let Some(view) = self.view.take() {
            unsafe { view.removed() };
        }
        self.editor_open = false;
    }

    fn is_open(&self) -> bool {
        self.editor_open
    }

    fn size(&self) -> Option<(u32, u32)> {
        let view = self.view.as_ref()?;
        let mut rect = ViewRect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if unsafe { view.getSize(&raw mut rect) } != kResultOk {
            return None;
        }
        let w = u32::try_from(rect.right - rect.left).ok()?;
        let h = u32::try_from(rect.bottom - rect.top).ok()?;
        Some((w, h))
    }

    fn is_resizable(&self) -> bool {
        let Some(view) = self.view.as_ref() else {
            return false;
        };
        unsafe { view.canResize() == kResultOk }
    }

    fn set_size(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        let view = self.view.as_ref()?;
        let mut rect = ViewRect {
            left: 0,
            top: 0,
            right: i32::try_from(width).ok()?,
            bottom: i32::try_from(height).ok()?,
        };
        // Plugins may snap to a constraint via checkSizeConstraint.
        let _ = unsafe { view.checkSizeConstraint(&raw mut rect) };
        if unsafe { view.onSize(&raw mut rect) } != kResultOk {
            return None;
        }
        let w = u32::try_from(rect.right - rect.left).ok()?;
        let h = u32::try_from(rect.bottom - rect.top).ok()?;
        Some((w, h))
    }

    fn show(&mut self) {
        // VST3 attaches once and stays visible — no show/hide
        // separate from attached/removed.
    }

    fn hide(&mut self) {
        // Same as show — VST3 has no distinct hide.
    }
}
