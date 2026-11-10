//! `reflection`: the picture mirrored in a floor under a shape, fading with distance. What it does is documented once, on
//! `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::blur::gaussian_filter;
use crate::ops::edge::Edge;
use crate::params::{FieldName, Params};
use crate::{Image, Inputs, Picture};

/// Where `reflection` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Subject,
    opacity: 0.6, // constant: the default opacity of a step: a floor mirrors partway, its own surface showing
    inputs: &[],
};

/// Whole: the floor line is the lowest row of the shape anywhere in the picture.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// A floor mirror: the picture above a shape's lowest row is mirrored about it, softened and fading with depth.
///
/// The floor line is the bottom of the shape; the reflection is `height` of the shape's height deep, blurred by `blur`,
/// a perfect mirror at the floor line that is mixed into the floor below by less and less, down to 1 - `fade` at its
/// end. It stops at the picture's bottom edge, and the picture above the floor line is never changed.
///
/// When nothing in the field counts as the shape (see `field_threshold`), there is no floor line and so no reflection:
/// the picture comes back unchanged.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The shape whose lowest row is the floor line: `subject` (the subject's silhouette), `light`, `depth`, or a region
    /// of the recipe (text, a logo, a drawn shape). A row belongs to it from `field_threshold` up.
    #[param(default = "subject")]
    pub field: FieldName,
    /// How tall the reflection is, as a fraction of the shape's height: 0.4 = 40% of it is mirrored into the floor below
    /// the shape's lowest row, 1 = all of it; 0 = no reflection. Where the picture ends first, the reflection is cut
    /// there.
    #[param(default = 0.4, unit = fraction, min = 0, max = 1)]
    pub height: f32,
    /// How much of its strength the reflection loses between the floor line and its end, in a straight line: 0 = as
    /// strong at the end as at the floor line, 1 = fades out completely at its end.
    #[param(default = 0.7, unit = fraction, min = 0, max = 1)]
    pub fade: f32,
    /// Gaussian sigma of the reflection's softness in px at the recipe's size: raising it makes a blurrier mirror, as on
    /// a rough floor; 0 = a sharp mirror image.
    #[param(default = 2.0, unit = px, min = 0, max = 100)]
    pub blur: f32,
    /// The field's value, 0 to 1, that some pixel of a row must reach for the row to count as part of the shape when its
    /// lowest and highest rows are found: 0.5 = a pixel of at least half. Lowering it counts faint fringes, so the floor
    /// line moves down and the shape's height grows.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1, advanced)]
    pub field_threshold: f32,
    /// Where the blur's Gaussian is cut off, in sigmas: it reaches truncate x blur px. Larger is more exact and slower.
    #[param(default = 4.0, unit = ratio, min = 1, max = 10, advanced)]
    pub truncate: f32,
    /// What the blur sees beyond the picture's edge, and beyond the floor line, where the reflection is mirrored:
    /// 'reflect' (mirrored, edge pixel repeated, which continues the mirror image), 'mirror' (mirrored about the edge
    /// pixel), 'nearest' (the edge pixel repeated) or 'wrap' (the opposite edge).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The picture with the reflection mixed into the floor below the shape.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let mut pixels = picture.pixels().to_vec();
    // `Inputs::declared_by` refuses an offer that lacks a declared field, so this branch is out of reach: `render` cannot
    // return an error, and a field cannot be read without an Option
    let Some(field) = inputs.field("field") else {
        return Image::from_pixels(height, width, pixels);
    };
    let in_shape = |row: &[f32]| row.iter().any(|&value| value >= params.field_threshold);
    let mut rows = field.values().chunks_exact(width);
    let (Some(top), Some(bottom)) = (rows.clone().position(in_shape), rows.rposition(in_shape)) else {
        return Image::from_pixels(height, width, pixels);
    };
    let floor = bottom + 1; // the first row below the shape
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a count of rows, far below 2**24, and a share of it rounded to a whole, at least 0"
    )]
    let reach = (params.height * (floor - top) as f32).round_ties_even() as usize; // rows it would have, with room enough
    let depth = reach.min(height - floor); // rows it has: it is cut where the picture ends
    if depth == 0 {
        return Image::from_pixels(height, width, pixels);
    }
    // the picture above the floor line, blurred as a picture of its own: beyond its last row lies the edge rule's mirror
    let mirror = gaussian_filter(
        &picture.top(floor),
        f64::from(params.blur),
        f64::from(params.truncate),
        params.edge.extension(),
    );
    for below in 0..depth {
        #[expect(clippy::cast_precision_loss, reason = "a count of rows, far below 2**24")]
        let strength = 1.0 - params.fade * below as f32 / reach as f32;
        // the row just above the floor line is mirrored to the row just below it
        let mirrored = &mirror[(floor - 1 - below) * width..][..width];
        for (pixel, image) in pixels[(floor + below) * width..][..width].iter_mut().zip(mirrored) {
            *pixel = [0, 1, 2].map(|channel| (1.0 - strength) * pixel[channel] + strength * image[channel]);
        }
    }
    Image::from_pixels(height, width, pixels)
}
