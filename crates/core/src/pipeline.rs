//! The photo pipeline: many photos x many recipes. A [`Project`] says what to process and how to export it.

mod atomic;
mod cache;
mod checks;
mod export;
mod identity;
mod manifest;
mod names;
mod project;
mod run;
mod sheet_files;
mod stages;
mod starter;

pub use cache::MapCache;
pub use checks::{matte_warnings, still_warnings};
pub use export::{export_still, resize};
pub use identity::{core_identity, fingerprint, identity_files};
pub use manifest::{Item, Status, read_manifest, report_html, write_record};
pub use names::{file_name, safe_name};
pub use project::{Format, Project, RecipeSource, Settings};
pub use run::{Found, Given, RunOptions, needed_maps, run};
pub use stages::{number_pieces, write_stages};
pub use starter::init_project;
