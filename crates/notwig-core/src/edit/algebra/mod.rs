mod delete;
mod materialize;
mod move_ops;
mod paste;
mod trim;

pub use delete::{cut, delete, split};
pub use materialize::{copy, materialize};
pub use move_ops::{ObjectMove, move_selection, relocate_objects};
pub use paste::{duplicate, paste};
pub use trim::{Edge, trim_container, trim_object, trim_selection};
