//! The photo pipeline: many photos x many recipes. A [`Project`] says what to process and how to export it.

mod project;

pub use project::{Format, Project, RecipeSource, Settings};
