use std::fmt;
use std::marker::PhantomData;

/// Typed key for one extension point.
///
/// The point contains only a stable identifier. Its interface and registered
/// implementations live in the host registry.
pub struct ExtensionPoint<T: ?Sized> {
    id: &'static str,
    marker: PhantomData<fn() -> T>,
}

impl<T: ?Sized> ExtensionPoint<T> {
    /// Creates a point with a stable namespaced identifier.
    #[must_use]
    pub const fn new(id: &'static str) -> Self {
        Self {
            id,
            marker: PhantomData,
        }
    }

    /// Returns the stable point identifier.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        self.id
    }
}

impl<T: ?Sized> Copy for ExtensionPoint<T> {}

impl<T: ?Sized> Clone for ExtensionPoint<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized> fmt::Debug for ExtensionPoint<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ExtensionPoint").field(&self.id).finish()
    }
}
