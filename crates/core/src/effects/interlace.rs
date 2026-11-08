//! `interlace`: every other row slid sideways, as two fields of an interlaced frame that fell out of step. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::edge::Edge;
use crate::ops::shift::shifted;
use crate::params::{Choice, Params};
use crate::{Image, Inputs, Picture};

/// Where `interlace` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Glitch,
    opacity: 1.0,
    inputs: &[],
};

/// Which of the two fields of rows slides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Choice)]
pub enum Field {
    /// The second, fourth, sixth... row, counting from the top as the first.
    Odd,
    /// The first, third, fifth... row.
    Even,
}

/// Slide every other row of the picture sideways by `offset` px, leaving the rows between untouched.
///
/// The two fields of an interlaced frame are the odd and the even rows; when the picture moved between them, every
/// vertical edge breaks into a comb. Here one field (`field`) is moved by `offset` and the other stays, which gives that
/// comb on a still picture. What the slide uncovers at the side is set by `edge`.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How far the slid field moves sideways, -500 to 500 px, tuned for a picture about 480 px high: positive moves its
    /// rows right, negative left, 0 leaves the picture unchanged. A distance between two pixels blends them. Raising it
    /// widens the teeth of the comb along every vertical edge.
    #[param(default = 6.0, unit = px, min = -500, max = 500)]
    pub offset: f32,
    /// Which field slides: 'odd' the second, fourth, sixth... row (counting from the top as the first), 'even' the first,
    /// third, fifth... row. The other field stays where it was.
    #[param(default = Odd)]
    pub field: Field,
    /// What fills the gap the slide opens at the side: 'nearest' holds the row's last pixel there, 'wrap' brings back what
    /// left on the other side, 'reflect' mirrors the row (the edge pixel repeated once), 'mirror' mirrors it about the edge
    /// pixel.
    #[param(default = Nearest, advanced)]
    pub edge: Edge,
}

/// The picture with the rows of one field slid sideways.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    // constant: no vertical distance, the rows only slide sideways
    let moved = shifted(
        picture.pixels(),
        height,
        width,
        0.0,
        f64::from(params.offset),
        params.edge.extension(),
    );
    // the rows of the slid field have this remainder of the row index: the odd ones (second, fourth...) 1, the even ones 0
    let slid = usize::from(params.field == Field::Odd);
    picture.map_at(|row, column, pixel| {
        if row % 2 == slid {
            moved[row * width + column]
        } else {
            pixel
        }
    })
}
