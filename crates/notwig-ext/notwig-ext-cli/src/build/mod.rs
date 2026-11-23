//! Extension build and packaging.
//!
//! [`source`] finds the extension root and reads Cargo metadata, [`compile`]
//! drives per-target Cargo builds, [`package`] writes ZIPs, and [`paths`] owns
//! the package-relative path and file-name rules. The public functions are
//! re-exported here.

mod compile;
mod package;
mod paths;
mod source;

pub use compile::build_entrypoint;
pub use package::{PackageEntry, package_entries, package_extension, package_targets};
