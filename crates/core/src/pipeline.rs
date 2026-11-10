//! The photo pipeline: many photos x many recipes. A [`Project`] says what to process and how to export it.

mod cache;
mod identity;
mod project;

pub use cache::MapCache;
pub use identity::{core_identity, fingerprint, identity_files};
pub use project::{Format, Project, RecipeSource, Settings};
