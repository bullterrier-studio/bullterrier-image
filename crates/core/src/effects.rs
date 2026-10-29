//! The effects, one module per file in `src/effects/`, and their table; the build script writes both.
//!
//! An effect file has two items: `Params`, the effect's parameters, declared once (see [`mod@crate::params`]; they are read
//! with `#[derive(Deserialize)]` and `#[serde(deny_unknown_fields)]`, none left out), and
//! `fn render(picture: &Picture<'_>, params: &Params) -> Image`. Effects never refer to one another: what two of them
//! share goes in `crate::ops`.

use crate::params::Table;
use crate::{Error, Image, Picture};

/// The function that runs one effect: reads its parameters, then renders the picture with them.
// `::serde_json`, the crate, as in the generated table: an effect module named `serde_json` is in scope here
pub(crate) type Run = fn(&Picture<'_>, ::serde_json::Value) -> Result<Image, Error>;

/// The function that describes one effect's parameters.
pub(crate) type Describe = fn() -> Table;

include!(concat!(env!("OUT_DIR"), "/effects.rs"));
