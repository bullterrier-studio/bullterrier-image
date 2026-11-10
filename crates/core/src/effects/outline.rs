//! `outline`: a coloured stroke, and optionally a soft halo, around a shape. What it does is documented once, on
//! `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::distance::signed;
use crate::params::{Choice, Color, FieldName, Params};
use crate::{Image, Inputs, Picture};

/// Where `outline` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Subject,
    opacity: 1.0,
    inputs: &[],
};

/// Radius: a pixel is touched only within the stroke and its halo of the edge (`width + glow`), plus the one-pixel
/// antialiasing ramp.
pub(crate) fn footprint(params: &Params) -> Footprint {
    Footprint::Radius(f64::from(params.width + params.glow) + 1.0) // constant: the antialiasing ramp's px
}

/// Which side of the shape's edge the stroke lies on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Position {
    /// Around the shape, whose own pixels stay untouched.
    Outside,
    /// Painted over the shape's rim.
    Inside,
    /// Half of the width on each side of the edge.
    Center,
}

/// A coloured stroke of `width` px around a shape (the subject by default), with an optional soft halo of `glow` px.
///
/// The stroke follows the shape's edge on the side `position` says; 'outside' leaves the shape's own pixels exactly as
/// they are, like a sticker's border, and a big `glow` with a thin stroke makes an aura. Where the shape is soft the
/// stroke lies behind it. The edge is antialiased over one pixel.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The shape outlined: `subject` (the subject's silhouette), `background`, `light`, `depth`, or a region of the
    /// recipe (text, a logo, a drawn shape). A pixel is inside it from `field_threshold` up.
    #[param(default = "subject")]
    pub field: FieldName,
    /// Thickness of the stroke in px at the recipe's size; raising it thickens the line. 0 = no stroke.
    #[param(default = 6.0, unit = px, min = 0, max = 200)]
    pub width: f32,
    /// The colour of the stroke and of its halo: '#rrggbb', '#rgb' or [r, g, b] 0..1.
    #[param(default = "#ffffff")]
    pub color: Color,
    /// Which side of the shape's edge the stroke lies on: 'outside' (around the shape, whose own pixels stay
    /// untouched), 'inside' (painted over the shape's rim) or 'center' (half of `width` on each side).
    #[param(default = Outside)]
    pub position: Position,
    /// How far a soft halo of the same colour reaches beyond the stroke, in px; it fades smoothly from the stroke to
    /// nothing. Raising it spreads the halo (a large value makes an aura). 0 = no halo.
    #[param(default = 0.0, unit = px, min = 0, max = 200)]
    pub glow: f32,
    /// The field's value, 0 to 1, from which a pixel belongs to the shape whose edge is outlined: 0.5 = more than half
    /// covered. Lowering it takes in more of a soft edge (hair, fur), raising it less.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1, advanced)]
    pub field_threshold: f32,
}

/// The picture with the stroke and its halo laid over it.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    // `Inputs::declared_by` refuses an offer that lacks a declared field, so this branch is out of reach: `render` cannot
    // return an error, and a field cannot be read without an Option
    let Some(field) = inputs.field("field") else {
        return Image::from_pixels(height, width, picture.pixels().to_vec());
    };
    let shape = field.values();
    let inside: Vec<bool> = shape.iter().map(|&value| value >= params.field_threshold).collect();
    let signed = signed(&inside, (height, width)); // > 0 outside the shape, < 0 inside it
    // the share of the stroke beyond the edge
    let outer_share = match params.position {
        Position::Outside => 1.0,
        Position::Inside => 0.0,
        Position::Center => 0.5, // constant: half the stroke on each side
    };
    let reach_out = params.width * outer_share;
    let reach_in = params.width - reach_out;
    let (glow, tint) = (params.glow, params.color.0);
    picture.map_at(|row, column, pixel| {
        let at = row * width + column;
        let distance = signed[at];
        // full coverage up to the reach, then a one-pixel ramp down to nothing
        let alpha = if distance > 0.0 {
            let stroke = (reach_out - distance + 1.0).clamp(0.0, 1.0);
            let covered = if glow > 0.0 {
                // px past the outer edge of the stroke; the fade is quadratic so that it has no visible edge at `glow`
                let beyond = (distance - reach_out).max(0.0);
                let halo = (1.0 - beyond / glow).clamp(0.0, 1.0);
                stroke.max(halo * halo)
            } else {
                stroke
            };
            // outside the shape the stroke lies behind it: a soft edge shows the stroke only where the shape is absent
            covered * (1.0 - shape[at])
        } else {
            (reach_in + distance + 1.0).clamp(0.0, 1.0)
        };
        [0, 1, 2].map(|channel| pixel[channel] + (tint[channel] - pixel[channel]) * alpha)
    })
}
