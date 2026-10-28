//! The effects, one module per file in `src/effects/`, and their table; the build script writes both.
//!
//! An effect file has two items: `Params`, the effect's parameters (`#[derive(Deserialize)]`,
//! `#[serde(deny_unknown_fields)]` and no defaults, so a struct that drifts from the Python model fails its first
//! call), and `fn render(picture: &Picture<'_>, params: &Params) -> Image`. Effects never refer
//! to one another: what two of them share goes in `crate::ops`.

use crate::{Error, Image, Picture};

/// The function that runs one effect: reads its parameters, then renders the picture with them.
pub(crate) type Run = fn(&Picture<'_>, serde_json::Value) -> Result<Image, Error>;

include!(concat!(env!("OUT_DIR"), "/effects.rs"));
