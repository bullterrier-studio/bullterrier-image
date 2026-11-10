//! `light_wrap`: the backdrop's light spilling over the edge of a cut-out. What it does is documented once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::blur::weighted_gaussian;
use crate::ops::distance::signed;
use crate::ops::edge::Extension;
use crate::ops::light::screen;
use crate::params::{FieldName, Params};
use crate::{Canvas, Footprint, Image, Inputs, Picture};

/// Where `light_wrap` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Subject,
    opacity: 1.0,
    inputs: &[],
};

/// How far the backdrop's colour is gathered from, in sigmas of its blur.
const TRUNCATE: f64 = 4.0; // constant: scipy's default reach for a Gaussian

/// Radius: the blur's reach (`TRUNCATE * blur` + 0.5 px, and a px more for rounding). A pixel changes only where the
/// backdrop is within that reach, and then the nearest backdrop pixel, which gives its depth, is within it too, however
/// large `width` is.
pub(crate) fn footprint(params: &Params, _canvas: &Canvas) -> Footprint {
    Footprint::Radius(TRUNCATE * f64::from(params.blur) + 1.0)
}

/// Let the light of the backdrop spill over the edge of a shape (the subject by default), as it does round a real object
/// in front of a bright background.
///
/// The backdrop's colour is the picture outside the shape, blurred by `blur` px so that detail does not wrap, and only
/// from the backdrop itself. It is screened over the shape: a pixel at depth d inside the edge gets
/// `amount * (1 - (d - 1) / width)` of it: the outermost ring (d = 1) gets `amount`, the band is the `width` rings
/// d = 1 to `width`. Pixels outside the shape and deeper than `width` are exactly as they were, and since it is
/// screened the shape only ever brightens (a black backdrop adds nothing). Use it after a background replacement, so the
/// subject looks lit by its new surroundings.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The shape the light wraps onto: `subject` (the subject's silhouette), `background`, `light`, `depth`, or a region
    /// of the recipe (text, a logo, a drawn shape). A pixel is inside it from `field_threshold` up, and the backdrop is
    /// everything else.
    #[param(default = "subject")]
    pub field: FieldName,
    /// How far in from the shape's edge the light reaches, in px at the recipe's size: raising it lets the light spill
    /// deeper. 0 = nothing is changed.
    #[param(default = 12.0, unit = px, min = 0, max = 96)]
    pub width: f32,
    /// How strongly the backdrop's colour is screened over the edge: 0 = off, 1 = the colour at full strength at the
    /// edge, fading to nothing just beyond `width`.
    #[param(default = 0.7, unit = fraction, min = 0, max = 1)]
    pub amount: f32,
    /// Gaussian sigma, in px, with which the backdrop's colour is softened: raising it spreads the light evenly and loses
    /// the backdrop's detail, lowering it carries the backdrop's shapes onto the edge.
    #[param(default = 6.0, unit = px, min = 1, max = 100)]
    pub blur: f32,
    /// The field's value, 0 to 1, from which a pixel belongs to the shape the light wraps onto: 0.5 = more than half
    /// covered. Lowering it takes in more of a soft edge (hair, fur), raising it less.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1, advanced)]
    pub field_threshold: f32,
}

/// The picture with the backdrop's light screened over the shape's edge.
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
    let signed = signed(&inside, (height, width)); // < 0 inside the shape; an infinite depth is deeper than any band
    let backdrop: Vec<f32> = inside.iter().map(|&within| f32::from(u8::from(!within))).collect();
    let light = weighted_gaussian(picture, &backdrop, f64::from(params.blur), TRUNCATE, Extension::Reflect);
    picture.map_at(|row, column, pixel| {
        let at = row * width + column;
        let depth = -signed[at];
        if !inside[at] || depth - 1.0 >= params.width {
            return pixel;
        }
        let Some(colour) = light[at] else {
            return pixel;
        };
        let share = params.amount * (1.0 - (depth - 1.0) / params.width);
        screen(pixel, colour.map(|channel| channel * share))
    })
}
