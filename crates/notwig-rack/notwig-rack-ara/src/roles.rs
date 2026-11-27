//! ARA processor roles shared by every companion format.

use ara2_bridge_host::ExtensionRoles;
use ara2_bridge_sys::{kARAEditorRendererRole, kARAEditorViewRole, kARAPlaybackRendererRole};

bitflags::bitflags! {
    /// Roles a companion API can assign to an ARA-bound processor instance.
    ///
    /// These mirror `ARAPlugInInstanceRoleFlags`. The host passes the complete
    /// set it understands as `known`, and the subset assigned to a specific
    /// instance as `assigned`.
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    pub struct AraRoles: i32 {
        /// Realtime or offline playback renderer.
        const PLAYBACK_RENDERER = kARAPlaybackRendererRole.cast_signed();
        /// Editor-preview renderer.
        const EDITOR_RENDERER = kARAEditorRendererRole.cast_signed();
        /// Editor view.
        const EDITOR_VIEW = kARAEditorViewRole.cast_signed();
    }
}

impl AraRoles {
    /// The roles understood by this framework and used as the default
    /// `known` set.
    pub const KNOWN: Self = Self::PLAYBACK_RENDERER
        .union(Self::EDITOR_RENDERER)
        .union(Self::EDITOR_VIEW);

    /// The default roles assigned to one fully featured instance.
    pub const ASSIGNED: Self = Self::KNOWN;

    /// Converts to the host runtime role set.
    #[must_use]
    pub fn to_extension_roles(self) -> ExtensionRoles {
        ExtensionRoles::from_bits_retain(self.bits())
    }
}

impl From<AraRoles> for ExtensionRoles {
    fn from(value: AraRoles) -> Self {
        value.to_extension_roles()
    }
}

#[cfg(test)]
mod tests {
    use super::AraRoles;

    #[test]
    fn known_is_the_union_of_assigned_roles() {
        assert_eq!(AraRoles::KNOWN.bits(), 0b111);
        assert!(AraRoles::KNOWN.contains(AraRoles::ASSIGNED));
    }
}
