//! Pure-Rust VST3 ARA companion interfaces.
//!
//! `ARAVST3.h` defines two COM interfaces that a VST3 audio processor can
//! implement: `ARA::IPlugInEntryPoint` (ARA 1) and
//! `ARA::IPlugInEntryPoint2` (ARA 2 role-aware binding). They are tiny
//! `FUnknown` extensions, so notwig-rack declares them locally instead of
//! requiring the Steinberg C++ SDK for an ARA build.

use notwig_rack_ara::{
    ARADocumentControllerRef, ARAFactory, ARAPlugInExtensionInstance, ARAPlugInInstanceRoleFlags,
};
use vst3::Steinberg::Vst::IComponent;
use vst3::Steinberg::{FUnknown, FUnknownVtbl, TUID, kResultOk};
use vst3::com_scrape_types::{Guid, Inherits, Interface, Unknown};
use vst3::{ComPtr, uid};

const fn tuid_to_guid(tuid: TUID) -> Guid {
    let mut guid = [0u8; 16];
    let mut index = 0usize;
    while index < 16 {
        guid[index] = tuid[index].cast_unsigned();
        index += 1;
    }
    guid
}

const I_PLUGIN_ENTRY_POINT_IID: Guid =
    tuid_to_guid(uid(0x1281_4E54, 0xA1CE_4076, 0x82B9_6813, 0x1695_0BD6));
const I_PLUGIN_ENTRY_POINT2_IID: Guid =
    tuid_to_guid(uid(0xCD9A_5913, 0xC9EB_46D7, 0x96CA_53AD, 0xD1DB_89F5));

macro_rules! funknown_impl {
    ($interface:ty, $vtbl:ty, $iid:expr) => {
        unsafe impl Send for $interface {}
        unsafe impl Sync for $interface {}
        unsafe impl Inherits<FUnknown> for $interface {}

        impl Unknown for $interface {
            unsafe fn query_interface(
                this: *mut Self,
                iid: &Guid,
            ) -> Option<*mut std::ffi::c_void> {
                let mut object = std::ptr::null_mut();
                let result = unsafe {
                    ((*(*this).vtbl).base.queryInterface)(
                        this.cast::<FUnknown>(),
                        iid.as_ptr().cast::<TUID>(),
                        &raw mut object,
                    )
                };
                if result == kResultOk {
                    Some(object)
                } else {
                    None
                }
            }

            unsafe fn add_ref(this: *mut Self) -> usize {
                unsafe { ((*(*this).vtbl).base.addRef)(this.cast::<FUnknown>()) as usize }
            }

            unsafe fn release(this: *mut Self) -> usize {
                unsafe { ((*(*this).vtbl).base.release)(this.cast::<FUnknown>()) as usize }
            }
        }

        unsafe impl Interface for $interface {
            type Vtbl = $vtbl;
            const IID: Guid = $iid;

            fn inherits(iid: &Guid) -> bool {
                iid == &Self::IID || FUnknown::inherits(iid)
            }
        }
    };
}

/// ARA 1 entry point.
#[repr(C)]
pub struct IPlugInEntryPoint {
    vtbl: *const IPlugInEntryPointVtbl,
}

/// ARA 1 entry-point vtable.
#[repr(C)]
pub struct IPlugInEntryPointVtbl {
    base: FUnknownVtbl,
    get_factory: unsafe extern "system" fn(this: *mut IPlugInEntryPoint) -> *const ARAFactory,
    bind_to_document_controller: unsafe extern "system" fn(
        this: *mut IPlugInEntryPoint,
        controller: ARADocumentControllerRef,
    )
        -> *const ARAPlugInExtensionInstance,
}

funknown_impl!(
    IPlugInEntryPoint,
    IPlugInEntryPointVtbl,
    I_PLUGIN_ENTRY_POINT_IID
);

/// ARA 2 role-aware entry point.
#[repr(C)]
pub struct IPlugInEntryPoint2 {
    vtbl: *const IPlugInEntryPoint2Vtbl,
}

/// ARA 2 entry-point vtable.
#[repr(C)]
pub struct IPlugInEntryPoint2Vtbl {
    base: FUnknownVtbl,
    get_factory: unsafe extern "system" fn(this: *mut IPlugInEntryPoint2) -> *const ARAFactory,
    bind_to_document_controller_with_roles:
        unsafe extern "system" fn(
            this: *mut IPlugInEntryPoint2,
            controller: ARADocumentControllerRef,
            known_roles: ARAPlugInInstanceRoleFlags,
            assigned_roles: ARAPlugInInstanceRoleFlags,
        ) -> *const ARAPlugInExtensionInstance,
}

funknown_impl!(
    IPlugInEntryPoint2,
    IPlugInEntryPoint2Vtbl,
    I_PLUGIN_ENTRY_POINT2_IID
);

fn entry_point2(component: &ComPtr<IComponent>) -> Option<ComPtr<IPlugInEntryPoint2>> {
    component.as_com_ref().cast::<IPlugInEntryPoint2>()
}

fn entry_point1(component: &ComPtr<IComponent>) -> Option<ComPtr<IPlugInEntryPoint>> {
    component.as_com_ref().cast::<IPlugInEntryPoint>()
}

/// Returns the ARA factory exposed by a component, preferring ARA 2.
pub(crate) fn factory(component: &ComPtr<IComponent>) -> Option<*const ARAFactory> {
    if let Some(entry) = entry_point2(component) {
        let raw = unsafe { factory2(entry.as_ptr()) };
        if !raw.is_null() {
            return Some(raw);
        }
    }
    let entry = entry_point1(component)?;
    let raw = unsafe { factory1(entry.as_ptr()) };
    (!raw.is_null()).then_some(raw)
}

unsafe fn factory1(entry: *mut IPlugInEntryPoint) -> *const ARAFactory {
    unsafe { ((*(*entry).vtbl).get_factory)(entry) }
}

unsafe fn factory2(entry: *mut IPlugInEntryPoint2) -> *const ARAFactory {
    unsafe { ((*(*entry).vtbl).get_factory)(entry) }
}

/// Binds a component to `controller` with the supplied roles.
///
/// # Errors
///
/// Returns an ARA error when the processor exposes no ARA entry point or the
/// companion returns a null extension.
pub(crate) fn bind(
    component: &ComPtr<IComponent>,
    controller: ARADocumentControllerRef,
    known: notwig_rack_ara::AraRoles,
    assigned: notwig_rack_ara::AraRoles,
) -> Result<notwig_rack_ara::AraPluginExtension, notwig_rack_ara::AraError> {
    if let Some(entry) = entry_point2(component) {
        let raw = unsafe {
            ((*(*entry.as_ptr()).vtbl).bind_to_document_controller_with_roles)(
                entry.as_ptr(),
                controller,
                known.bits(),
                assigned.bits(),
            )
        };
        return unsafe { notwig_rack_ara::AraPluginExtension::from_raw(raw) };
    }
    if let Some(entry) = entry_point1(component) {
        let raw = unsafe {
            ((*(*entry.as_ptr()).vtbl).bind_to_document_controller)(entry.as_ptr(), controller)
        };
        return unsafe { notwig_rack_ara::AraPluginExtension::from_raw(raw) };
    }
    Err(notwig_rack_ara::AraError::Unsupported(
        "VST3 component has no ARA entry point",
    ))
}

#[cfg(test)]
mod tests {
    use super::{IPlugInEntryPoint, IPlugInEntryPoint2};
    use vst3::com_scrape_types::Interface;

    #[test]
    fn ara_entry_point_iids_are_distinct() {
        assert_ne!(IPlugInEntryPoint::IID, IPlugInEntryPoint2::IID);
        assert_ne!(IPlugInEntryPoint::IID, [0u8; 16]);
        assert_ne!(IPlugInEntryPoint2::IID, [0u8; 16]);
    }
}
