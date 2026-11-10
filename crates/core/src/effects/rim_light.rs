//! `rim_light`: a band of coloured light along a shape's edge, on the side facing the light. What it does is documented
//! once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::blur::gaussian_filter_owned;
use crate::ops::distance::signed;
use crate::ops::edge::Edge;
use crate::ops::light::screen;
use crate::ops::relief::shade_slopes;
use crate::params::{Color, FieldName, Params};
use crate::{Canvas, Footprint};
use crate::{Image, Inputs, Picture};

/// Where `rim_light` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Subject,
    opacity: 1.0,
    inputs: &[],
};

/// Whole: a shape with no edge anywhere in the picture lights nothing.
pub(crate) fn footprint(_params: &Params, _canvas: &Canvas) -> Footprint {
    Footprint::Whole
}

/// Light along a shape's edge (the subject by default) from one side: a band of `width` px inside the edge, tinted
/// `color`.
///
/// Only the edge that faces the light is lit, by the cosine of the angle between its outward direction and the
/// direction of `angle` (head-on = full, side-on = none, away = none), and the light fades with depth by `falloff`. It
/// is screened over the picture, so it only ever brightens, and never reaches outside the shape or its interior beyond
/// `width`. The picture's own border is no edge. A shape without an edge (empty or full) lights nothing.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The shape whose edge is lit: `subject` (the subject's silhouette), `background`, `light`, `depth`, or a region of
    /// the recipe (text, a logo, a drawn shape). A pixel is inside it from `field_threshold` up.
    #[param(default = "subject")]
    pub field: FieldName,
    /// The direction the light comes from, in degrees clockwise from pointing right, as seen on screen: 0 = from the
    /// right, 90 = from below, 180 = from the left, 270 = from above. Only edges whose outward direction faces it are
    /// lit.
    #[param(default = 320.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub angle: f32,
    /// The colour of the light: '#rrggbb', '#rgb' or [r, g, b] 0..1. The default is a warm orange.
    #[param(default = "#ff9a4d")]
    pub color: Color,
    /// Depth of the lit band inside the shape's edge, in px at the recipe's size: raising it lets the light reach
    /// further in.
    #[param(default = 12.0, unit = px, min = 1, max = 200)]
    pub width: f32,
    /// How strong the light is where it is brightest (an edge that faces the light head-on): 0 = off, 1 = the colour at
    /// full strength, above 1 saturates sooner.
    #[param(default = 1.0, unit = ratio, min = 0, max = 5)]
    pub amount: f32,
    /// How the light fades from the edge into the band: it is (1 - depth / width)^falloff of its strength. 1 = evenly,
    /// above 1 stays close to the edge, below 1 carries further in.
    #[param(default = 1.5, unit = ratio, min = 0.1, max = 10)]
    pub falloff: f32,
    /// Gaussian sigma, in px, with which the silhouette is smoothed before the edge's direction is read: raising it
    /// evens out the jaggedness of a pixel edge (fur), at the cost of detail in small features. 0 uses the raw edge.
    #[param(default = 2.0, unit = px, min = 0, max = 50, advanced)]
    pub smoothing: f32,
    /// Where the `smoothing` blur is cut off, in sigmas: it reaches truncate x smoothing px. Larger is more exact and
    /// slower.
    #[param(default = 4.0, unit = ratio, min = 1, max = 10, advanced)]
    pub truncate: f32,
    /// What the `smoothing` blur sees beyond the picture's border: 'reflect' (mirrored, border pixel repeated), 'mirror'
    /// (mirrored about the border pixel), 'nearest' (the border pixel repeated) or 'wrap' (the opposite border).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
    /// The field's value, 0 to 1, from which a pixel belongs to the shape whose edge is lit: 0.5 = more than half
    /// covered. Lowering it takes in more of a soft edge (hair, fur), raising it less.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1, advanced)]
    pub field_threshold: f32,
}

/// The picture with the rim's light screened over it.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let unchanged = || Image::from_pixels(height, width, picture.pixels().to_vec());
    // `Inputs::declared_by` refuses an offer that lacks a declared field, so this branch is out of reach: `render` cannot
    // return an error, and a field cannot be read without an Option
    let Some(field) = inputs.field("field") else {
        return unchanged();
    };
    let shape = field.values();
    let inside: Vec<bool> = shape.iter().map(|&value| value >= params.field_threshold).collect();
    let signed = signed(&inside, (height, width)); // > 0 outside the shape, < 0 inside it, infinite without an edge
    if signed.iter().any(|distance| distance.is_infinite()) {
        return unchanged();
    }
    // the outward direction of the edge is the way the smoothed distance grows
    let edge = params.edge.extension();
    let smooth = gaussian_filter_owned(
        signed.clone(),
        (height, width),
        f64::from(params.smoothing),
        f64::from(params.truncate),
        edge,
    );
    // x runs right and y down, so a clockwise angle turns from x toward y; a whole turn is the same direction exactly
    let toward = f64::from(params.angle.rem_euclid(360.0)).to_radians(); // constant: a full turn in degrees
    #[expect(clippy::cast_possible_truncation, reason = "a direction, 0..1 in each part")]
    let (to_x, to_y) = (toward.cos() as f32, toward.sin() as f32);
    let (band_width, falloff, amount, tint) = (params.width, params.falloff, params.amount, params.color.0);
    let pixels_in = picture.pixels();
    let mut pixels = vec![[0.0; 3]; height * width];
    // constant: the distance is already in px, so a slope of the smoothed distance needs no scale
    shade_slopes(
        &smooth,
        (height, width),
        1.0,
        edge,
        &mut pixels,
        |index, rise_y, rise_x| {
            if !inside[index] {
                return pixels_in[index];
            }
            // constant: guards a flat distance (no direction, no light)
            let length = rise_x.hypot(rise_y).max(1e-6);
            let facing = ((rise_x * to_x + rise_y * to_y) / length).max(0.0);
            let depth = -signed[index]; // px from the edge into the shape; 1 on its outermost pixels
            let band = (1.0 - (depth - 1.0) / band_width).clamp(0.0, 1.0).powf(falloff);
            let light = amount * facing * band * shape[index];
            screen(pixels_in[index], tint.map(|channel| light * channel))
        },
    );
    Image::from_pixels(height, width, pixels)
}
