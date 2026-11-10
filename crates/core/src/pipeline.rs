//! The photo pipeline: many photos x many recipes. A [`Project`] says what to process and how to export it.

mod atomic;
mod cache;
mod checks;
mod export;
mod identity;
mod manifest;
mod project;

pub use cache::MapCache;
pub use checks::{matte_warnings, still_warnings};
pub use export::export_still;
pub use identity::{core_identity, fingerprint, identity_files};
pub use manifest::{Item, Status, read_manifest, report_html, write_record};
pub use project::{Format, Project, RecipeSource, Settings};
