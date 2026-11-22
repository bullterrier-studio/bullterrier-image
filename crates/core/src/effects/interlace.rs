//! `interlace`: every other row slid sideways, as two fields of an interlaced frame that fell out of step. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Input, Meta};
use crate::ops::edge::Edge;
use crate::ops::shift::shifted;
use crate::params::{Choice, Params};
use crate::{Image, Inputs, Picture};

/// Where `interlace` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Glitch,
    opacity: 1.0,
    inputs: &[Input::Place],
};

/// What `interlace` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Pixel
}

/// How far `interlace` paints beyond the picture's edge: not worked out.
pub(crate) fn spread(_params: &Params) -> crate::Spread {
    crate::Spread::Unbounded
}

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
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    // constant: no vertical distance, the rows only slide sideways
    let moved = shifted(
        picture.pixels(),
        height,
        width,
        0, // no move down
        0.0,
        f64::from(params.offset),
        params.edge.extension(),
    );
    // the rows of the slid field have this remainder of the row index: the odd ones (second, fourth...) 1, the even ones 0
    let slid = usize::from(params.field == Field::Odd);
    // the fields are the canvas's rows, whichever of them the picture starts at
    let first_row = inputs.place(picture).first_row;
    picture.map_at(|row, column, pixel| {
        if (first_row + row) % 2 == slid {
            moved[row * width + column]
        } else {
            pixel
        }
    })
}

/// The slid field on the GPU (`interlace.wgsl`): the CPU's own reading, the source column's whole offset and the share of
/// the next pixel worked out once in f64.
#[cfg(feature = "gpu")]
pub(crate) fn kernel() -> crate::gpu::Kernel<Params> {
    crate::gpu::Kernel::new(
        include_str!("interlace.wgsl"),
        |params: &Params, _: &crate::gpu::Header| {
            // the column a pixel reads is its own less the offset
            let source = -f64::from(params.offset);
            let first = source.floor();
            let rule = match params.edge {
                crate::ops::edge::Edge::Reflect => 0,
                crate::ops::edge::Edge::Nearest => 1,
                crate::ops::edge::Edge::Mirror => 2,
                crate::ops::edge::Edge::Wrap => 3,
            };
            let int = |value: i64| f32::from_bits(i32::try_from(value).unwrap_or(i32::MAX).cast_unsigned());
            #[expect(
                clippy::cast_possible_truncation,
                reason = "an offset is at most a few thousand pixels"
            )]
            let whole = first as i64;
            vec![
                [crate::ops::cast::single(source - first), 0.0, 0.0, 0.0],
                [int(whole), int(rule), int(i64::from(params.field == Field::Odd)), 0.0],
            ]
        },
    )
}
