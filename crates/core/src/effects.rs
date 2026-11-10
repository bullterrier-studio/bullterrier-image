//! The effects, one public module per file in `src/effects/`, and the list of them; the build script writes both.
//!
//! An effect file has four items: `Params`, the effect's parameters, public and declared once (see [`mod@crate::params`];
//! they are read with `#[derive(Deserialize)]` and `#[serde(deny_unknown_fields)]`, none left out; `Step::of` builds a
//! step from them); `META`, its section of the catalogue, default opacity and inputs (`pub(crate) const META: Meta`);
//! `pub(crate) fn footprint(params: &Params) -> Footprint`, what it reads around a pixel (see
//! [`Footprint`](crate::Footprint); `Whole` when unsure); and
//! `pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image`. Effects never refer to
//! one another: what two of them share goes in `crate::ops`.

include!(concat!(env!("OUT_DIR"), "/effects.rs"));
