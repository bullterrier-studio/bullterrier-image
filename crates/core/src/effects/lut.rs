//! `lut`: a colour grade from a 3D lookup table. What it does is documented once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::params::{LutName, Params};
use crate::{Image, Inputs, Picture};

/// Where `lut` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Colour,
    opacity: 1.0,
    inputs: &[],
};

/// Colour grade from a LUT file (a film look): apply a `.cube` 3D lookup table, mixed in by `amount`.
///
/// Each pixel's colour is looked up in the table, interpolating trilinearly between the eight nearest entries; values
/// outside the table's domain take its edge entry, and the result is kept in 0..1.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The 3D colour lookup table: the path of a `.cube` file (Adobe/Resolve format; 1D tables are refused) in a recipe
    /// file (relative to the file) or in `fx`, or, in Python, a Lut or an (n, n, n, 3) array; in Rust, the name of a
    /// table the photo was given or the path of a `.cube` file. The default table changes nothing. Colours between the
    /// table's entries are interpolated trilinearly.
    #[param()]
    pub file: LutName,
    /// How much of the table's grade shows: 0 leaves the picture unchanged, 0.5 is the midway between the picture and
    /// the graded one, 1 is the table as written.
    #[param(default = 1.0, unit = fraction, min = 0, max = 1, drivable)]
    pub amount: f32,
}

/// The picture graded by the table the step names (`file`; the identity without one), mixed in by `amount`.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let table = inputs.lut("file");
    picture.map(|pixel| {
        let graded = table.map_or(pixel, |table| table.lookup(pixel));
        [0, 1, 2].map(|channel| (pixel[channel] + params.amount * (graded[channel] - pixel[channel])).clamp(0.0, 1.0))
    })
}
