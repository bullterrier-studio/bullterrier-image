//! `focus_blur`: the picture blurred by how far a field's value lies from the one kept in focus. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::blur;
use crate::ops::edge::Edge;
use crate::params::{FieldName, Params};
use crate::{Image, Inputs, Picture};

/// Where `focus_blur` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Light,
    opacity: 1.0,
    inputs: &[],
};

/// What `focus_blur` reads around a pixel.
pub(crate) fn footprint(params: &Params) -> Footprint {
    Footprint::reach(f64::from(params.radius * params.truncate), params.edge == Edge::Wrap)
}

/// A shallow depth of field, or a tilt-shift miniature: pixels whose `field` value lies near `focus` stay sharp, and the
/// farther a value lies from it, higher or lower, the more the pixel is blurred, up to `radius`.
///
/// Each pixel is blurred by w x `radius`, w = clip((|field - focus| - width / 2) / feather, 0, 1), so the pixels whose
/// field lies within `width` / 2 of `focus` are returned exactly as they are. The blur is `levels` copies of the picture
/// with sigma radius x k / levels, and a pixel mixes the two copies (the sharp picture being copy 0) either side of w x
/// `levels`. `field` is the selection the recipe names (`depth`, a linear gradient...), 0..1: `depth` gives a lens
/// focused in the scene, a linear gradient a sharp band across the picture.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The selection the focus is measured on: `depth` (1 = nearest: a lens focused in the scene), `light`, `subject`,
    /// or a region of the recipe such as a linear gradient (a band of focus across the picture, the miniature look) or a
    /// radial one. Its value at a pixel is compared with `focus`.
    #[param(default = "depth")]
    pub field: FieldName,
    /// The value of the field that stays sharp, as a fraction of its range: for `depth` 1 = the nearest and 0 = the
    /// farthest. Raising it moves the focus towards the field's high values; driving it moves the focus through the
    /// scene.
    #[param(default = 0.8, unit = fraction, min = 0, max = 1)]
    pub focus: f32,
    /// How wide a band of the field's values around the focus stays exactly as it is, as a fraction of its range.
    /// Raising it keeps more of the picture sharp.
    #[param(default = 0.15, unit = fraction, min = 0, max = 1)]
    pub width: f32,
    /// How far in the field's values beyond the band's edge the blur takes to reach its full radius, as a fraction of
    /// its range: the blur grows evenly across it. Raising it makes a gentler fall-off, lowering it an abrupt one.
    #[param(default = 0.4, unit = fraction, min = 0.01, max = 1)]
    pub feather: f32,
    /// The Gaussian sigma in px at the recipe's size that the blur reaches at the full distance from the focus (a
    /// further `feather` from the band's edge): larger blurs the out-of-focus picture more; 0 = no blur.
    #[param(default = 10.0, unit = px, min = 0, max = 100)]
    pub radius: f32,
    /// How many blurred copies of the picture the blur is built from: copy k has the sigma radius x k / levels, and each
    /// pixel mixes the two copies (or the sharp picture and the first) nearest its distance from the focus. More is
    /// smoother between levels and slower; 1 mixes the sharp picture with one blur.
    #[param(default = 6, unit = count, min = 1, max = 16, advanced)]
    pub levels: i32,
    /// Where each blurred copy's Gaussian is cut off, in sigmas: it reaches truncate x its sigma px. Larger is more
    /// exact and slower.
    #[param(default = 4.0, unit = ratio, min = 1, max = 10, advanced)]
    pub truncate: f32,
    /// What the blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about
    /// the edge pixel), 'nearest' (the edge pixel repeated) or 'wrap' (the opposite edge).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The picture blurred by w x `radius` at each pixel, w the distance of its field from the sharp band.
#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "levels is from 1 to 16")]
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    // `Inputs::declared_by` refuses an offer that lacks a declared field, so this branch is out of reach: `render` cannot
    // return an error, and a field cannot be read without an Option
    let Some(field) = inputs.field("field") else {
        return Image::from_pixels(picture.height(), picture.width(), picture.pixels().to_vec());
    };
    let half_band = params.width / 2.0;
    let weights: Vec<f32> = field
        .values()
        .iter()
        .map(|value| (((value - params.focus).abs() - half_band) / params.feather).clamp(0.0, 1.0))
        .collect();
    let blurred = blur::graded_blur(
        picture,
        &weights,
        f64::from(params.radius),
        params.levels as u8,
        f64::from(params.truncate),
        params.edge.extension(),
    );
    let pixels = blurred.into_iter().map(|pixel| pixel.map(|value| value.clamp(0.0, 1.0)));
    Image::from_pixels(picture.height(), picture.width(), pixels.collect())
}
