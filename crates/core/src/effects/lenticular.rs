//! `lenticular`: vertical strips, every other one slid sideways, like a lenticular print seen off-centre. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::edge::Edge;
use crate::ops::shift::shifted;
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `lenticular` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Distortion,
    opacity: 1.0,
    inputs: &[],
};

/// Cut the picture into vertical strips of `strip` px and slide every second one sideways by `shift` px, as a lenticular
/// print is seen off-centre.
///
/// The strips keep their places; what changes is what they show: the 1st, 3rd, ... strips show the picture as it is and
/// the 2nd, 4th, ... show the whole picture moved sideways by `shift`, so the two views are interlaced as under a
/// lenticular lens. A strip shows the moved picture at the strip's own columns.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The width of each vertical strip in pixels, counted from the left edge; a last strip at the right edge may be
    /// narrower. Narrow strips make a fine comb, wide ones a coarse stagger; a strip wider than the picture leaves it
    /// whole. Tuned for a picture about 480 px high.
    #[param(default = 6, unit = px, min = 1, max = 500)]
    pub strip: i32,
    /// How far every second strip (the 2nd, 4th, ... from the left) is slid sideways, in pixels: positive moves its
    /// picture right, negative left, 0 leaves the picture unchanged. A distance that is not a whole number blends the two
    /// pixels it lies between. Raising it separates the two interleaved views further.
    #[param(default = 5.0, unit = px, min = -500, max = 500)]
    pub shift: f32,
    /// What a slid strip shows where its picture has moved off the picture's side: 'nearest' (the outermost column
    /// repeated), 'wrap' (the opposite side comes round), 'reflect' (mirrored, edge column repeated), 'mirror' (mirrored
    /// about the edge column).
    #[param(default = Nearest, advanced)]
    pub edge: Edge,
}

/// The picture with every second strip showing the picture slid sideways.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    // constant: a strip slides sideways only, no move down
    let moved = shifted(
        picture.pixels(),
        height,
        width,
        0.0,
        f64::from(params.shift),
        params.edge.extension(),
    );
    #[expect(clippy::cast_sign_loss, reason = "strip is at least 1")]
    let strip = params.strip as usize;
    picture.map_at(|row, column, pixel| {
        if (column / strip) % 2 == 1 {
            moved[row * width + column]
        } else {
            pixel
        }
    })
}
