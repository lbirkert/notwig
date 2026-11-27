//! Host-side typed extension registry.

use std::any::Any;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::extension_point::{Cardinality, ExtensionPoint};

/// Failure returned by typed registry operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    /// The point has no registered implementations.
    #[error("extension point {point} has no active implementation")]
    Missing {
        /// Point id.
        point: &'static str,
    },
    /// The point requires exactly one implementation but has a different count.
    #[error("extension point {point} requires exactly one active implementation, found {count}")]
    Cardinality {
        /// Point id.
        point: &'static str,
        /// Number of active implementations.
        count: usize,
    },
    /// A point id was queried with a different interface type.
    #[error("extension point {point} was registered with a different interface")]
    Interface {
        /// Point id.
        point: &'static str,
    },
    /// A single-cardinality point already has an active implementation.
    #[error("extension point {point} already has an active implementation")]
    AlreadyActive {
        /// Point id.
        point: &'static str,
    },
}

struct PointEntries<T: ?Sized + 'static> {
    implementations: BTreeMap<String, Arc<T>>,
}

impl<T: ?Sized + 'static> Default for PointEntries<T> {
    fn default() -> Self {
        Self {
            implementations: BTreeMap::new(),
        }
    }
}

/// Typed registry for core and in-process extension implementations.
#[derive(Default)]
pub struct CoreExtensionRegistry {
    points: BTreeMap<&'static str, Box<dyn Any>>,
    active: BTreeMap<&'static str, String>,
}

impl CoreExtensionRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an implementation without changing the active single point,
    /// unless the point has no active implementation yet.
    pub fn register<T: ?Sized + 'static>(
        &mut self,
        point: ExtensionPoint<T>,
        implementation_id: impl Into<String>,
        value: Arc<T>,
    ) -> Result<(), RegistryError> {
        let id = implementation_id.into();
        let entries = self.entries_mut::<T>(point.id())?;
        entries.implementations.insert(id.clone(), value);
        if point.cardinality() == Cardinality::Single && !self.active.contains_key(point.id()) {
            self.active.insert(point.id(), id);
        }
        Ok(())
    }

    /// Replaces an implementation and makes it active for single points.
    pub fn replace<T: ?Sized + 'static>(
        &mut self,
        point: ExtensionPoint<T>,
        implementation_id: impl Into<String>,
        value: Arc<T>,
    ) -> Result<(), RegistryError> {
        let id = implementation_id.into();
        self.entries_mut::<T>(point.id())?
            .implementations
            .insert(id.clone(), value);
        if point.cardinality() == Cardinality::Single {
            self.active.insert(point.id(), id);
        }
        Ok(())
    }

    /// Selects the active implementation for a single point.
    pub fn set_active<T: ?Sized + 'static>(
        &mut self,
        point: ExtensionPoint<T>,
        implementation_id: impl Into<String>,
    ) -> Result<(), RegistryError> {
        let id = implementation_id.into();
        let entries = self.entries::<T>(point.id())?;
        if !entries.implementations.contains_key(&id) {
            return Err(RegistryError::Missing { point: point.id() });
        }
        self.active.insert(point.id(), id);
        Ok(())
    }

    /// Returns all active implementations for a point.
    #[must_use]
    pub fn get_extensions<T: ?Sized + 'static>(&self, point: ExtensionPoint<T>) -> Vec<Arc<T>> {
        self.try_get_extensions(point).unwrap_or_default()
    }

    /// Returns all active implementations for a point.
    pub fn try_get_extensions<T: ?Sized + 'static>(
        &self,
        point: ExtensionPoint<T>,
    ) -> Result<Vec<Arc<T>>, RegistryError> {
        let entries = self.entries::<T>(point.id())?;
        match point.cardinality() {
            Cardinality::Many => Ok(entries.implementations.values().cloned().collect()),
            Cardinality::Single => {
                let Some(active) = self.active.get(point.id()) else {
                    return Ok(Vec::new());
                };
                Ok(entries
                    .implementations
                    .get(active)
                    .cloned()
                    .into_iter()
                    .collect())
            }
        }
    }

    /// Returns the single active implementation for a point.
    ///
    /// # Panics
    ///
    /// Panics when the point has zero or more than one active implementation.
    #[must_use]
    pub fn get_extension<T: ?Sized + 'static>(&self, point: ExtensionPoint<T>) -> Arc<T> {
        match self.try_get_extension(point) {
            Ok(extension) => extension,
            Err(error) => panic!("{error}"),
        }
    }

    /// Returns the single active implementation for a point.
    pub fn try_get_extension<T: ?Sized + 'static>(
        &self,
        point: ExtensionPoint<T>,
    ) -> Result<Arc<T>, RegistryError> {
        let values = self.try_get_extensions(point)?;
        if values.len() == 1 {
            Ok(values.into_iter().next().expect("length checked"))
        } else if values.is_empty() {
            Err(RegistryError::Missing { point: point.id() })
        } else {
            Err(RegistryError::Cardinality {
                point: point.id(),
                count: values.len(),
            })
        }
    }

    fn entries<T: ?Sized + 'static>(
        &self,
        point: &'static str,
    ) -> Result<&PointEntries<T>, RegistryError> {
        self.points
            .get(point)
            .and_then(|entries| entries.downcast_ref::<PointEntries<T>>())
            .ok_or(RegistryError::Interface { point })
    }

    fn entries_mut<T: ?Sized + 'static>(
        &mut self,
        point: &'static str,
    ) -> Result<&mut PointEntries<T>, RegistryError> {
        if !self.points.contains_key(point) {
            self.points
                .insert(point, Box::new(PointEntries::<T>::default()));
        }
        self.points
            .get_mut(point)
            .and_then(|entries| entries.downcast_mut::<PointEntries<T>>())
            .ok_or(RegistryError::Interface { point })
    }
}
