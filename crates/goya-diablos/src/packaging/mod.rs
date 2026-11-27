//! Native packaging for Diablos (pkg, pkg_add, pkgsrc).

pub mod pkg;
pub mod pkg_add;
pub mod pkgsrc;

pub use pkg::{FreebsdPkg, create_pkg};
pub use pkg_add::{OpenbsdPkg, create_pkg_add};
pub use pkgsrc::{NetbsdPkg, create_pkgsrc};