use std::fmt;
use std::marker::PhantomData;

/// Typed key for one extension point.
///
/// The point contains only a stable identifier. Its interface and registered
/// implementations live in the host registry.
pub struct ExtensionPoint<T: ?Sized> {
    id: &'static str,
    cardinality: Cardinality,
    marker: PhantomData<fn() -> T>,
}

/// Cardinality policy for one extension point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cardinality {
    /// Any number of implementations may be active.
    Many,
    /// Exactly one implementation may be active.
    Single,
}

impl<T: ?Sized> ExtensionPoint<T> {
    /// Creates a point with a stable namespaced identifier.
    #[must_use]
    pub const fn new(id: &'static str) -> Self {
        Self::many(id)
    }

    /// Creates a multi-implementation point.
    #[must_use]
    pub const fn many(id: &'static str) -> Self {
        Self {
            id,
            cardinality: Cardinality::Many,
            marker: PhantomData,
        }
    }

    /// Creates a single-active-implementation point.
    #[must_use]
    pub const fn single(id: &'static str) -> Self {
        Self {
            id,
            cardinality: Cardinality::Single,
            marker: PhantomData,
        }
    }

    /// Returns the point cardinality policy.
    #[must_use]
    pub const fn cardinality(&self) -> Cardinality {
        self.cardinality
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
