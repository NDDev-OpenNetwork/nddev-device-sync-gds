//! Read-only GDS status adapter. GDS retains Git topology, classification and
//! policy ownership. NDS never interprets an unknown scope as authorization.

pub mod view;
pub use view::*;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
mod native;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
mod response;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub use native::Gds;

/// Canonical compiled-in runtime manifest for this integration.
pub const MANIFEST: &str = include_str!("../contracts/module.json");
