//! `drop_shadow`: copies of a shape behind it: a shadow, paper layers, a long shadow. What it does is documented once, on
//! `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::blur::gaussian_filter_owned;
use crate::ops::edge::Edge;
use crate::ops::shift::shifted;
use crate::params::{Color, FieldName, Params};
use crate::{Image, Inputs, Picture};

/// Where `drop_shadow` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Subject,
    opacity: 0.6, // constant: the default opacity of a step: a shadow mixed in partway lets the floor show through
    inputs: &[],
};

/// What `drop_shadow` reads around a pixel: the shape as far as the farthest copy lies, softened by the blur's reach.
pub(crate) fn footprint(params: &Params) -> Footprint {
    // constant: the Gaussian reaches int(truncate x blur + 0.5) px, and a copy moved by a fraction reads one row more
    Footprint::reach(
        f64::from(params.copies) * f64::from(params.offset) + f64::from(params.blur * params.truncate) + 1.5,
        params.edge == Edge::Wrap,
    )
}

/// How far `drop_shadow` paints beyond the picture's edge: the farthest copy's move, the way it moves, and the blur's reach
/// on every side.
pub(crate) fn spread(params: &Params) -> crate::Spread {
    if params.edge == Edge::Wrap {
        return crate::Spread::Unbounded;
    }
    let heading = f64::from(params.angle.rem_euclid(360.0)).to_radians(); // constant: a full turn in degrees
    let far = f64::from(params.copies) * f64::from(params.offset);
    // constant: the Gaussian reaches int(truncate x blur + 0.5) px
    let blur = f64::from(params.blur * params.truncate) + 0.5;
    crate::Spread::moved(far * heading.sin(), far * heading.cos()).then(crate::Spread::all(blur))
}

/// Copies of a shape (the subject by default) behind it, each `offset` px farther along `angle` and softened by `blur`:
/// a drop shadow, layered paper cutouts or a long shadow.
///
/// One copy is a drop shadow. More, with `blur` 0 and a `color` to `end_color` ramp, are layers of cut paper; many at a
/// small `offset` in one colour with `fade` are a long shadow (preset `long_shadow`). The copies run in colour from
/// `color` (nearest the shape) to `end_color` (farthest); a nearer copy lies over the ones beyond it, each at
/// `copy_opacity` falling by `fade`, and the shape on top of all of them, so the shape itself is never changed.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The shape that casts the copies: `subject` (the subject's silhouette), `light`, `depth`, or a region of the recipe
    /// (text, a logo, a drawn shape), its value 0..1 how much of a pixel belongs to it.
    #[param(default = "subject")]
    pub field: FieldName,
    /// How many copies of the shape are stacked behind it: 1 = a single shadow, more = a deeper stack of layers.
    #[param(default = 1, unit = count, min = 1, max = 400)]
    pub copies: i32,
    /// How far each copy lies beyond the one before it, in px, tuned for a picture about 480 px high: the first is
    /// `offset` from the shape, the last `copies` x `offset`. 0 = every copy hidden behind the shape, leaving only what
    /// `blur` spreads past its edge.
    #[param(default = 22.0, unit = px, min = 0, max = 1000)]
    pub offset: f32,
    /// The direction the copies step away in, in degrees clockwise from pointing right on the screen, 0 to 360: 0 = to
    /// the right, 45 = to the lower right, 90 = down, 180 = to the left, 270 = up.
    #[param(default = 56.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub angle: f32,
    /// Gaussian sigma of each copy's soft edge in px: raising it makes the copies larger and fainter at their edge; 0 =
    /// hard-edged silhouettes.
    #[param(default = 24.0, unit = px, min = 0, max = 200)]
    pub blur: f32,
    /// The colour of the copy nearest the shape: '#rrggbb', '#rgb' or [r, g, b] 0..1. Black darkens what lies behind the
    /// shape; another colour tints it.
    #[param(default = "#000000")]
    pub color: Color,
    /// The colour of the farthest copy; the copies between step evenly from `color` to it. With one copy only `color` is
    /// used.
    #[param(default = "#000000")]
    pub end_color: Color,
    /// How much of what lies behind each copy it covers: 1 = fully, below 1 the copies show through each other and the
    /// picture; 0 = no copies. Overlapping copies compound (two copies at 0.5 cover 0.75); the step's `opacity` mixes
    /// the finished stack in once.
    #[param(default = 1.0, unit = fraction, min = 0, max = 1)]
    pub copy_opacity: f32,
    /// How much of `copy_opacity` the farthest copy loses, falling in a straight line along the copies: 0 = all copies
    /// alike, 0.5 = the farthest half as opaque, 1 = it vanishes. With one copy nothing fades.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1)]
    pub fade: f32,
    /// Where each copy's Gaussian is cut off, in sigmas: it reaches truncate x blur px. Larger is more exact and slower.
    #[param(default = 4.0, unit = ratio, min = 1, max = 10, advanced)]
    pub truncate: f32,
    /// What the copies see beyond the picture's edge, when moved and softened: 'nearest' (the outermost row repeated, so
    /// a shape cut off by the border casts its copies on without a gap), 'reflect' (mirrored, edge pixel repeated),
    /// 'mirror' (mirrored about the edge pixel) or 'wrap' (the opposite edge).
    #[param(default = Nearest, advanced)]
    pub edge: Edge,
}

/// The picture with the copies laid behind the shape.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    // `Inputs::declared_by` refuses an offer that lacks a declared field, so this branch is out of reach: `render` cannot
    // return an error, and a field cannot be read without an Option
    let Some(field) = inputs.field("field") else {
        return Image::from_pixels(height, width, picture.pixels().to_vec());
    };
    let shape = field.values();
    let edge = params.edge.extension();
    // the shape as a grey picture, which moves as a picture does
    let silhouette: Vec<[f32; 3]> = shape.iter().map(|&value| [value; 3]).collect();
    // (down, right) as seen in the picture; a whole turn is the same direction exactly
    let heading = f64::from(params.angle.rem_euclid(360.0)).to_radians(); // constant: a full turn in degrees
    let (down, right) = (
        f64::from(params.offset) * heading.sin(),
        f64::from(params.offset) * heading.cos(),
    );
    let copies = params.copies.unsigned_abs();
    // Laying the copies over the picture, the farthest first, is the same as walking them from the nearest: each takes
    // its opacity of what the nearer ones left clear, and its colour, which is `color` plus `along` of the way to
    // `end_color`, is summed as `color` and the `along`-weighted share, so each copy costs one-channel planes, not three.
    let mut clear = vec![1.0_f32; height * width];
    let mut along_share = vec![0.0_f32; height * width];
    for number in 1..=copies {
        #[expect(clippy::cast_precision_loss, reason = "at most 400 copies")]
        let along = (number - 1) as f32 / copies.saturating_sub(1).max(1) as f32; // 0 at the first copy, 1 at the last
        let opacity = params.copy_opacity * (1.0 - params.fade * along);
        let steps = f64::from(number);
        let moved = shifted(&silhouette, height, width, down * steps, right * steps, edge);
        let cover: Vec<f32> = moved.iter().map(|pixel| pixel[0]).collect();
        let cover = gaussian_filter_owned(
            cover,
            (height, width),
            f64::from(params.blur),
            f64::from(params.truncate),
            edge,
        );
        for ((clear, share), cover) in clear.iter_mut().zip(&mut along_share).zip(cover) {
            let taken = *clear * opacity * cover;
            *clear -= taken;
            *share += taken * along;
        }
    }
    let (near, far) = (params.color.0, params.end_color.0);
    picture.map_at(|row, column, pixel| {
        let at = row * width + column;
        let behind: [f32; 3] = std::array::from_fn(|channel| {
            pixel[channel] * clear[at]
                + near[channel] * (1.0 - clear[at])
                + (far[channel] - near[channel]) * along_share[at]
        });
        // the copies lie behind the shape: a pixel moves toward them by the share of it the shape does not cover
        std::array::from_fn(|channel| pixel[channel] + (1.0 - shape[at]) * (behind[channel] - pixel[channel]))
    })
}
