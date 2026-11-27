//! Audio Unit v2 ARA companion properties.
//!
//! ARA for AU v2 is exposed through two private (`'AraF'` / `'AraE'`) global
//! properties. Both carry a magic token so a non-ARA property collision is
//! detected rather than misinterpreted.

use notwig_rack_ara::{
    ARADocumentControllerRef, ARAFactory, ARAPlugInExtensionInstance, ARAPlugInInstanceRoleFlags,
    AraError, AraPluginExtension, AraRoles,
};
use objc2_audio_toolbox::{AudioComponentInstance, AudioUnitGetProperty, kAudioUnitScope_Global};
use std::ffi::c_void;
use std::ptr::{self, NonNull};

const ARA_AUDIO_UNIT_MAGIC: u32 = u32::from_be_bytes(*b"Ara!");
const K_AUDIO_UNIT_PROPERTY_ARA_FACTORY: u32 = u32::from_be_bytes(*b"AraF");
const K_AUDIO_UNIT_PROPERTY_ARA_BINDING: u32 = u32::from_be_bytes(*b"AraE");

#[repr(C)]
struct AraAudioUnitFactory {
    in_out_magic_number: u32,
    out_factory: *const ARAFactory,
}

#[repr(C)]
struct AraAudioUnitPlugInExtensionBinding {
    in_out_magic_number: u32,
    in_document_controller_ref: ARADocumentControllerRef,
    out_plug_in_extension: *const ARAPlugInExtensionInstance,
    known_roles: ARAPlugInInstanceRoleFlags,
    assigned_roles: ARAPlugInInstanceRoleFlags,
}

/// Queries the ARA factory property from a live AU instance.
pub(crate) unsafe fn factory(unit: AudioComponentInstance) -> Option<*const ARAFactory> {
    let mut data = AraAudioUnitFactory {
        in_out_magic_number: ARA_AUDIO_UNIT_MAGIC,
        out_factory: ptr::null(),
    };
    let mut size = u32::try_from(std::mem::size_of::<AraAudioUnitFactory>()).ok()?;
    let status = unsafe {
        AudioUnitGetProperty(
            unit,
            K_AUDIO_UNIT_PROPERTY_ARA_FACTORY,
            kAudioUnitScope_Global,
            0,
            NonNull::from(&mut data).cast::<c_void>(),
            NonNull::from(&mut size),
        )
    };
    if status != 0
        || usize::try_from(size).ok()? != std::mem::size_of::<AraAudioUnitFactory>()
        || data.in_out_magic_number != ARA_AUDIO_UNIT_MAGIC
        || data.out_factory.is_null()
    {
        None
    } else {
        Some(data.out_factory)
    }
}

/// Binds a live AU instance to an ARA document controller.
pub(crate) unsafe fn bind(
    unit: AudioComponentInstance,
    controller: ARADocumentControllerRef,
    known: AraRoles,
    assigned: AraRoles,
) -> Result<AraPluginExtension, AraError> {
    let mut data = AraAudioUnitPlugInExtensionBinding {
        in_out_magic_number: ARA_AUDIO_UNIT_MAGIC,
        in_document_controller_ref: controller,
        out_plug_in_extension: ptr::null(),
        known_roles: known.bits(),
        assigned_roles: assigned.bits(),
    };
    let mut size = u32::try_from(std::mem::size_of::<AraAudioUnitPlugInExtensionBinding>())
        .map_err(|_| AraError::Unsupported("AU ARA binding size"))?;
    let status = unsafe {
        AudioUnitGetProperty(
            unit,
            K_AUDIO_UNIT_PROPERTY_ARA_BINDING,
            kAudioUnitScope_Global,
            0,
            NonNull::from(&mut data).cast::<c_void>(),
            NonNull::from(&mut size),
        )
    };
    if status != 0 {
        return Err(AraError::Unsupported("AU ARA binding property"));
    }
    if usize::try_from(size).map_err(|_| AraError::Abi("AU ARA binding size"))?
        != std::mem::size_of::<AraAudioUnitPlugInExtensionBinding>()
        || data.in_out_magic_number != ARA_AUDIO_UNIT_MAGIC
        || data.in_document_controller_ref != controller
        || data.out_plug_in_extension.is_null()
    {
        return Err(AraError::Peer("AU ARA binding returned an invalid result"));
    }
    unsafe { AraPluginExtension::from_raw(data.out_plug_in_extension) }
}
