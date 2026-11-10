//! `defringe`: the colour halo a cut-out's edge carries, replaced by the colour of the subject just inside. What it does
//! is documented once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::blur::weighted_gaussian;
use crate::ops::distance::signed;
use crate::ops::edge::Extension;
use crate::params::{FieldName, Params};
use crate::{Canvas, Footprint, Image, Inputs, Picture};

/// Where `defringe` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Subject,
    opacity: 1.0,
    inputs: &[],
};

/// How far the interior colour is gathered from, in sigmas of its blur.
const TRUNCATE: f64 = 4.0; // constant: scipy's default reach for a Gaussian
/// The blur's sigma as a share of the band's width.
const SIGMA_PER_WIDTH: f64 = 0.5; // constant: the interior colour is a mean over about the band's depth either side

/// Radius: a pixel needs its depth (`width`), then the interior around it (the kernel's reach, 4 sigmas of `width` / 2
/// plus half a px) with each of those pixels' depth (`width` more), and a px for the kernel's rounding.
pub(crate) fn footprint(params: &Params, _canvas: &Canvas) -> Footprint {
    Footprint::Radius(f64::from(params.width) * (1.0 + TRUNCATE * SIGMA_PER_WIDTH) + 1.0)
}

/// Remove the colour halo that a cut-out's edge carries: within `width` px inside the edge of a shape (the subject by
/// default) the picture takes the colour of the shape's interior.
///
/// The interior colour is the shape's pixels deeper than `width` from the edge, averaged with a Gaussian of sigma
/// `width` / 2, so it is pulled outward to the edge from the nearest solid colour. A pixel at depth d inside the edge
/// takes `amount * (1 - (d - 1) / width)` of it: the outermost ring (d = 1) takes `amount`, the band is the `width`
/// rings d = 1 to `width`, and nothing deeper changes. Pixels outside the shape and deeper than `width` are exactly as
/// they were, and so is a shape too thin to have an interior. Use it after a background
/// replacement, when the old backdrop's colour still tints the matte's edge.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The shape whose edge is cleaned: `subject` (the subject's silhouette), `background`, `light`, `depth`, or a region
    /// of the recipe (text, a logo, a drawn shape). A pixel is inside it from `field_threshold` up.
    #[param(default = "subject")]
    pub field: FieldName,
    /// Depth of the band inside the shape's edge that is recoloured, in px at the recipe's size: raising it cleans a
    /// wider fringe. 0 = nothing is changed.
    #[param(default = 3.0, unit = px, min = 0, max = 48)]
    pub width: f32,
    /// How fully the band takes the interior colour at the edge: 0 = unchanged, 1 = the edge pixels are almost wholly
    /// the interior colour. It fades to nothing just beyond `width`.
    #[param(default = 1.0, unit = fraction, min = 0, max = 1)]
    pub amount: f32,
    /// The field's value, 0 to 1, from which a pixel belongs to the shape whose edge is cleaned: 0.5 = more than half
    /// covered. Lowering it takes in more of a soft edge (hair, fur), raising it less.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1, advanced)]
    pub field_threshold: f32,
}

/// The picture with the band inside the edge pulled toward the colour of the interior.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let pixels = picture.pixels();
    // `Inputs::declared_by` refuses an offer that lacks a declared field, so this branch is out of reach: `render` cannot
    // return an error, and a field cannot be read without an Option
    let Some(field) = inputs.field("field") else {
        return Image::from_pixels(height, width, pixels.to_vec());
    };
    if params.width <= 0.0 || params.amount <= 0.0 {
        return Image::from_pixels(height, width, pixels.to_vec());
    }
    let inside: Vec<bool> = field
        .values()
        .iter()
        .map(|&value| value >= params.field_threshold)
        .collect();
    // > 0 outside the shape, < 0 inside it; an infinite depth (no edge in reach) is deeper than any band
    let signed = signed(&inside, (height, width));
    let band = params.width;
    let interior: Vec<f32> = inside
        .iter()
        .zip(&signed)
        .map(|(&within, &distance)| f32::from(u8::from(within && -distance > band)))
        .collect();
    let sigma = f64::from(band) * SIGMA_PER_WIDTH;
    let solid = weighted_gaussian(picture, &interior, sigma, TRUNCATE, Extension::Reflect);
    picture.map_at(|row, column, pixel| {
        let at = row * width + column;
        let depth = -signed[at];
        if !inside[at] || depth - 1.0 >= band {
            return pixel;
        }
        let Some(target) = solid[at] else {
            return pixel;
        };
        let share = params.amount * (1.0 - (depth - 1.0) / band);
        [0, 1, 2].map(|channel| pixel[channel] + (target[channel] - pixel[channel]) * share)
    })
}
