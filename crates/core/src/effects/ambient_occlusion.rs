//! `ambient_occlusion`: the picture darkened where its depth (or any field) says it is sheltered, in creases and beside
//! nearer things. What it does is documented once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::blur;
use crate::ops::edge::Edge;
use crate::params::{FieldName, Params};
use crate::{Image, Inputs, Picture};

/// Where `ambient_occlusion` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Light,
    opacity: 0.6, // constant: the default opacity of a step: a full occlusion is heavy, the step mixes it in partway
    inputs: &[],
};

/// An ambient occlusion: each pixel darkened by `gain` times how much farther it lies than the depth around it, within
/// `radius`.
///
/// Darken the places the depth says are sheltered: creases, and the ground or wall right beside something nearer.
/// Screen-space, from the depth alone: a pixel's cavity is the depth averaged by a Gaussian of sigma `radius` minus its
/// own depth, positive where it lies farther than its surroundings. It is darkened by clip(gain x cavity, 0, 1), one grey
/// factor on every channel, so the colour ratios are kept and nothing is brightened. Flat and evenly sloping depths are
/// left as they are. A depth step at a subject's outline darkens its farther side (a contact shadow) and leaves the
/// nearer side alone; raising `radius` widens that shadow. The step's `opacity` sets how much is mixed in.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The field read as how near each pixel is, 1 = nearest: `depth` (the scene's depth), `light` (bright parts stand
    /// nearer, so dark creases between them darken), `subject` (a contact shadow around the subject), or a region of the
    /// recipe.
    #[param(default = "depth")]
    pub field: FieldName,
    /// The size of the neighbourhood that counts as shelter, the Gaussian sigma in px at the recipe's size over which the
    /// surrounding depth is averaged. Raising it darkens wider areas around nearer things; lowering it keeps the
    /// darkening to fine creases.
    #[param(default = 10.0, unit = px, min = 1, max = 200)]
    pub radius: f32,
    /// How strongly a depth difference darkens: a pixel lying d farther than its surroundings (depth 0..1) is darkened by
    /// gain x d, at most to black. Raising it darkens shallow creases more and turns deep ones black sooner.
    #[param(default = 4.0, unit = ratio, min = 1, max = 100)]
    pub gain: f32,
    /// Where the averaging Gaussian is cut off, in sigmas: it reaches truncate x radius px. Larger is more exact and
    /// slower.
    #[param(default = 4.0, unit = ratio, min = 1, max = 10, advanced)]
    pub truncate: f32,
    /// What the averaging sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored
    /// about the edge pixel), 'nearest' (the edge pixel repeated) or 'wrap' (the opposite edge, so a tileable picture
    /// darkens without a seam).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The picture times one grey factor, 1 - clip(gain x (the field's Gaussian mean - the field), 0, 1).
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    // `Inputs::declared_by` refuses an offer that lacks a declared field, so this branch is out of reach: `render` cannot
    // return an error, and a field cannot be read without an Option
    let Some(field) = inputs.field("field") else {
        return Image::from_pixels(picture.height(), picture.width(), picture.pixels().to_vec());
    };
    let mean = blur::gaussian_filter_plane(
        field.values(),
        (picture.height(), picture.width()),
        f64::from(params.radius),
        f64::from(params.truncate),
        params.edge.extension(),
    );
    let width = picture.width();
    picture.map_at(|row, column, pixel| {
        let at = row * width + column;
        let occlusion = (params.gain * (mean[at] - field.values()[at])).clamp(0.0, 1.0);
        pixel.map(|value| value * (1.0 - occlusion))
    })
}
