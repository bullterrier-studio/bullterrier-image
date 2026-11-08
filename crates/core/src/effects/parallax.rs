//! `parallax`: the picture's layers shifted by their depth, or any field, as if the camera moved sideways. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::centre::Centre;
use crate::ops::edge::Edge;
use crate::ops::rank::maximum_filter;
use crate::ops::sample::{Interpolation, Sampler};
use crate::params::{FieldName, Params};
use crate::{Image, Inputs, Picture};

/// Where `parallax` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Distortion,
    opacity: 1.0,
    inputs: &[],
};

/// What `parallax` reads around a pixel.
// The depth is read up to that far away (the nearest within the largest shift) and the picture up to the shift itself, no
// farther: the two reads do not add.
pub(crate) fn footprint(params: &Params) -> Footprint {
    Footprint::reach(
        f64::from(params.amount) * f64::from(params.focus.max(1.0 - params.focus)),
        params.edge == Edge::Wrap,
    )
}

/// Shift every layer of the picture by amount x (depth - focus) px along `angle`, as a sideways camera move would: layers
/// nearer than `focus` shift along `angle`, farther ones the opposite way, each by its distance in depth from the focus.
///
/// Each output pixel reads the picture that far back along `angle`, taking its depth as the nearest within reach of the
/// largest shift, so a near object lands whole over what lies behind it. Background it uncovers is the background beside it
/// stretched over the gap: a 2.5D look, not a new view.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The field read as how near each layer is, 1 = nearest: `depth` (the scene's depth), `subject` (the subject moves as
    /// one layer against its background), `light` (bright parts move as nearer), or a region of the recipe.
    #[param(default = "depth")]
    pub field: FieldName,
    /// The direction the nearest layers move, in degrees clockwise from pointing right, as seen on screen: 0 = right, 90 =
    /// down, 180 = left. Layers farther than the focus move the opposite way.
    #[param(default = 0.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub angle: f32,
    /// How far the nearest layer (depth 1) moves when the focus is the farthest depth, 0 to 48 px: a layer moves amount x
    /// (its depth - focus) px. Raising it exaggerates the shift; 0 leaves the picture unchanged.
    #[param(default = 9.6, unit = px, min = 0, max = 48)]
    pub amount: f32,
    /// The depth that stays still, as a fraction of the picture's depth range: 1 = the nearest, 0 = the farthest. Nearer
    /// layers move along `angle`, farther ones the opposite way; raising it moves more of the picture against `angle`.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub focus: f32,
    /// How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps every value
    /// exactly as it is but edges turn jagged), 'bilinear' blends the four pixels around it (smooth, slightly soft),
    /// 'bicubic' fits a cubic through the neighbouring pixels (sharpest, a little slower, and may ring beside hard edges).
    #[param(default = Bilinear, advanced)]
    pub interpolation: Interpolation,
    /// What a pixel near the border sees when its shift reads from beyond the picture: 'reflect' (mirrored, edge pixel
    /// repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite
    /// edge).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The picture with every layer shifted by its depth.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the radius is a whole number of pixels from 0 to the amount (at most 48 design px, scaled by the picture's size)"
)]
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    // `Inputs::declared_by` refuses an offer that lacks a declared field, so this branch is out of reach: `render` cannot
    // return an error, and a field cannot be read without an Option
    let Some(field) = inputs.field("field") else {
        return picture.map(|pixel| pixel);
    };
    let (height, width) = (picture.height(), picture.width());
    let reach = f64::from(params.amount); // px a layer moves per unit of depth from the focus
    // A pixel reads where it lies minus its shift, so a pixel a near object lands on must take the object's shift, not its
    // own (farther) one: the depth is replaced by the nearest within the largest shift, ceil(reach x the farthest depth
    // from the focus) px, so near objects move whole instead of tearing. A square is the simplest window that covers every
    // direction, and its maximum is separable (fast at any size).
    let focus = f64::from(params.focus);
    let radius = (reach * focus.max(1.0 - focus)).ceil() as usize;
    let edge = params.edge.extension();
    let nearest = maximum_filter(field.values(), height, width, 2 * radius + 1, edge);
    let angle = f64::from(params.angle).to_radians();
    let (sin, cos) = angle.sin_cos();
    let sampler = Sampler::new(picture.pixels(), height, width, params.interpolation, edge);
    Centre::new(height, width, 0.5, 0.5).warp(&sampler, |spot| {
        // at the depth's own precision: a depth equal to the focus moves 0 px
        let shift = reach * (f64::from(nearest[spot.index]) - focus);
        // rows count down: a move down reads from above
        (spot.row - shift * sin, spot.column - shift * cos)
    })
}
