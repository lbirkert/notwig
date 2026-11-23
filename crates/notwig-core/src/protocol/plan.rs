use std::fmt;
use std::sync::Arc;

#[derive(Clone)]
pub struct PlanHandle(Arc<dyn std::any::Any + Send + Sync>);

impl PlanHandle {
    #[must_use]
    pub fn new<T: std::any::Any + Send + Sync>(plan: T) -> Self {
        Self(Arc::new(plan))
    }

    #[must_use]
    pub fn from_arc<T: std::any::Any + Send + Sync>(plan: Arc<T>) -> Self {
        Self(plan)
    }

    #[must_use]
    pub fn downcast_ref<T: std::any::Any>(&self) -> Option<&T> {
        self.0.downcast_ref::<T>()
    }

    #[must_use]
    pub fn as_arc_any(&self) -> Arc<dyn std::any::Any + Send + Sync> {
        Arc::clone(&self.0)
    }
}

impl fmt::Debug for PlanHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PlanHandle(<prepared>)")
    }
}

impl PartialEq for PlanHandle {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(Arc::as_ptr(&self.0), Arc::as_ptr(&other.0))
    }
}
